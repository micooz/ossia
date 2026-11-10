import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { readLock, verifyTree, refreshGoMtimes, sha256 } from './engine.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const APP_VERSION = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')).version;
const TARGETS = {
  'darwin-arm64': {
    triple: 'aarch64-apple-darwin',
    bundles: ['app'],
    platform: 'macos',
    artifact: 'macos-arm64',
  },
  'darwin-x86_64': {
    triple: 'x86_64-apple-darwin',
    bundles: ['app'],
    platform: 'macos',
    artifact: 'macos-x86_64',
  },
  'mingw-x86_64': {
    triple: 'x86_64-pc-windows-msvc',
    bundles: ['nsis'],
    platform: 'windows',
    artifact: 'windows-x86_64',
  },
  'linux-x86_64': {
    triple: 'x86_64-unknown-linux-gnu',
    bundles: ['deb', 'appimage'],
    platform: 'linux',
    artifact: 'linux-x86_64',
  },
  'linux-arm64': {
    triple: 'aarch64-unknown-linux-gnu',
    bundles: ['deb', 'appimage'],
    platform: 'linux',
    artifact: 'linux-arm64',
  },
};

export function specFor(target) {
  const spec = TARGETS[target];
  if (!spec) throw new Error(`unsupported package target: ${target}`);
  return spec;
}

export function assertBuildable(target, lock) {
  const entry = lock.targets ? lock.targets[target] : lock;
  if (!entry) throw new Error(`engine target is not locked: ${target}`);
  if (entry.target && entry.target !== target) throw new Error(`engine target mismatch: ${target}`);
  if (!entry.archive) {
    throw new Error(
      `${target} has no built engine archive; build and lock the native engine before packaging`,
    );
  }
  return entry;
}

function targetArg(argv) {
  return argv.find((arg) => arg.startsWith('--target='))?.slice('--target='.length) ?? hostTarget();
}

function hostTarget() {
  const target = `${process.platform === 'win32' ? 'mingw' : process.platform}-${process.arch === 'x64' ? 'x86_64' : process.arch}`;
  if (!TARGETS[target]) throw new Error(`unsupported host target: ${target}`);
  return target;
}

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: ROOT,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
    maxBuffer: 64 << 20,
    ...options,
  });
}

function clean() {
  return run('git', ['status', '--porcelain', '--untracked-files=all']);
}

function assertClean() {
  const status = clean();
  if (status.trim()) {
    throw new Error(
      'packaging requires a clean tracked tree so the source archive matches the app',
    );
  }
}

function licenseFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((name) => /^(licen[cs]e|copying|notice)/i.test(name))
    .filter((name) => fs.statSync(path.join(dir, name)).isFile())
    .sort();
}

function copyLicenseFiles(source, destination) {
  const files = licenseFiles(source);
  fs.mkdirSync(destination, { recursive: true });
  for (const name of files) fs.copyFileSync(path.join(source, name), path.join(destination, name));
  return files;
}

function rustDependencies(triple) {
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version', '1']));
  const byName = new Map();
  for (const pkg of metadata.packages) byName.set(`${pkg.name}@${pkg.version}`, pkg);
  const tree = run('cargo', [
    'tree',
    '--locked',
    '-e',
    'normal',
    '--target',
    triple,
    '--prefix',
    'none',
    '--format',
    '{p}',
  ]);
  const seen = new Set();
  const result = [];
  for (const line of tree.split(/\r?\n/)) {
    const match = line.trim().match(/^(\S+) v(\S+)/);
    if (!match) continue;
    const key = `${match[1]}@${match[2]}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const pkg = byName.get(key);
    if (pkg) result.push(pkg);
  }
  return result;
}

function collectRustLicenses(packages, destination) {
  const entries = [];
  for (const pkg of packages) {
    const directory = path.join(destination, `${pkg.name}@${pkg.version}`);
    const manifestDir = path.dirname(pkg.manifest_path);
    let files = pkg.license_file
      ? [path.resolve(manifestDir, pkg.license_file)].filter((file) => fs.existsSync(file))
      : licenseFiles(manifestDir).map((name) => path.join(manifestDir, name));
    files = [...new Set(files)];
    if (files.length) {
      fs.mkdirSync(directory, { recursive: true });
      for (const file of files) fs.copyFileSync(file, path.join(directory, path.basename(file)));
    }
    entries.push({
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
      repository: pkg.repository ?? pkg.homepage ?? null,
      source: pkg.source,
      licenseFiles: files.map((file) =>
        path.relative(destination, path.join(directory, path.basename(file))),
      ),
      missingLicenseText: files.length === 0,
    });
  }
  return entries;
}

function collectNpmLicenses(destination) {
  const groups = JSON.parse(run('pnpm', ['licenses', 'list', '--prod', '--json']));
  const entries = [];
  for (const [license, packages] of Object.entries(groups)) {
    for (const pkg of packages) {
      for (const packagePath of pkg.paths ?? []) {
        const directory = path.join(
          destination,
          `${pkg.name}@${pkg.versions?.join(',') || 'unknown'}`,
        );
        const files = copyLicenseFiles(packagePath, directory);
        entries.push({
          name: pkg.name,
          versions: pkg.versions ?? [],
          license,
          source: pkg.homepage ?? null,
          paths: pkg.paths ?? [],
          licenseFiles: files,
          missingLicenseText: files.length === 0,
        });
      }
    }
  }
  const tauriApi = path.join(ROOT, 'packages/ui/node_modules/@tauri-apps/api');
  if (!entries.some((entry) => entry.name === '@tauri-apps/api')) {
    const packageJson = JSON.parse(fs.readFileSync(path.join(tauriApi, 'package.json'), 'utf8'));
    const directory = path.join(destination, `${packageJson.name}@${packageJson.version}`);
    const files = copyLicenseFiles(tauriApi, directory);
    entries.push({
      name: packageJson.name,
      versions: [packageJson.version],
      license: packageJson.license,
      source: packageJson.homepage ?? null,
      paths: [tauriApi],
      licenseFiles: files,
      missingLicenseText: files.length === 0,
    });
  }
  return entries;
}

function notices(manifest) {
  const lines = [
    '# Third-party notices',
    '',
    `Ossia ${manifest.appVersion} (${manifest.target})`,
    '',
    'The LilyPond engine and its complete license tree are included under `licenses/engine/`.',
    'Ossia itself is GPL-3.0-or-later; the full license text is `licenses/Ossia-GPL-3.0-or-later.txt`.',
    '',
  ];
  const groups = new Map();
  for (const dep of manifest.npmDependencies) {
    const key = dep.license || 'unknown';
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(`- ${dep.name} ${dep.versions.join(', ')} (npm)`);
  }
  for (const dep of manifest.rustDependencies) {
    const key = dep.license || 'unknown';
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(`- ${dep.name} ${dep.version} (Rust)`);
  }
  for (const [license, packages] of [...groups.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    lines.push(`## ${license}`, '');
    lines.push(...packages.sort(), '');
  }
  const missing = [
    ...manifest.npmDependencies.filter((dep) => dep.missingLicenseText),
    ...manifest.rustDependencies.filter((dep) => dep.missingLicenseText),
  ];
  if (missing.length) {
    lines.push('## Missing local license text', '');
    lines.push(
      'These packages declare a license but do not ship a local license file. Their SPDX identifiers and sources are recorded in `SOURCE-MANIFEST.json`.',
      '',
    );
    for (const dep of missing) lines.push(`- ${dep.name} (${dep.license || 'unknown'})`);
    lines.push('');
  }
  return `${lines.join('\n')}\n`;
}

function materials(stage, target, entry, sourceArchive) {
  const destination = path.join(stage, 'materials');
  const licenses = path.join(destination, 'licenses');
  fs.mkdirSync(licenses, { recursive: true });
  fs.copyFileSync(path.join(ROOT, 'LICENSE'), path.join(licenses, 'Ossia-NOTICE.txt'));
  fs.copyFileSync(
    path.join(
      ROOT,
      'vendor/lilypond',
      `${entry.version}-${target}`,
      'licenses',
      'lilypond-2.26.0.COPYING',
    ),
    path.join(licenses, 'Ossia-GPL-3.0-or-later.txt'),
  );
  const engineLicenses = path.join(licenses, 'engine');
  fs.mkdirSync(engineLicenses, { recursive: true });
  fs.cpSync(
    path.join(ROOT, 'vendor/lilypond', `${entry.version}-${target}`, 'licenses'),
    engineLicenses,
    {
      recursive: true,
    },
  );
  const rustPackages = rustDependencies(specFor(target).triple);
  const rust = collectRustLicenses(
    rustPackages.filter((pkg) => pkg.source !== null),
    path.join(licenses, 'rust'),
  );
  const workspacePackages = rustPackages
    .filter((pkg) => pkg.source === null)
    .map((pkg) => ({ name: pkg.name, version: pkg.version }));
  const npm = collectNpmLicenses(path.join(licenses, 'npm'));
  const engineLock = JSON.parse(fs.readFileSync(path.join(ROOT, 'vendor/ENGINE.lock'), 'utf8'));
  const engineSource =
    engineLock.targets[target]?.source ?? engineLock.targets['linux-arm64']?.source;
  const dependencyLock = JSON.parse(
    fs.readFileSync(path.join(ROOT, 'vendor/ENGINE.dependencies.json'), 'utf8'),
  );
  const manifest = {
    schemaVersion: 1,
    appVersion: APP_VERSION,
    target,
    commit: run('git', ['rev-parse', 'HEAD']).trim(),
    engine: {
      version: entry.version,
      engineBuildId: entry.engineBuildId,
      archive: entry.archive,
      source: engineSource ?? null,
      dependencyLock: {
        path: 'vendor/ENGINE.dependencies.json',
        sha256: sha256(path.join(ROOT, 'vendor/ENGINE.dependencies.json')),
        packages: dependencyLock.packages.map((item) => ({
          file: item.file,
          url: item.url,
          sha256: item.sha256,
        })),
        buildScripts: dependencyLock.buildScripts.map((item) => item.path),
      },
    },
    sourceArchive: {
      file: path.basename(sourceArchive),
      sha256: sha256(sourceArchive),
    },
    workspacePackages,
    rustDependencies: rust,
    npmDependencies: npm,
    sourceReferences: [
      'LilyPond 2.26.0 and its dependency archives are referenced by checksum in vendor/ENGINE.lock and vendor/ENGINE.dependencies.json.',
      'Rust registry sources are identified by cargo metadata and copied license files where available.',
      'npm production sources are identified by pnpm licenses list.',
    ],
  };
  const manifestPath = path.join(destination, 'SOURCE-MANIFEST.json');
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  fs.writeFileSync(path.join(destination, 'THIRD-PARTY-NOTICES.md'), notices(manifest));
  fs.copyFileSync(manifestPath, path.join(licenses, 'SOURCE-MANIFEST.json'));
  fs.copyFileSync(
    path.join(destination, 'THIRD-PARTY-NOTICES.md'),
    path.join(licenses, 'THIRD-PARTY-NOTICES.md'),
  );
  return { destination, licenses, manifest };
}

function buildUI() {
  run('pnpm', ['--filter', '@ossia/ui', 'build'], { stdio: 'inherit' });
}

function overlay(stage, target, entry) {
  const spec = specFor(target);
  const engine = path.join(ROOT, 'vendor/lilypond', `${entry.version}-${target}`);
  const ui = path.join(ROOT, 'packages/ui/dist');
  const integrations = path.join(ROOT, 'integrations');
  const licenses = path.join(stage, 'materials/licenses');
  const config = {
    bundle: {
      resources: {
        [`${engine}${path.sep}`]: 'engine',
        [`${ui}${path.sep}`]: 'ui',
        [`${integrations}${path.sep}`]: 'integrations',
        [`${licenses}${path.sep}`]: 'licenses',
      },
    },
  };
  const file = path.join(stage, 'tauri.package.conf.json');
  fs.writeFileSync(file, `${JSON.stringify(config, null, 2)}\n`);
  return { file, spec };
}

function findApp(target) {
  const triple = specFor(target).triple;
  const candidates = [
    path.join(ROOT, 'target', triple, 'release/bundle/macos/Ossia.app'),
    path.join(ROOT, 'target/release/bundle/macos/Ossia.app'),
  ];
  const app = candidates.find((candidate) => fs.existsSync(candidate));
  if (!app) throw new Error(`built app not found for ${target}`);
  return app;
}

function appExecutable(app) {
  const dir = path.join(app, 'Contents/MacOS');
  const name = fs.readdirSync(dir).find((entry) => {
    const stat = fs.statSync(path.join(dir, entry));
    return stat.isFile() && (stat.mode & 0o111) !== 0;
  });
  if (!name) throw new Error('app executable not found');
  return path.join(dir, name);
}

async function normalizeEngine(app, target, entry) {
  const engine = path.join(app, 'Contents/Resources/engine');
  const inventory = JSON.parse(fs.readFileSync(path.join(engine, 'resources.json'), 'utf8'));
  if (inventory.engineBuildId !== entry.engineBuildId || inventory.target !== target) {
    throw new Error('bundled engine inventory does not match the lock');
  }
  for (const file of inventory.files) {
    const absolute = path.join(engine, file.path);
    const stat = fs.statSync(absolute);
    if (stat.size !== file.bytes || sha256(absolute) !== file.sha256) {
      throw new Error(`bundled engine resource is damaged: ${file.path}`);
    }
    if (file.executable && process.platform !== 'win32' && (stat.mode & 0o111) === 0) {
      fs.chmodSync(absolute, stat.mode | 0o755);
    }
  }
  await refreshGoMtimes(engine);
  verifyTree(engine, readLock(ROOT, target), target === hostTarget());
}

async function freePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const port = server.address().port;
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}

function wait(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(check, timeout = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeout) {
    try {
      if (await check()) return;
    } catch {}
    await wait(200);
  }
  throw new Error('timed out waiting for packaged runtime');
}

async function verifyApp(app, home) {
  const executable = appExecutable(app);
  const doctor = JSON.parse(run(executable, ['--home', home, 'doctor']));
  if (!doctor.engine?.ok)
    throw new Error(`bundled doctor failed: ${JSON.stringify(doctor.engine)}`);
  const port = await freePort();
  const child = spawn(executable, ['--home', home, '--port', String(port), 'serve'], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  try {
    await waitFor(async () => {
      const response = await fetch(`http://127.0.0.1:${port}/`);
      return response.ok && (await response.text()).includes('Ossia');
    });
    const status = JSON.parse(run(executable, ['--home', home, 'status']));
    if (!status.runtime?.webBaseUrl) throw new Error('packaged runtime status failed');
    const gui = spawn(executable, ['--home', home, '--port', String(port)], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    await wait(1500);
    if (gui.exitCode !== null) throw new Error('management app exited during launch');
    gui.kill('SIGTERM');
    await waitFor(async () => {
      const current = JSON.parse(run(executable, ['--home', home, 'status']));
      return Boolean(current.runtime?.instanceId === status.runtime.instanceId);
    });
  } finally {
    try {
      run(executable, ['--home', home, 'stop']);
    } catch {}
    child.kill('SIGTERM');
  }
}

function dmg(root, app, destination) {
  const staging = path.join(root, 'dmg-root');
  try {
    fs.rmSync(staging, { recursive: true, force: true });
    fs.mkdirSync(staging, { recursive: true });
    run('ditto', [app, path.join(staging, 'Ossia.app')]);
    fs.symlinkSync('/Applications', path.join(staging, 'Applications'));
    run('hdiutil', [
      'create',
      '-volname',
      'Ossia',
      '-srcfolder',
      staging,
      '-ov',
      '-format',
      'UDZO',
      destination,
    ]);
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

async function verifyDmg(dmgPath, home) {
  const mount = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-dmg-'));
  try {
    run('hdiutil', ['attach', '-nobrowse', '-readonly', '-mountpoint', mount, dmgPath]);
    await verifyApp(path.join(mount, 'Ossia.app'), home);
  } finally {
    try {
      run('hdiutil', ['detach', mount, '-quiet', '-force']);
    } catch {}
    fs.rmSync(mount, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
}

function checksums(files, destination) {
  const root = path.dirname(destination);
  const lines = files
    .filter((file) => fs.statSync(file).isFile())
    .sort()
    .map((file) => `${sha256(file)}  ${path.relative(root, file)}`);
  fs.writeFileSync(destination, `${lines.join('\n')}\n`);
}

export async function packageTarget(target, options = {}) {
  const lock = readLock(ROOT, target);
  const entry = assertBuildable(target, lock);
  const spec = specFor(target);
  if (!options.allowDirty) assertClean();
  const engine = path.join(ROOT, 'vendor/lilypond', `${entry.version}-${target}`);
  verifyTree(engine, lock, target === hostTarget());
  if (!fs.existsSync(path.join(ROOT, 'packages/ui/dist/index.html'))) buildUI();
  const stage = path.join(ROOT, 'target/package', target);
  fs.rmSync(stage, { recursive: true, force: true });
  fs.mkdirSync(stage, { recursive: true });
  const sourceArchive = path.join(stage, `ossia-${APP_VERSION}-source.tar.gz`);
  run('git', [
    'archive',
    '--format=tar.gz',
    `--prefix=ossia-${APP_VERSION}/`,
    '-o',
    sourceArchive,
    'HEAD',
  ]);
  const material = materials(stage, target, entry, sourceArchive);
  const config = overlay(stage, target, entry);
  const tauri = path.join(ROOT, 'packages/desktop/node_modules/.bin/tauri');
  run(
    tauri,
    [
      'build',
      '--target',
      spec.triple,
      '--bundles',
      spec.bundles.join(','),
      '--no-sign',
      '--config',
      config.file,
    ],
    { cwd: path.join(ROOT, 'packages/desktop'), stdio: 'inherit' },
  );

  const artifactDir = path.join(ROOT, 'artifacts', spec.artifact);
  fs.rmSync(artifactDir, { recursive: true, force: true });
  fs.mkdirSync(artifactDir, { recursive: true });
  const files = [];
  if (spec.platform === 'macos') {
    const app = findApp(target);
    await normalizeEngine(app, target, entry);
    const installed = path.join(artifactDir, 'Ossia.app');
    run('ditto', [app, installed]);
    const home = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-package-home-'));
    try {
      await verifyApp(installed, home);
    } finally {
      fs.rmSync(home, { recursive: true, force: true });
    }
    const zip = path.join(artifactDir, `Ossia_${APP_VERSION}_${spec.triple}.app.zip`);
    run('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', installed, zip]);
    files.push(zip);
    const disk = path.join(artifactDir, `Ossia_${APP_VERSION}_${spec.triple}.dmg`);
    dmg(artifactDir, installed, disk);
    const verifyHome = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-dmg-home-'));
    try {
      await verifyDmg(disk, verifyHome);
    } finally {
      fs.rmSync(verifyHome, { recursive: true, force: true });
    }
    files.push(disk);
  } else {
    const bundleRoot = path.join(ROOT, 'target', spec.triple, 'release/bundle');
    for (const bundle of fs.existsSync(bundleRoot) ? fs.readdirSync(bundleRoot) : []) {
      const directory = path.join(bundleRoot, bundle);
      if (!fs.statSync(directory).isDirectory()) continue;
      for (const name of fs.readdirSync(directory)) {
        const source = path.join(directory, name);
        if (fs.statSync(source).isFile()) {
          const destination = path.join(artifactDir, name);
          fs.copyFileSync(source, destination);
          files.push(destination);
        }
      }
    }
  }
  fs.cpSync(material.destination, path.join(artifactDir, 'materials'), { recursive: true });
  fs.copyFileSync(sourceArchive, path.join(artifactDir, 'materials', path.basename(sourceArchive)));
  files.push(path.join(artifactDir, 'materials', path.basename(sourceArchive)));
  const release = {
    schemaVersion: 1,
    appVersion: APP_VERSION,
    target,
    triple: spec.triple,
    commit: run('git', ['rev-parse', 'HEAD']).trim(),
    engineBuildId: entry.engineBuildId,
    engineArchive: entry.archive,
    signing: spec.platform === 'macos' ? 'ad-hoc-linker-signed' : 'unsigned',
    notarized: false,
    verification:
      spec.platform === 'macos' && target === hostTarget() ? 'native-macos-bundle' : 'build-only',
    artifacts: files.map((file) => path.relative(artifactDir, file)),
  };
  fs.writeFileSync(path.join(artifactDir, 'release.json'), `${JSON.stringify(release, null, 2)}\n`);
  checksums(
    [...files, path.join(artifactDir, 'release.json')],
    path.join(artifactDir, 'SHA256SUMS'),
  );
  return { artifactDir, release, materials: material.manifest };
}

async function main(argv) {
  const target = targetArg(argv);
  const result = await packageTarget(target, {
    allowDirty: argv.includes('--allow-dirty'),
  });
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main(process.argv.slice(2)).catch((error) => {
    console.error(error.stack || error.message);
    process.exitCode = 1;
  });
}

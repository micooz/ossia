import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const TAG_PATTERN = /^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-beta\.(?:0|[1-9]\d*))?)$/;

function parseSectionVersion(toml, section) {
  let current = false;
  let version = null;

  for (const line of toml.split('\n')) {
    const trimmed = line.trim();
    if (/^\[[^\]]+\]$/.test(trimmed)) {
      current = trimmed === `[${section}]`;
      continue;
    }
    if (!current || version !== null || !/^version\s*=/.test(trimmed)) continue;
    const match = trimmed.match(/^version\s*=\s*"([^"]+)"/);
    if (!match) throw new Error(`${section} version is not a TOML string`);
    version = match[1];
  }

  if (version === null) throw new Error(`Cargo.toml has no version in [${section}]`);
  return version;
}

function replaceSectionVersion(toml, section, version) {
  const lines = toml.split('\n');
  let current = false;
  let replaced = false;

  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (/^\[[^\]]+\]$/.test(trimmed)) {
      current = trimmed === `[${section}]`;
      continue;
    }
    if (current && !replaced && /^version\s*=/.test(trimmed)) {
      lines[index] = `version = "${version}"`;
      replaced = true;
    }
  }

  if (!replaced) throw new Error(`Cargo.toml has no version to replace in [${section}]`);
  return lines.join('\n');
}

/** Whether a tag uses the supported stable or numbered beta release format. */
export function isReleaseTag(tag) {
  return typeof tag === 'string' && TAG_PATTERN.test(tag);
}

/** Parse a stable or numbered beta release tag into its manifest version. */
export function parseTag(tag) {
  const match = String(tag).match(TAG_PATTERN);
  if (!match) {
    throw new Error(
      `release tag must be vMAJOR.MINOR.PATCH or vMAJOR.MINOR.PATCH-beta.N, got ${JSON.stringify(tag)}`,
    );
  }
  return match[1];
}

/** Whether a valid release tag is a beta that must not become Latest. */
export function isPrereleaseTag(tag) {
  return parseTag(tag).includes('-beta.');
}

/** Map SemVer beta tags to RPM Version/Release without changing the binary version. */
export function rpmBundleConfig(tag) {
  const [version, beta] = parseTag(tag).split('-beta.');
  const release = beta === undefined ? '1' : `0.beta.${beta}.1`;
  return { version, bundle: { linux: { rpm: { release } } } };
}

/** Compare supported release tags using SemVer precedence. */
export function isNewerTag(candidate, current) {
  const [nextCore, nextBeta] = parseTag(candidate).split('-beta.');
  const [latestCore, latestBeta] = parseTag(current).split('-beta.');
  const next = nextCore.split('.').map(BigInt);
  const latest = latestCore.split('.').map(BigInt);

  for (let index = 0; index < 3; index += 1) {
    if (next[index] !== latest[index]) return next[index] > latest[index];
  }
  if (nextBeta === undefined) return latestBeta !== undefined;
  return latestBeta !== undefined && BigInt(nextBeta) > BigInt(latestBeta);
}

async function readJson(path, label) {
  const value = JSON.parse(await readFile(path, 'utf8'));
  if (typeof value.version !== 'string') throw new Error(`${label} has no string version`);
  return value;
}

async function replaceJsonVersion(path, version) {
  const text = await readFile(path, 'utf8');
  const pattern = /("version"\s*:\s*")[^"]*(")/;
  if (!pattern.test(text)) throw new Error(`${path} has no JSON version`);
  await writeFile(path, text.replace(pattern, `$1${version}$2`));
}

export async function readVersions(root) {
  const cargo = await readFile(join(root, 'Cargo.toml'), 'utf8');
  const packageJson = await readJson(join(root, 'package.json'), 'package.json');
  const desktopPackage = await readJson(
    join(root, 'packages/desktop/package.json'),
    'packages/desktop/package.json',
  );
  const tauriConfig = await readJson(
    join(root, 'packages/desktop/src-tauri/tauri.conf.json'),
    'tauri.conf.json',
  );
  return {
    cargo: parseSectionVersion(cargo, 'workspace.package'),
    packageJson: packageJson.version,
    desktopPackage: desktopPackage.version,
    tauriConfig: tauriConfig.version,
  };
}

export async function setVersion(root, tag, options = {}) {
  const version = parseTag(tag);
  const cargoPath = join(root, 'Cargo.toml');
  const packagePath = join(root, 'package.json');
  const desktopPath = join(root, 'packages/desktop/package.json');
  const tauriPath = join(root, 'packages/desktop/src-tauri/tauri.conf.json');

  const cargo = await readFile(cargoPath, 'utf8');
  await writeFile(cargoPath, replaceSectionVersion(cargo, 'workspace.package', version));
  await replaceJsonVersion(packagePath, version);
  await replaceJsonVersion(desktopPath, version);
  await replaceJsonVersion(tauriPath, version);

  if (options.refreshLock !== false) {
    execFileSync('cargo', ['metadata', '--format-version', '1'], {
      cwd: root,
      stdio: 'ignore',
    });
  }
  return version;
}

export async function checkVersion(root, tag) {
  const expected = parseTag(tag);
  const versions = await readVersions(root);
  for (const [name, actual] of Object.entries(versions)) {
    if (actual !== expected) {
      throw new Error(`${name} version is ${actual}, expected ${expected}`);
    }
  }
  return expected;
}

async function main(argv) {
  const [command, first, second] = argv;
  const root = process.cwd();

  switch (command) {
    case 'parse':
      console.log(parseTag(first));
      return;
    case 'prerelease':
      console.log(isPrereleaseTag(first));
      return;
    case 'rpm-config':
      console.log(JSON.stringify(rpmBundleConfig(first)));
      return;
    case 'set':
      console.log(await setVersion(root, first));
      return;
    case 'check':
      console.log(await checkVersion(root, first));
      return;
    case 'newer':
      if (!isNewerTag(first, second)) throw new Error(`${first} is not newer than ${second}`);
      return;
    default:
      throw new Error(
        'usage: version.mjs <parse|prerelease|rpm-config|set|check|newer> <tag> [previous-tag]',
      );
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release version: ${error.message}`);
    process.exitCode = 1;
  });
}

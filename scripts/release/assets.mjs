import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, cp, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { parseTag } from './version.mjs';

/** Installer names uploaded for every release, before SHA256SUMS. */
export const INSTALL_ASSETS = [
  'ossia-darwin-arm64.dmg',
  'ossia-darwin-amd64.dmg',
  'ossia-darwin-arm64.zip',
  'ossia-darwin-amd64.zip',
  'ossia-windows-amd64.exe',
  'ossia-linux-amd64.deb',
  'ossia-linux-amd64.rpm',
];

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/** Normalize a Tauri output filename to the public platform/architecture contract. */
export function assetName(source) {
  const name = basename(source);
  if (INSTALL_ASSETS.includes(name)) return name;

  if (name.endsWith('.dmg')) {
    if (/_(aarch64|arm64)(?:-apple-darwin)?\.dmg$/i.test(name)) {
      return 'ossia-darwin-arm64.dmg';
    }
    if (/_(x64|x86_64|amd64)(?:-apple-darwin)?\.dmg$/i.test(name)) {
      return 'ossia-darwin-amd64.dmg';
    }
  }
  if (name.endsWith('.app.zip')) {
    if (/_(aarch64|arm64)(?:-apple-darwin)?\.app\.zip$/i.test(name)) {
      return 'ossia-darwin-arm64.zip';
    }
    if (/_(x64|x86_64|amd64)(?:-apple-darwin)?\.app\.zip$/i.test(name)) {
      return 'ossia-darwin-amd64.zip';
    }
  }
  if (name.endsWith('.exe')) {
    if (/_(x64|x86_64|amd64)(?:-pc-windows-msvc)?-setup\.exe$/i.test(name)) {
      return 'ossia-windows-amd64.exe';
    }
  }
  if (name.endsWith('.deb')) {
    if (/_(x64|x86_64|amd64)\.deb$/i.test(name)) return 'ossia-linux-amd64.deb';
  }
  if (name.endsWith('.rpm')) {
    if (/\.(x86_64|amd64)\.rpm$/i.test(name)) return 'ossia-linux-amd64.rpm';
  }
  throw new Error(`unsupported release asset ${name}`);
}

async function filesUnder(root) {
  const output = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) output.push(...(await filesUnder(path)));
    else if (entry.isFile()) output.push(path);
  }
  return output;
}

async function artifactDirectories(input) {
  const entries = await readdir(input, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isDirectory() && entry.name.startsWith('build-'))
    .map((entry) => ({ target: entry.name.slice('build-'.length), dir: join(input, entry.name) }))
    .sort((a, b) => a.target.localeCompare(b.target));
}

async function collectInstallAssets(input, output) {
  await mkdir(output, { recursive: true });
  const collected = new Map();
  const sources = [];

  for (const artifact of await artifactDirectories(input)) {
    for (const source of await filesUnder(artifact.dir)) {
      const name = basename(source);
      if (/^ossia-.*-source\.tar\.gz$/.test(name)) {
        sources.push(source);
        continue;
      }
      if (!/\.(dmg|zip|exe|deb|rpm)$/i.test(name)) continue;
      const publicName = assetName(source);
      if (collected.has(publicName)) {
        throw new Error(`two source files map to ${publicName}`);
      }
      collected.set(publicName, source);
    }
  }

  for (const name of INSTALL_ASSETS) {
    if (!collected.has(name)) throw new Error(`missing release asset ${name}`);
    await copyFile(collected.get(name), join(output, name));
  }
  if (collected.size !== INSTALL_ASSETS.length) {
    throw new Error(`expected ${INSTALL_ASSETS.length} install assets, got ${collected.size}`);
  }
  if (sources.length === 0) throw new Error('missing source archive');

  const digests = new Set();
  for (const source of sources) digests.add(sha256(await readFile(source)));
  if (digests.size !== 1) throw new Error('source archives differ between targets');
  return { source: sources[0] };
}

async function buildMaterialsArchive(input, output, version) {
  const stage = await mkdtemp(join(os.tmpdir(), 'ossia-materials-'));
  try {
    for (const artifact of await artifactDirectories(input)) {
      const files = await filesUnder(artifact.dir);
      const release = files.find((file) => basename(file) === 'release.json');
      const manifest = files.find((file) => basename(file) === 'SOURCE-MANIFEST.json');
      const notices = files.find((file) => basename(file) === 'THIRD-PARTY-NOTICES.md');
      if (!release || !manifest || !notices) {
        throw new Error(`incomplete release materials for ${artifact.target}`);
      }
      const manifestRoot = join(stage, 'manifests', artifact.target);
      await mkdir(manifestRoot, { recursive: true });
      await copyFile(release, join(manifestRoot, 'release.json'));
      await copyFile(manifest, join(manifestRoot, 'SOURCE-MANIFEST.json'));
      await copyFile(notices, join(manifestRoot, 'THIRD-PARTY-NOTICES.md'));

      const licenses = files.find((file) => file.includes(`${join('materials', 'licenses')}`));
      if (!licenses) throw new Error(`missing license materials for ${artifact.target}`);
      const licenseRoot = join(stage, 'licenses', artifact.target);
      await mkdir(licenseRoot, { recursive: true });
      const materialsRoot = join(artifact.dir, 'materials', 'licenses');
      await cp(materialsRoot, licenseRoot, { recursive: true });
    }
    const archive = join(output, `ossia-${version}-materials.tar.gz`);
    execFileSync('tar', ['-czf', archive, '-C', stage, '.'], { stdio: 'inherit' });
    return archive;
  } finally {
    await rm(stage, { recursive: true, force: true });
  }
}

async function writeChecksums(names, output) {
  const lines = [];
  for (const name of [...names].sort((a, b) => a.localeCompare(b))) {
    const bytes = await readFile(join(output, name));
    lines.push(`${sha256(bytes)}  ${name}`);
  }
  await writeFile(join(output, 'SHA256SUMS'), `${lines.join('\n')}\n`);
}

/** Collect installers, one source archive and one combined materials archive. */
export async function collectRelease({ input, output, version }) {
  const normalized = parseTag(`v${version.replace(/^v/, '')}`);
  const collected = await collectInstallAssets(input, output);
  const sourceName = `ossia-${normalized}-source.tar.gz`;
  await copyFile(collected.source, join(output, sourceName));
  const materials = await buildMaterialsArchive(input, output, normalized);
  const names = [...INSTALL_ASSETS, sourceName, basename(materials)];
  await writeChecksums(names, output);
  return names;
}

async function main(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) values.set(argv[index], argv[index + 1]);
  const version = values.get('--version');
  const input = values.get('--input');
  const output = values.get('--output');
  if (!version || !input || !output) {
    throw new Error('usage: assets.mjs --version <tag> --input DIR --output DIR');
  }
  const names = await collectRelease({ input, output, version });
  console.log(`collected ${names.length} release assets and SHA256SUMS`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release assets: ${error.message}`);
    process.exitCode = 1;
  });
}

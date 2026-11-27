import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { parseTag } from './version.mjs';

/** Installer names uploaded for every release, before SHA256SUMS. */
export const INSTALL_ASSETS = [
  'ossia-darwin-arm64.dmg',
  'ossia-darwin-arm64.zip',
  'ossia-darwin-amd64.dmg',
  'ossia-darwin-amd64.zip',
  'ossia-windows-amd64.exe',
];

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

/** Normalize a Tauri output filename to the public platform/architecture contract. */
export function assetName(source) {
  const name = basename(source);
  if (INSTALL_ASSETS.includes(name)) return name;

  if (/_(x64|x86_64|amd64)(?:-apple-darwin)?\.dmg$/i.test(name)) {
    return 'ossia-darwin-amd64.dmg';
  }
  if (/_(x64|x86_64|amd64)(?:-apple-darwin)?\.app\.zip$/i.test(name)) {
    return 'ossia-darwin-amd64.zip';
  }
  if (name.endsWith('.dmg')) {
    if (/_(aarch64|arm64)(?:-apple-darwin)?\.dmg$/i.test(name)) {
      return 'ossia-darwin-arm64.dmg';
    }
  }
  if (name.endsWith('.app.zip')) {
    if (/_(aarch64|arm64)(?:-apple-darwin)?\.app\.zip$/i.test(name)) {
      return 'ossia-darwin-arm64.zip';
    }
  }
  if (name.endsWith('.exe')) {
    if (/_(x64|x86_64|amd64)(?:-pc-windows-msvc)?-setup\.exe$/i.test(name)) {
      return 'ossia-windows-amd64.exe';
    }
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

  for (const artifact of await artifactDirectories(input)) {
    for (const source of await filesUnder(artifact.dir)) {
      const name = basename(source);
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
}

async function writeChecksums(names, output) {
  const lines = [];
  for (const name of [...names].sort((a, b) => a.localeCompare(b))) {
    const bytes = await readFile(join(output, name));
    lines.push(`${sha256(bytes)}  ${name}`);
  }
  await writeFile(join(output, 'SHA256SUMS'), `${lines.join('\n')}\n`);
}

/** Collect installers and write their checksums. */
export async function collectRelease({ input, output, version }) {
  parseTag(`v${version.replace(/^v/, '')}`);
  await collectInstallAssets(input, output);
  const names = [...INSTALL_ASSETS];
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

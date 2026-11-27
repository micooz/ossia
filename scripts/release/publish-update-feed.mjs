import { execFileSync } from 'node:child_process';
import { readFile, stat } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { TARGETS } from './update-feed.mjs';
import { parseTag } from './version.mjs';

const UPLOADER = join(dirname(fileURLToPath(import.meta.url)), 'upload-update.mjs');
const DMGS = {
  'darwin-arm64': 'ossia-darwin-arm64.dmg',
  'darwin-amd64': 'ossia-darwin-amd64.dmg',
};

/** Publish the verified release installers to each update-feed target. */
export async function publishUpdateFeed({
  tag,
  assetDir,
  notesFile,
  stageDir,
  dryRun = false,
  upload = async (args) =>
    execFileSync(process.execPath, [UPLOADER, ...args], { stdio: 'inherit' }),
}) {
  parseTag(tag);
  if (!assetDir || !notesFile) throw new Error('asset directory and release notes are required');
  const directory = resolve(assetDir);
  const notes = resolve(notesFile);
  if (!(await readFile(notes, 'utf8')).trim()) throw new Error('release notes are empty');

  const plan = Object.entries(TARGETS).map(([target, asset]) => ({
    target,
    asset: join(directory, asset),
    extra: DMGS[target] ? join(directory, DMGS[target]) : null,
  }));
  for (const { asset, extra } of plan) {
    for (const file of [asset, ...(extra ? [extra] : [])]) {
      if (!(await stat(file)).isFile()) throw new Error(`release asset is not a file: ${file}`);
    }
  }

  for (const { target, asset, extra } of plan) {
    const args = ['--tag', tag, '--target', target, '--asset', asset, '--notes-file', notes];
    if (extra) args.push('--extra', extra);
    if (stageDir) args.push('--stage-dir', resolve(stageDir));
    if (dryRun) args.push('--dry-run');
    await upload(args);
  }
}

async function main(argv) {
  const args = new Map();
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === '--dry-run') args.set(key, true);
    else if (
      ['--tag', '--asset-dir', '--notes-file', '--stage-dir'].includes(key) &&
      argv[index + 1]
    ) {
      args.set(key, argv[++index]);
    } else throw new Error(`invalid or missing argument: ${key}`);
  }
  await publishUpdateFeed({
    tag: args.get('--tag'),
    assetDir: args.get('--asset-dir'),
    notesFile: args.get('--notes-file'),
    stageDir: args.get('--stage-dir'),
    dryRun: args.has('--dry-run'),
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`update feed: ${error.message}`);
    process.exitCode = 1;
  });
}

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { readFile } from 'node:fs/promises';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';

import { test } from 'vite-plus/test';

import { publishUpdateFeed } from '../../../scripts/release/publish-update-feed.mjs';

const installers = [
  'ossia-darwin-arm64.zip',
  'ossia-darwin-arm64.dmg',
  'ossia-darwin-amd64.zip',
  'ossia-darwin-amd64.dmg',
  'ossia-windows-amd64.exe',
];

async function fixture() {
  const root = await mkdtemp(path.join(os.tmpdir(), 'ossia-publish-feed-'));
  const assets = path.join(root, 'release');
  const notes = path.join(root, 'notes.md');
  await mkdir(assets);
  for (const file of installers) await writeFile(path.join(assets, file), file);
  await writeFile(path.join(assets, 'ossia-0.1.0-beta.20-source.tar.gz'), 'source');
  await writeFile(notes, 'Release notes');
  return { root, assets, notes };
}

test('publishes all updater targets and macOS DMGs without source archives', async () => {
  const f = await fixture();
  const calls = [];
  try {
    await publishUpdateFeed({
      tag: 'v0.1.0-beta.20',
      assetDir: f.assets,
      notesFile: f.notes,
      upload: async (args) => calls.push(args),
    });
    assert.deepEqual(
      calls.map((args) => args[args.indexOf('--target') + 1]),
      ['darwin-arm64', 'darwin-amd64', 'windows-amd64'],
    );
    assert.equal(calls.filter((args) => args.includes('--extra')).length, 2);
    for (const args of calls) {
      assert.ok(args.includes('v0.1.0-beta.20'));
      assert.ok(args.includes(f.notes));
      assert.ok(!args.some((arg) => arg.endsWith('.tar.gz')));
    }
  } finally {
    await rm(f.root, { recursive: true, force: true });
  }
});

test('validates all inputs before uploading anything', async () => {
  const f = await fixture();
  let calls = 0;
  try {
    await rm(path.join(f.assets, 'ossia-windows-amd64.exe'));
    await assert.rejects(
      publishUpdateFeed({
        tag: 'v0.1.0-beta.20',
        assetDir: f.assets,
        notesFile: f.notes,
        upload: async () => calls++,
      }),
      /ossia-windows-amd64.exe/,
    );
    assert.equal(calls, 0);
  } finally {
    await rm(f.root, { recursive: true, force: true });
  }
});

test('stops after an upload fails', async () => {
  const f = await fixture();
  let calls = 0;
  try {
    await assert.rejects(
      publishUpdateFeed({
        tag: 'v0.1.0-beta.20',
        assetDir: f.assets,
        notesFile: f.notes,
        upload: async () => {
          if (++calls === 2) throw new Error('upload failed');
        },
      }),
      /upload failed/,
    );
    assert.equal(calls, 2);
  } finally {
    await rm(f.root, { recursive: true, force: true });
  }
});

test('dry-run builds every feed using the real uploader without publishing', async () => {
  const f = await fixture();
  const requests = [];
  const server = http.createServer((request, response) => {
    const target = request.url.split('/')[1];
    requests.push(target);
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ schemaVersion: 1, target, releases: [] }));
  });
  try {
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const stage = path.join(f.root, 'stage');
    await promisify(execFile)(
      process.execPath,
      [
        'scripts/release/publish-update-feed.mjs',
        '--tag',
        'v0.1.0-beta.20',
        '--asset-dir',
        f.assets,
        '--notes-file',
        f.notes,
        '--stage-dir',
        stage,
        '--dry-run',
      ],
      {
        cwd: process.cwd(),
        env: {
          ...process.env,
          OSSIA_RELEASES_URL: `http://127.0.0.1:${server.address().port}`,
          OSSIA_RELEASES_PREFIX: '',
          OSSIA_RELEASES_SECRET_ID: '',
          OSSIA_RELEASES_SECRET_KEY: '',
          OSSIA_RELEASES_SECURITY_TOKEN: '',
        },
      },
    );
    assert.equal(requests.length, 3);
    for (const target of requests) {
      const index = JSON.parse(await readFile(path.join(stage, target, 'index.json'), 'utf8'));
      assert.equal(index.releases[0].version, '0.1.0-beta.20');
      assert.equal(index.releases[0].body, 'Release notes');
      assert.match(index.releases[0].assets[0].digest, /^sha256:[a-f0-9]{64}$/);
    }
  } finally {
    await new Promise((resolve) => server.close(resolve));
    await rm(f.root, { recursive: true, force: true });
  }
});

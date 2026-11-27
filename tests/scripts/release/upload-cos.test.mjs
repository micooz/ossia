import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { test } from 'vite-plus/test';

import { cosClient, fileDigest, objectKey, putObject } from '../../../scripts/release/cos.mjs';
import { readOriginIndex } from '../../../scripts/release/upload-update.mjs';

test('COS release requests use the global acceleration endpoint', () => {
  const client = cosClient({ secretId: 'test', secretKey: 'test' });
  const url = client.getObjectUrl({
    Bucket: 'example-1250000000',
    Region: 'ap-shanghai',
    Key: 'index.json',
    Sign: false,
  });
  assert.equal(new URL(url).hostname, 'example-1250000000.cos.accelerate.myqcloud.com');
});

test('reads the authoritative manifest from object storage', async () => {
  const index = { schemaVersion: 1, target: 'darwin-arm64', releases: [] };
  const args = {
    bucket: 'bucket',
    region: 'region',
    key: 'target/index.json',
    target: 'darwin-arm64',
  };
  const calls = [];
  const client = {
    getObject: async (params) => {
      calls.push(params);
      return { Body: Buffer.from(JSON.stringify(index)) };
    },
  };
  assert.deepEqual(await readOriginIndex({ ...args, client }), index);
  assert.equal(calls[0].Key, args.key);
  assert.deepEqual(
    await readOriginIndex({
      ...args,
      client: {
        getObject: async () => {
          throw Object.assign(new Error('not found'), { statusCode: 404, code: 'NoSuchKey' });
        },
      },
    }),
    index,
  );
  await assert.rejects(
    readOriginIndex({
      ...args,
      client: {
        getObject: async () => {
          throw new Error('access denied');
        },
      },
    }),
    /access denied/,
  );
  await assert.rejects(
    readOriginIndex({
      ...args,
      client: {
        getObject: async () => {
          throw Object.assign(new Error('bucket not found'), {
            statusCode: 404,
            code: 'NoSuchBucket',
          });
        },
      },
    }),
    /bucket not found/,
  );
});

test('object keys normalize prefixes and preserve path segments', () => {
  assert.equal(
    objectKey('', 'darwin-arm64', '0.1.0-beta.5', 'asset.zip'),
    'darwin-arm64/0.1.0-beta.5/asset.zip',
  );
  assert.equal(
    objectKey('/releases/', 'darwin-arm64', 'index.json'),
    'releases/darwin-arm64/index.json',
  );
});

test('file digests are calculated without loading the asset into memory', async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-cos-'));
  try {
    const file = path.join(directory, 'asset.bin');
    fs.writeFileSync(file, 'ossia');
    assert.deepEqual(await fileDigest(file), {
      sha256: '4732beaba992b1682046f422f532bf4d792ee6fc296300e8fb962d174e2d6886',
      size: 5,
    });
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('COS uploads use uploadFile for files and putObject for manifests', async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-cos-upload-'));
  const calls = [];
  const client = {
    uploadFile: async (params) => calls.push({ kind: 'file', params }),
    putObject: async (params) => calls.push({ kind: 'object', params }),
  };
  try {
    const file = path.join(directory, 'asset.zip');
    fs.writeFileSync(file, 'zip');
    const common = {
      client,
      bucket: 'ossia-releases-1250000000',
      region: 'ap-guangzhou',
      contentType: 'application/zip',
      cacheControl: 'public, max-age=60',
      acl: 'public-read',
    };
    await putObject({ ...common, key: 'target/asset.zip', file });
    await putObject({ ...common, key: 'target/index.json', body: Buffer.from('{}') });
    assert.equal(calls[0].kind, 'file');
    assert.equal(calls[0].params.FilePath, file);
    assert.equal(calls[0].params.ACL, 'public-read');
    assert.equal(calls[1].kind, 'object');
    assert.deepEqual(calls[1].params.Body, Buffer.from('{}'));
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('upload CLI stages a merged manifest without credentials in dry-run mode', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-upload-'));
  try {
    const asset = path.join(directory, 'asset.zip');
    const notes = path.join(directory, 'release-notes.md');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    const stage = path.join(directory, 'stage');
    fs.writeFileSync(asset, 'zip');
    fs.writeFileSync(notes, '## Fixes\n- Updated release notes.\n');
    fs.writeFileSync(
      current,
      JSON.stringify({ schemaVersion: 1, target: 'darwin-arm64', releases: [] }),
    );
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://ossia-releases.maisow.com\n');
    const output = execFileSync(
      process.execPath,
      [
        'scripts/release/upload-update.mjs',
        '--env-file',
        env,
        '--tag',
        'v0.1.0-beta.6',
        '--target',
        'darwin-arm64',
        '--asset',
        asset,
        '--notes-file',
        notes,
        '--current-index',
        current,
        '--stage-dir',
        stage,
        '--dry-run',
      ],
      { cwd: process.cwd(), encoding: 'utf8' },
    );
    assert.match(output, /darwin-arm64\/index\.json/);
    const index = JSON.parse(
      fs.readFileSync(path.join(stage, 'darwin-arm64', 'index.json'), 'utf8'),
    );
    assert.equal(index.releases[0].version, '0.1.0-beta.6');
    assert.equal(index.releases[0].body, '## Fixes\n- Updated release notes.');
    assert.equal(index.releases[0].assets[0].digest.startsWith('sha256:'), true);
    assert.equal(index.releases[0].assets[0].url, '0.1.0-beta.6/ossia-darwin-arm64.zip');
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('upload CLI refuses a release without notes', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-upload-no-notes-'));
  try {
    const asset = path.join(directory, 'asset.zip');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    fs.writeFileSync(asset, 'zip');
    fs.writeFileSync(
      current,
      JSON.stringify({ schemaVersion: 1, target: 'darwin-arm64', releases: [] }),
    );
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://ossia-releases.maisow.com\n');
    assert.throws(
      () =>
        execFileSync(
          process.execPath,
          [
            'scripts/release/upload-update.mjs',
            '--env-file',
            env,
            '--tag',
            'v0.1.0-beta.6',
            '--target',
            'darwin-arm64',
            '--asset',
            asset,
            '--current-index',
            current,
            '--dry-run',
          ],
          { cwd: process.cwd(), encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
        ),
      /--notes-file is required/,
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('upload CLI refuses to replace an existing version with different bytes', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-upload-conflict-'));
  try {
    const asset = path.join(directory, 'asset.zip');
    const notes = path.join(directory, 'notes.md');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    fs.writeFileSync(asset, 'new bytes');
    fs.writeFileSync(notes, 'Notes');
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://example.com\n');
    fs.writeFileSync(
      current,
      JSON.stringify({
        schemaVersion: 1,
        target: 'darwin-arm64',
        releases: [
          {
            version: '0.1.0-beta.20',
            prerelease: true,
            body: 'Notes',
            assets: [
              {
                name: 'ossia-darwin-arm64.zip',
                size: 1,
                digest: `sha256:${'a'.repeat(64)}`,
                url: '0.1.0-beta.20/ossia-darwin-arm64.zip',
              },
            ],
          },
        ],
      }),
    );
    assert.throws(
      () =>
        execFileSync(
          process.execPath,
          [
            'scripts/release/upload-update.mjs',
            '--env-file',
            env,
            '--tag',
            'v0.1.0-beta.20',
            '--target',
            'darwin-arm64',
            '--asset',
            asset,
            '--notes-file',
            notes,
            '--current-index',
            current,
            '--stage-dir',
            path.join(directory, 'stage'),
            '--dry-run',
          ],
          { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
        ),
      /already exists with different asset/,
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('upload CLI can retry an existing version with identical bytes', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-upload-retry-'));
  try {
    const asset = path.join(directory, 'asset.zip');
    const notes = path.join(directory, 'notes.md');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    const bytes = Buffer.from('same bytes');
    fs.writeFileSync(asset, bytes);
    fs.writeFileSync(notes, 'Notes');
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://example.com\n');
    fs.writeFileSync(
      current,
      JSON.stringify({
        schemaVersion: 1,
        target: 'darwin-arm64',
        releases: [
          {
            version: '0.1.0-beta.20',
            prerelease: true,
            body: 'old',
            assets: [
              {
                name: 'ossia-darwin-arm64.zip',
                size: bytes.length,
                digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`,
                url: '0.1.0-beta.20/ossia-darwin-arm64.zip',
              },
            ],
          },
        ],
      }),
    );
    assert.doesNotThrow(() =>
      execFileSync(
        process.execPath,
        [
          'scripts/release/upload-update.mjs',
          '--env-file',
          env,
          '--tag',
          'v0.1.0-beta.20',
          '--target',
          'darwin-arm64',
          '--asset',
          asset,
          '--notes-file',
          notes,
          '--current-index',
          current,
          '--stage-dir',
          path.join(directory, 'stage'),
          '--dry-run',
        ],
        { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
      ),
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('Windows releases upload versioned installers', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-latest-'));
  try {
    const asset = path.join(directory, 'installer.exe');
    const notes = path.join(directory, 'notes.md');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    fs.writeFileSync(asset, 'installer');
    fs.writeFileSync(notes, 'Notes');
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://example.com\n');
    fs.writeFileSync(
      current,
      JSON.stringify({
        schemaVersion: 1,
        target: 'windows-amd64',
        releases: [{ version: '1.2.0', prerelease: false, assets: [] }],
      }),
    );
    function uploads(tag) {
      return JSON.parse(
        execFileSync(
          process.execPath,
          [
            'scripts/release/upload-update.mjs',
            '--env-file',
            env,
            '--tag',
            tag,
            '--target',
            'windows-amd64',
            '--asset',
            asset,
            '--notes-file',
            notes,
            '--current-index',
            current,
            '--stage-dir',
            path.join(directory, 'stage'),
            '--dry-run',
          ],
          { encoding: 'utf8' },
        ),
      ).uploads;
    }
    assert.deepEqual(uploads('v1.3.0'), ['windows-amd64/1.3.0/ossia-windows-amd64.exe']);
    assert.deepEqual(uploads('v1.3.1-beta.1'), [
      'windows-amd64/1.3.1-beta.1/ossia-windows-amd64.exe',
    ]);
    assert.deepEqual(uploads('v1.1.0'), ['windows-amd64/1.1.0/ossia-windows-amd64.exe']);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('macOS publication uploads versioned DMG and ZIP assets', () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-latest-mac-'));
  try {
    const asset = path.join(directory, 'asset.zip');
    const dmg = path.join(directory, 'ossia-darwin-arm64.dmg');
    const notes = path.join(directory, 'notes.md');
    const current = path.join(directory, 'index.json');
    const env = path.join(directory, '.env');
    fs.writeFileSync(asset, 'zip');
    fs.writeFileSync(dmg, 'dmg');
    fs.writeFileSync(notes, 'Notes');
    fs.writeFileSync(env, 'OSSIA_RELEASES_URL=https://example.com\n');
    fs.writeFileSync(
      current,
      JSON.stringify({ schemaVersion: 1, target: 'darwin-arm64', releases: [] }),
    );
    const uploads = JSON.parse(
      execFileSync(
        process.execPath,
        [
          'scripts/release/upload-update.mjs',
          '--env-file',
          env,
          '--tag',
          'v1.0.0',
          '--target',
          'darwin-arm64',
          '--asset',
          asset,
          '--extra',
          dmg,
          '--notes-file',
          notes,
          '--current-index',
          current,
          '--stage-dir',
          path.join(directory, 'stage'),
          '--dry-run',
        ],
        { encoding: 'utf8' },
      ),
    ).uploads;
    assert.deepEqual(uploads, [
      'darwin-arm64/1.0.0/ossia-darwin-arm64.dmg',
      'darwin-arm64/1.0.0/ossia-darwin-arm64.zip',
    ]);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

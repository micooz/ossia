import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import {
  candidateRoots,
  hostTarget,
  install,
  parseLock,
  readLock,
  verifyTree,
  writeInventory,
} from '../../scripts/engine.mjs';

await test('five-target lock declares actual launchers and refuses unbuilt ARM64 packages', async () => {
  const lock = parseLock(
    fs.readFileSync(new URL('../../vendor/ENGINE.lock', import.meta.url), 'utf8'),
  );
  assert.equal(Object.keys(lock.targets).length, 5);
  assert.equal(lock.targets['mingw-x86_64'].tools.musicxml2ly.program, 'bin/python.exe');
  assert.deepEqual(lock.targets['mingw-x86_64'].tools.musicxml2ly.args, ['bin/musicxml2ly.py']);
  assert.equal(lock.targets['linux-arm64'].archive, null);
  assert.throws(() => parseLock('{"schemaVersion":2}'));
  assert.throws(() => hostTarget('linux', 's390x'));
  const { roots } = candidateRoots(undefined, { OSSIA_LILYPOND_ROOT: '/explicit/developer/root' });
  assert.deepEqual(roots, [{ src: 'env', dir: '/explicit/developer/root' }]);
  await assert.rejects(install(undefined, { target: 'linux-arm64' }), /no locked binary archive/);
});

await test('resource inventory detects corruption, omitted files, and traversal', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-engine-inventory-'));
  try {
    const lock = {
      target: hostTarget(),
      engineBuildId: 'fixture',
      archive: { sha256: 'a'.repeat(64) },
      requiredDirectories: ['data'],
      tools: {},
    };
    fs.mkdirSync(path.join(root, 'data'));
    fs.writeFileSync(path.join(root, 'data/input'), 'original');
    writeInventory(root, lock);
    verifyTree(root, lock, false);
    fs.writeFileSync(path.join(root, 'data/input'), 'modified');
    assert.throws(() => verifyTree(root, lock, false), /damaged/);
    writeInventory(root, lock);
    fs.writeFileSync(path.join(root, 'additional'), 'unexpected');
    assert.throws(() => verifyTree(root, lock, false), /incomplete/);
    fs.rmSync(path.join(root, 'additional'));
    const inventory = JSON.parse(fs.readFileSync(path.join(root, 'resources.json'), 'utf8'));
    inventory.files[0].path = '../outside';
    fs.writeFileSync(path.join(root, 'resources.json'), JSON.stringify(inventory));
    assert.throws(() => verifyTree(root, lock, false));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

await test('missing or corrupted archive never destroys an existing engine', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-engine-prepare-'));
  try {
    fs.mkdirSync(path.join(root, 'vendor'));
    fs.copyFileSync(
      new URL('../../vendor/ENGINE.lock', import.meta.url),
      path.join(root, 'vendor/ENGINE.lock'),
    );
    const lock = readLock(root);
    const dir = path.join(root, 'vendor/lilypond', lock.tag);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, 'keep'), 'keep');
    await assert.rejects(install(root), /archive missing/);
    fs.mkdirSync(path.join(root, 'vendor/dist'), { recursive: true });
    fs.writeFileSync(path.join(root, 'vendor/dist', lock.archive.file), 'corrupted archive');
    await assert.rejects(install(root), /archive missing/);
    assert.equal(fs.readFileSync(path.join(dir, 'keep'), 'utf8'), 'keep');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

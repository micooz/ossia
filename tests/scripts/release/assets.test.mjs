import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { test } from 'vite-plus/test';

import { assetName, collectRelease, INSTALL_ASSETS } from '../../../scripts/release/assets.mjs';

test('Tauri output names map to the public asset contract', () => {
  assert.equal(assetName('Ossia_0.1.0_aarch64-apple-darwin.dmg'), 'ossia-darwin-arm64.dmg');
  assert.equal(assetName('ossia_0.1.0_aarch64-apple-darwin.dmg'), 'ossia-darwin-arm64.dmg');
  assert.equal(assetName('Ossia_0.1.0_aarch64-apple-darwin.app.zip'), 'ossia-darwin-arm64.zip');
  assert.equal(assetName('Ossia_0.1.0_x64-setup.exe'), 'ossia-windows-amd64.exe');
  assert.equal(assetName('Ossia_0.1.0_x86_64-apple-darwin.dmg'), 'ossia-darwin-amd64.dmg');
  assert.equal(assetName('Ossia_0.1.0_x86_64-apple-darwin.app.zip'), 'ossia-darwin-amd64.zip');
  assert.throws(() => assetName('Ossia_0.1.0_amd64.deb'), /unsupported/);
  assert.throws(() => assetName('Ossia_0.1.0_aarch64.AppImage'), /unsupported/);
});

function writeReleaseFixture(root) {
  const targets = ['macos-arm64', 'macos-x86_64', 'windows-x86_64'];
  const files = {
    'macos-arm64': [
      'Ossia_0.1.0_aarch64-apple-darwin.dmg',
      'Ossia_0.1.0_aarch64-apple-darwin.app.zip',
    ],
    'macos-x86_64': [
      'Ossia_0.1.0_x86_64-apple-darwin.dmg',
      'Ossia_0.1.0_x86_64-apple-darwin.app.zip',
    ],
    'windows-x86_64': ['Ossia_0.1.0_x64-setup.exe'],
  };
  for (const target of targets) {
    const artifact = path.join(root, `build-${target}`);
    const materials = path.join(artifact, 'materials');
    fs.mkdirSync(materials, { recursive: true });
    for (const name of files[target]) fs.writeFileSync(path.join(artifact, name), name);
    fs.writeFileSync(path.join(materials, 'SOURCE-MANIFEST.json'), '{}');
    fs.writeFileSync(path.join(materials, 'THIRD-PARTY-NOTICES.md'), 'notices');
    fs.mkdirSync(path.join(materials, 'licenses'), { recursive: true });
    fs.writeFileSync(path.join(materials, 'licenses', 'COPYING'), 'license');
  }
}

test('release collection requires every installer and emits checksums', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-assets-'));
  try {
    const input = path.join(root, 'downloaded');
    const output = path.join(root, 'release');
    fs.mkdirSync(input);
    writeReleaseFixture(input);
    const names = await collectRelease({ input, output, version: 'v0.1.0-beta.1' });
    assert.equal(names.length, INSTALL_ASSETS.length);
    assert.equal(
      fs.readdirSync(output).sort().join('\n'),
      [...INSTALL_ASSETS, 'SHA256SUMS'].sort().join('\n'),
    );
    const sums = fs.readFileSync(path.join(output, 'SHA256SUMS'), 'utf8').trim().split('\n');
    assert.equal(sums.length, names.length);
    for (const name of names) {
      const digest = createHash('sha256')
        .update(fs.readFileSync(path.join(output, name)))
        .digest('hex');
      assert.ok(sums.includes(`${digest}  ${name}`));
    }
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('missing and duplicate installers fail before publication', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-assets-error-'));
  try {
    const input = path.join(root, 'downloaded');
    fs.mkdirSync(input);
    writeReleaseFixture(input);
    fs.renameSync(
      path.join(input, 'build-windows-x86_64', 'Ossia_0.1.0_x64-setup.exe'),
      path.join(root, 'removed.exe'),
    );
    await assert.rejects(
      collectRelease({ input, output: path.join(root, 'release'), version: 'v0.1.0-beta.1' }),
      /missing release asset ossia-windows-amd64.exe/,
    );

    const duplicateRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-assets-duplicate-'));
    try {
      writeReleaseFixture(duplicateRoot);
      fs.writeFileSync(
        path.join(duplicateRoot, 'build-macos-arm64', 'ossia-darwin-arm64.dmg'),
        'duplicate',
      );
      await assert.rejects(
        collectRelease({
          input: duplicateRoot,
          output: path.join(root, 'duplicate-output'),
          version: 'v0.1.0-beta.1',
        }),
        /two source files map to ossia-darwin-arm64.dmg/,
      );
    } finally {
      fs.rmSync(duplicateRoot, { recursive: true, force: true });
    }
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

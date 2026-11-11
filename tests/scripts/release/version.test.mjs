import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import {
  checkVersion,
  isNewerTag,
  isPrereleaseTag,
  parseTag,
  rpmBundleConfig,
  setVersion,
} from '../../../scripts/release/version.mjs';

void test('release tags parse stable and numbered beta versions', () => {
  assert.equal(parseTag('v0.1.0'), '0.1.0');
  assert.equal(parseTag('v1.2.3-beta.4'), '1.2.3-beta.4');
  assert.equal(isPrereleaseTag('v1.2.3-beta.4'), true);
  assert.equal(isPrereleaseTag('v1.2.3'), false);
  assert.throws(() => parseTag('v1.2'), /release tag must be/);
  assert.throws(() => parseTag('v1.2.3-rc.1'), /release tag must be/);
});

void test('release versions compare by SemVer precedence', () => {
  assert.equal(isNewerTag('v0.1.1', 'v0.1.0'), true);
  assert.equal(isNewerTag('v0.1.0', 'v0.1.0-beta.2'), true);
  assert.equal(isNewerTag('v0.1.0-beta.3', 'v0.1.0-beta.2'), true);
  assert.equal(isNewerTag('v0.1.0-beta.2', 'v0.1.0'), false);
});

void test('beta RPM versions use a numeric release', () => {
  assert.deepEqual(rpmBundleConfig('v0.1.0-beta.2'), {
    version: '0.1.0',
    bundle: { linux: { rpm: { release: '0.beta.2.1' } } },
  });
  assert.deepEqual(rpmBundleConfig('v0.1.0'), {
    version: '0.1.0',
    bundle: { linux: { rpm: { release: '1' } } },
  });
});

void test('version injection updates every manifest and can be checked', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ossia-version-'));
  try {
    fs.mkdirSync(path.join(root, 'packages/desktop/src-tauri'), { recursive: true });
    fs.writeFileSync(
      path.join(root, 'Cargo.toml'),
      '[workspace.package]\nversion = "0.1.0"\nedition = "2021"\n',
    );
    fs.writeFileSync(path.join(root, 'package.json'), '{"version":"0.1.0"}\n');
    fs.writeFileSync(path.join(root, 'packages/desktop/package.json'), '{"version":"0.1.0"}\n');
    fs.writeFileSync(
      path.join(root, 'packages/desktop/src-tauri/tauri.conf.json'),
      '{\n  "productName": "ossia",\n  "version": "0.1.0",\n  "bundle": {\n    "targets": ["app"]\n  }\n}\n',
    );

    await setVersion(root, 'v0.1.0-beta.1', { refreshLock: false });
    assert.equal(await checkVersion(root, 'v0.1.0-beta.1'), '0.1.0-beta.1');
    assert.match(
      fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8'),
      /version = "0\.1\.0-beta\.1"/,
    );
    assert.equal(
      fs.readFileSync(path.join(root, 'packages/desktop/src-tauri/tauri.conf.json'), 'utf8'),
      '{\n  "productName": "ossia",\n  "version": "0.1.0-beta.1",\n  "bundle": {\n    "targets": ["app"]\n  }\n}\n',
    );
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

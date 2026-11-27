import assert from 'node:assert/strict';

import { test } from 'vite-plus/test';

import { detectInstallerTarget } from '../../website/assets/site-platform.js';

test('detects only explicitly reported supported 64-bit architectures', async () => {
  const device = (platform, architecture, bitness) => ({
    platform,
    getHighEntropyValues: async () => ({ architecture, bitness }),
  });
  assert.equal(await detectInstallerTarget(device('macOS', 'arm', '64')), 'darwin-arm64');
  assert.equal(await detectInstallerTarget(device('macOS', 'x86', '64')), 'darwin-amd64');
  assert.equal(await detectInstallerTarget(device('Windows', 'x86', '64')), 'windows-amd64');
  assert.equal(await detectInstallerTarget(device('Windows', 'arm', '64')), null);
  assert.equal(await detectInstallerTarget(device('Linux', 'x86', '64')), null);
  assert.equal(await detectInstallerTarget(device('macOS', 'arm', '')), null);
  assert.equal(await detectInstallerTarget(device('macOS', 'x86', '32')), null);
  assert.equal(await detectInstallerTarget(undefined), null);
  assert.equal(
    await detectInstallerTarget({
      platform: 'macOS',
      getHighEntropyValues: async () => {
        throw new Error('denied');
      },
    }),
    null,
  );
});

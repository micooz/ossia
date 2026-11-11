import assert from 'node:assert/strict';
import test from 'node:test';

import { assertBuildable, specFor } from '../../scripts/package.mjs';

void test('targets map to Rust triples and platform bundles', () => {
  assert.deepEqual(specFor('darwin-arm64'), {
    triple: 'aarch64-apple-darwin',
    bundles: ['app'],
    platform: 'macos',
    artifact: 'macos-arm64',
  });
  assert.equal(specFor('mingw-x86_64').triple, 'x86_64-pc-windows-msvc');
  assert.equal(specFor('linux-arm64').triple, 'aarch64-unknown-linux-gnu');
  assert.deepEqual(specFor('linux-x86_64').bundles, ['deb', 'rpm']);
});

void test('unbuilt linux arm64 is rejected instead of producing a fake artifact', () => {
  const lock = {
    targets: {
      'linux-arm64': { archive: null },
      'darwin-arm64': { archive: { sha256: 'a'.repeat(64) } },
    },
  };
  assert.throws(() => assertBuildable('linux-arm64', lock), /no built engine archive/);
  assert.equal(assertBuildable('darwin-arm64', lock).archive.sha256.length, 64);
});

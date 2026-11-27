import assert from 'node:assert/strict';

import { test } from 'vite-plus/test';

import { TARGETS, mergeRelease } from '../../../scripts/release/update-feed.mjs';

test('mirror targets map to the updater asset names', () => {
  assert.equal(TARGETS['darwin-arm64'], 'ossia-darwin-arm64.zip');
  assert.equal(TARGETS['darwin-amd64'], 'ossia-darwin-amd64.zip');
  assert.equal(TARGETS['windows-amd64'], 'ossia-windows-amd64.exe');
  assert.deepEqual(Object.keys(TARGETS), ['darwin-arm64', 'darwin-amd64', 'windows-amd64']);
});

test('mirror index keeps all versions and replaces the same version', () => {
  const index = {
    schemaVersion: 1,
    target: 'darwin-arm64',
    releases: [
      { version: '0.3.0', prerelease: false, body: 'new', assets: [] },
      { version: '0.1.0', prerelease: false, body: 'old', assets: [] },
    ],
  };
  const merged = mergeRelease(index, {
    version: '0.2.0',
    prerelease: false,
    body: 'middle',
    assets: [],
  });
  assert.deepEqual(
    merged.releases.map((release) => release.version),
    ['0.3.0', '0.2.0', '0.1.0'],
  );

  const replaced = mergeRelease(merged, {
    version: '0.2.0',
    prerelease: true,
    body: 'replaced',
    assets: [],
  });
  assert.equal(replaced.releases.find((release) => release.version === '0.2.0').body, 'replaced');
  assert.equal(replaced.releases.length, 3);
});

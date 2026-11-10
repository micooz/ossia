import assert from 'node:assert/strict';
import { test } from 'node:test';

import { highestTag, nextPatchTag } from './tags.mjs';

void test('the highest release tag uses the same ordering as version checks', () => {
  assert.equal(highestTag(['v0.1.0', 'v0.2.0-beta.1', 'v0.1.9']), 'v0.2.0-beta.1');
  assert.equal(highestTag(['not-a-release']), null);
});

void test('automatic tags always allocate the next stable patch, even after a beta', () => {
  assert.equal(nextPatchTag(['v0.1.0-beta.1']), 'v0.1.1');
  assert.equal(nextPatchTag(['v0.1.0', 'v0.4.2']), 'v0.4.3');
});

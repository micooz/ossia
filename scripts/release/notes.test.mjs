import assert from 'node:assert/strict';
import { test } from 'node:test';

import { CHINESE_MARKER, splitBilingualNotes } from './notes.mjs';

void test('bilingual DeepSeek notes require matching category order', () => {
  const notes = `## Features
- Add a workflow

## Bug Fixes
- Fix a crash

${CHINESE_MARKER}

## 新功能
- 新增工作流

## 错误修复
- 修复崩溃`;
  assert.deepEqual(splitBilingualNotes(notes), [
    '## Features\n- Add a workflow\n\n## Bug Fixes\n- Fix a crash',
    '## 新功能\n- 新增工作流\n\n## 错误修复\n- 修复崩溃',
  ]);
  assert.throws(
    () => splitBilingualNotes(`## Features\n- x\n\n${CHINESE_MARKER}\n\n## 改进\n- y`),
    /categories must correspond/,
  );
});

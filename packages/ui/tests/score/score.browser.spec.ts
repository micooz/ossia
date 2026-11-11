import { expect, test } from '@playwright/test';

import { hash, newer, secondId, scoreDocument, mockScoreRuntime } from './score-runtime.ts';

test('live language events update an open score and browser color scheme is independent', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, { languageEvent: 'en-US' });
  await page.goto('/score.html#ticket=language-view');
  await expect(page.getByText('Local service connected')).toBeVisible();
  await expect(page.getByLabel('Current score')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Workspace score' })).toBeVisible();
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test('ticket is single-use, refresh and independent views preserve selection', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto('/score.html#ticket=view-one');
  await expect(page.getByText('本地服务已连接')).toBeVisible();
  expect(page.url()).not.toContain('ticket=');
  expect(h.exchanges()).toBe(1);
  await page.getByText('使用键盘选择小节').click();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 1 小节');
  const other = await context.newPage();
  await other.goto('/score.html#ticket=view-two');
  await other.getByLabel('当前乐谱').selectOption(secondId);
  await expect(other.locator('.document-status h2')).toHaveText('另一份乐谱');
  await expect(page.locator('.document-status h2')).toHaveText('合成钢琴练习');
  await page.reload();
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 1 小节');
  expect(h.exchanges()).toBe(2);
  await page.getByRole('button', { name: '清空选区' }).click();
  await expect(page.locator('.selection')).toContainText('尚未选择小节');
});
test('SVG transforms, staff boxes, pages, zoom and keyboard work at narrow and wide widths', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto('/score.html#ticket=geometry');
  await expect(page.locator('.score-image')).toBeVisible();
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    await page.getByLabel('谱面缩放').selectOption('150');
    const box = await page.locator('.overlay').boundingBox();
    expect(box).not.toBeNull();
    await page
      .locator('.overlay')
      .click({ position: { x: box!.width * 0.4, y: (box!.height * 55) / 140 } });
    await expect(page.locator('.selection li')).toHaveText('谱表 2 · 第 1 小节');
    const rect = page.locator('.selection-box');
    await expect(rect).toHaveAttribute('y', '50');
    await expect(rect).toHaveAttribute('height', '10');
    expect(h.points.at(-1)!.y).toBeCloseTo(55, 0);
    await page.getByRole('button', { name: '清空选区' }).click();
  }
  await page.getByLabel('谱面页码').selectOption('2');
  await page.getByText('使用键盘选择小节').click();
  const button = page.getByRole('button', { name: '谱表 1 · 第 2 小节', exact: true });
  await button.focus();
  await page.keyboard.press('Space');
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 2 小节');
  await page.keyboard.press('Enter');
  await expect(page.locator('.selection li')).toHaveCount(0);
});
test('failed compilation retains old pages, clears selection, reports positions and busy conflict', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto('/score.html#ticket=failure');
  await page.getByText('使用键盘选择小节').click();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  h.change();
  await expect(page.getByText('上次成功结果：', { exact: false })).toBeVisible();
  await expect(page.locator('.selection li')).toHaveCount(0);
  await expect(page.locator('.score-image')).toBeVisible();
  await expect(page.locator('.diagnostics')).toContainText('第 4 行，第 2 列');
  await expect(
    page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }),
  ).toBeDisabled();
  await page.getByLabel('谱面页码').selectOption('2');
  await expect(page.locator('.score-image')).toHaveAttribute('alt', /第 2 页/);
  h.conflict();
  await page.getByRole('button', { name: '丢弃工作区修改' }).click();
  await page.getByRole('button', { name: '确认丢弃' }).click();
  await expect(page.locator('.message.error')).toContainText('另一任务正在编辑');
  await expect(page.locator('.document-status')).toContainText('工作区有未提交修改');
});
test('stale selection is reported; reconnect preserves state and a restarted runtime needs a new link', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto('/score.html#ticket=reconnect');
  await page.getByText('使用键盘选择小节').click();
  h.stale();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('source changed');
  h.disconnect(true);
  await expect(page.getByText('连接中断，正在重连…')).toBeVisible();
  h.disconnect(false);
  await expect(page.getByText('本地服务已连接')).toBeVisible();
  h.expire();
  await expect(page.getByRole('heading', { name: '视图已失效' })).toBeVisible();
});

test('discard confirmation keeps the original hash when source changes while the dialog is open', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  h.states[0]!.dirty = true;
  await page.goto('/score.html#ticket=confirm-hash');
  await page.getByRole('button', { name: '丢弃工作区修改' }).click();
  h.change();
  await expect(page.locator('.document-status')).toContainText(newer.slice(0, 12));
  await page.getByRole('button', { name: '确认丢弃' }).click();
  await expect.poll(() => h.mutations.length).toBe(1);
  expect(h.mutations[0]!.expectedSourceHash).toBe(hash);
});

test('import presents precise losses, carries confirmation and never bypasses refusal', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  const confirmation = 'c'.repeat(64);
  let confirmed = false;
  let refused = false;
  await page.route('**/api/v1/documents', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postData() ?? '';
    confirmed = body.includes(confirmation);
    const report = {
      status: refused ? 'refused' : confirmed ? 'clean' : 'needs-confirmation',
      confirmation,
      inputHash: hash,
      engineBuildId: 'synthetic',
      source: null,
      sourceHash: null,
      confirmed,
      measures: 2,
      voices: 1,
      noteEvents: 2,
      losses: ['合成损失：歌词未保留'],
      blocking: refused ? ['合成阻断：Negative skip'] : [],
      diagnostics: [],
    };
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        jobId: 'import-job',
        documentId: null,
        kind: 'import-request',
        state: 'done',
        phase: 'finished',
        sourceHash: null,
        error: null,
        result: {
          report,
          document: !refused && confirmed ? scoreDocument() : null,
          render: null,
          job: null,
        },
      }),
    });
  });
  await page.goto('/score.html#ticket=import');
  expect(await page.evaluate(() => '__TAURI_INTERNALS__' in window)).toBe(false);
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'synthetic.musicxml',
    mimeType: 'text/xml',
    buffer: Buffer.from('<score-partwise/>'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.getByText('合成损失：歌词未保留')).toBeVisible();
  expect(confirmed).toBe(false);
  await page.getByRole('button', { name: '知悉损失，确认导入' }).click();
  await expect.poll(() => confirmed).toBe(true);
  await expect(page.locator('.import-report')).toHaveCount(0);
  refused = true;
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'refused.musicxml',
    mimeType: 'text/xml',
    buffer: Buffer.from('<score-partwise/>'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.getByText('合成阻断：Negative skip')).toBeVisible();
  await expect(page.getByRole('button', { name: '知悉损失，确认导入' })).toHaveCount(0);
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'unsupported.mxl',
    mimeType: 'application/zip',
    buffer: Buffer.from('compressed'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.locator('.import')).toContainText('暂不支持 .mxl');
});

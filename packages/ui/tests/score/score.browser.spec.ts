import { expect, test } from '@playwright/test';

import { firstId, mockScoreRuntime, olderVersionId } from './score-runtime.ts';

test('score page focuses on one document, all pages and its title', async ({ context, page }) => {
  await mockScoreRuntime(context, { languageEvent: 'en-US' });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.getByText('Connected')).toBeVisible();
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  await expect(page).toHaveTitle('合成钢琴练习');
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(page.locator('.topbar-right .score-actions')).toBeVisible();
  await expect(
    page.locator('.topbar-right').getByRole('button', { name: 'Version', exact: true }),
  ).toHaveText('Version');
  await expect(
    page.locator('.topbar-right').getByRole('button', { name: 'Export', exact: true }),
  ).toHaveText('Export');
  await expect(
    page.locator('.topbar-right').getByRole('button', { name: 'Compare version', exact: true }),
  ).toHaveText('Compare');
  await expect(page.locator('.score-pages.vertical')).toHaveCount(1);
  await expect(page.locator('.import, .history, .diagnostics, .selection')).toHaveCount(0);
  await expect(page.locator('.document-status, .hash')).toHaveCount(0);
  await expect(page.getByText('使用键盘选择小节')).toHaveCount(0);
  await expect(page.locator('.connection-light')).toBeVisible();
  await expect(page.locator('.score-toolbar, .score-shell')).toHaveCount(0);
  await page.getByLabel('Version', { exact: true }).click();
  await expect(
    page.getByText('The current workspace score matches the latest version'),
  ).toBeVisible();
  await expect(page.getByText('Version 2 · current version')).toHaveCount(0);
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test('missing and unknown documents use the visual 404 page and the old route is gone', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto('/scores');
  await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  await expect(page).toHaveTitle('页面不存在 · Ossia');
  await page.goto('/scores?documentId=55555555-5555-4555-8555-555555555555');
  await expect(page.getByRole('heading', { name: '页面不存在' })).toBeVisible();
  const legacy = await page.request.get(`/score.html?documentId=${firstId}`);
  expect(legacy.status()).toBe(404);
});

test('all pages share one selection and draw it directly on the score', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const secondPage = page.locator('.overlay').nth(1);
  const box = await secondPage.boundingBox();
  expect(box).not.toBeNull();
  await secondPage.click({ position: { x: box!.width * 0.4, y: (box!.height * 55) / 140 } });
  await expect(page.locator('.selection-box')).toHaveCount(1);
  await expect(page.locator('.selection-box')).toHaveAttribute('y', '50');
  const other = await context.newPage();
  await other.goto(`/scores?documentId=${firstId}`);
  await expect(other.locator('.selection-box')).toHaveCount(1);
  await page.reload();
  await expect(page.locator('.selection-box')).toHaveCount(1);
  const selected = page.locator('.overlay').nth(1);
  const selectedBox = await selected.boundingBox();
  await selected.click({
    position: { x: selectedBox!.width * 0.4, y: (selectedBox!.height * 55) / 140 },
  });
  await expect(page.locator('.selection-box')).toHaveCount(0);
});

test('layout, zoom and modifier pan apply to the whole document', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 900, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const vertical = await page
    .locator('.page-host')
    .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().toJSON()));
  expect(vertical[0]!.x).toBeCloseTo(vertical[1]!.x, 0);
  expect(vertical[1]!.y).toBeGreaterThan(vertical[0]!.y + vertical[0]!.height);
  expect(vertical[1]!.y - (vertical[0]!.y + vertical[0]!.height)).toBeCloseTo(24, 0);
  await expect(page.locator('.score-toolbar')).toHaveCount(0);
  await page.locator('.score-view').hover();
  await page.getByRole('button', { name: '横向' }).click();
  await expect(page.locator('.score-pages.horizontal')).toHaveCount(1);
  const horizontal = await page
    .locator('.page-host')
    .evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().toJSON()));
  expect(horizontal[0]!.y).toBeCloseTo(horizontal[1]!.y, 0);
  expect(horizontal[1]!.x).toBeGreaterThan(horizontal[0]!.x + horizontal[0]!.width);
  expect(horizontal[1]!.x - (horizontal[0]!.x + horizontal[0]!.width)).toBeCloseTo(24, 0);
  const firstSize = { width: horizontal[0]!.width, height: horizontal[0]!.height };
  const scroll = page.locator('.score-scroll');
  await scroll.hover();
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -100);
  await page.keyboard.up('Control');
  await expect(page.getByLabel('谱面缩放')).toHaveText('120%');
  await expect
    .poll(async () => (await page.locator('.page-host').first().boundingBox())!.height)
    .toBeGreaterThan(firstSize.height * 1.15);
  await scroll.dispatchEvent('wheel', {
    deltaY: -1,
    ctrlKey: true,
    clientX: 450,
    clientY: 400,
  });
  await expect(page.getByLabel('谱面缩放')).toHaveText('130%');
  expect(await scroll.evaluate((node) => node.scrollWidth > node.clientWidth)).toBe(true);
  await page.keyboard.down('Control');
  for (let index = 0; index < 11; index++) {
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(10);
  }
  await expect(page.getByLabel('谱面缩放')).toHaveText('400%');
  for (let index = 0; index < 16; index++) {
    await page.mouse.wheel(0, 100);
    await page.waitForTimeout(10);
  }
  await page.keyboard.up('Control');
  await expect(page.getByLabel('谱面缩放')).toHaveText('25%');
  await page.keyboard.down('Control');
  for (let index = 0; index < 10; index++) {
    await page.mouse.wheel(0, -100);
    await page.waitForTimeout(10);
  }
  await page.keyboard.up('Control');
  expect(await scroll.evaluate((node) => node.scrollWidth > node.clientWidth)).toBe(true);
  const beforePan = await scroll.evaluate((node) => node.scrollLeft);
  await page.keyboard.down('Control');
  await page.mouse.move(700, 400);
  await page.mouse.down();
  await page.mouse.move(500, 350);
  await page.mouse.up();
  await page.keyboard.up('Control');
  await expect
    .poll(async () => scroll.evaluate((node) => node.scrollLeft))
    .toBeGreaterThan(beforePan);
});

test('floating controls change zoom with buttons and a menu', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const score = page.locator('.score-view');
  await score.hover();
  await expect(page.locator('.score-controls')).toHaveCSS('opacity', '1');
  const zoom = page.getByLabel('谱面缩放');
  await expect(zoom).toHaveText('100%');
  await expect(zoom.locator('svg')).toHaveCount(0);
  await page.getByRole('button', { name: '放大' }).click();
  await expect(zoom).toHaveText('110%');
  await page.getByRole('button', { name: '缩小' }).click();
  await expect(zoom).toHaveText('100%');
  await zoom.click();
  await expect(page.locator('.score-controls')).toHaveCSS('opacity', '1');
  await page.getByRole('menuitemradio', { name: '200%' }).click();
  await expect(zoom).toHaveText('200%');
});

test('score page size stays fixed when the viewport changes', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const initial = await page.locator('.page-host').first().boundingBox();
  await page.setViewportSize({ width: 760, height: 640 });
  const resized = await page.locator('.page-host').first().boundingBox();
  expect(initial!.width).toBe(900);
  expect(resized!.width).toBeCloseTo(initial!.width, 5);
  expect(resized!.height).toBeCloseTo(initial!.height, 5);
});

test('version selection, restore order and export dialog use the viewed version', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByLabel('版本', { exact: true }).click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  const restore = page.getByRole('button', { name: '恢复至版本 1' });
  const version = page.getByRole('button', { name: '版本', exact: true });
  await expect(restore).toBeVisible();
  expect((await restore.boundingBox())!.x).toBeLessThan((await version.boundingBox())!.x);
  await page.getByRole('button', { name: '对比版本' }).click();
  await expect(page.getByRole('menuitemradio', { name: /版本 1 · 当前/ })).toHaveAttribute(
    'data-disabled',
    '',
  );
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '导出' }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog.getByRole('heading', { name: '导出乐谱' })).toBeVisible();
  await expect(dialog.getByRole('navigation', { name: '导出格式' })).toBeVisible();
  await expect(dialog.getByRole('heading', { name: '导出设置' })).toHaveCount(0);
  await expect(dialog.getByRole('heading', { name: '导出预览' })).toHaveCount(0);
  await expect(dialog.locator('.export-dialog-header p')).toHaveCount(0);
  expect(await dialog.locator('.export-format strong').allTextContents()).toEqual([
    'PDF',
    'PNG',
    'SVG',
    'MIDI',
  ]);
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  await expect(dialog.getByText('共 2 页')).toBeVisible();
  await dialog.getByLabel('PDF 密码').fill('secret');
  const pdfDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await pdfDownload).suggestedFilename()).toBe('合成钢琴练习 - v1.pdf');
  expect(h.outputs).toHaveLength(1);
  expect(h.outputs[0]).toMatchObject({
    format: 'pdf',
    page: null,
    width: null,
    pdfPassword: 'secret',
    pngQuality: null,
    pngTransparent: null,
    source: { kind: 'version', versionId: olderVersionId },
  });
  await expect(page.locator('.export-result')).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: /MIDI/ }).click();
  await expect(dialog.locator('.export-preview')).toHaveCount(0);
  await expect(dialog.locator('.export-dialog-body.no-preview')).toHaveCount(1);
  await dialog.getByRole('button', { name: /PNG/ }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText('有损压缩图')).toBeVisible();
  await expect(dialog.getByText('无损矢量图')).toBeVisible();
  await dialog.getByLabel('文件名').fill('版本导出');
  await dialog.getByLabel('像素宽度').fill('2400');
  const quality = dialog.getByLabel('图片质量');
  await quality.focus();
  await page.keyboard.press('Home');
  for (let index = 1; index < 72; index++) await page.keyboard.press('ArrowRight');
  await expect(quality).toHaveValue('72');
  await dialog.getByLabel('透明背景').check();
  await expect(dialog.getByLabel('页码')).toHaveCount(0);
  await expect(dialog.locator('.export-preview-stage img')).toHaveCount(2);
  const pngDownload = page.waitForEvent('download');
  await dialog.getByRole('button', { name: '导出', exact: true }).click();
  expect((await pngDownload).suggestedFilename()).toBe('版本导出.zip');
  expect(h.outputs).toHaveLength(3);
  expect(h.outputs[1]).toMatchObject({
    format: 'png',
    page: 1,
    width: 2400,
    pdfPassword: null,
    pngQuality: 72,
    pngTransparent: true,
    source: { kind: 'version', versionId: olderVersionId },
  });
  expect(h.outputs[2]).toMatchObject({
    format: 'png',
    page: 2,
    width: 2400,
    pdfPassword: null,
    pngQuality: 72,
    pngTransparent: true,
    source: { kind: 'version', versionId: olderVersionId },
  });
  await expect(page.locator('.export-result')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '打开位置' })).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).not.toBeVisible();
  await page.getByRole('button', { name: '恢复至版本 1' }).click();
  await page.getByRole('button', { name: '确认恢复' }).click();
  await expect.poll(() => h.restores).toHaveLength(1);
  expect(h.restores[0]).toMatchObject({
    versionId: olderVersionId,
    expectedSourceHash: h.states[0]!.sourceHash,
  });
});

test('comparison shows every page on both sides and keeps independent zoom', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '对比版本' }).click();
  const current = page.getByRole('menuitemradio', { name: /当前乐谱 · 当前/ });
  await expect(current).toHaveAttribute('data-disabled', '');
  const currentTitle = current.getByText('当前乐谱 · 当前');
  const currentDescription = current.getByText(/当前工作区乐谱与最新版本一致/);
  expect((await currentTitle.boundingBox())!.y).toBeLessThan(
    (await currentDescription.boundingBox())!.y,
  );
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(page.locator('.compare .score-image')).toHaveCount(4);
  await expect(page.getByRole('button', { name: '导出' })).toBeDisabled();
  await page.locator('.compare-pane').first().hover();
  await page.getByRole('button', { name: '横向' }).first().click();
  await expect(page.locator('.compare .score-pages.horizontal')).toHaveCount(2);
  await expect(page.locator('.compare .score-controls')).toHaveCount(2);
  await expect(page.locator('.score-toolbar')).toHaveCount(0);
  const zoom = page.getByLabel('谱面缩放');
  const leftScroll = page.locator('.compare-pane').first().locator('.score-scroll');
  await leftScroll.hover();
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -100);
  await page.keyboard.up('Control');
  await expect(zoom.nth(0)).toHaveText('120%');
  await expect(zoom.nth(1)).toHaveText('100%');
  await page.getByLabel('对比版本', { exact: true }).click();
  await page.getByRole('menuitem', { name: '退出对比' }).click();
  await expect(page.locator('.compare')).toHaveCount(0);
  await expect(page.locator('.score-image')).toHaveCount(2);
  expect(h.outputs).toHaveLength(0);
});

test('stale source clears selection without exposing diagnostics and reconnect works', async ({
  context,
  page,
}) => {
  const h = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const overlay = page.locator('.overlay').first();
  const box = await overlay.boundingBox();
  await overlay.click({ position: { x: box!.width * 0.4, y: (box!.height * 25) / 140 } });
  await expect(page.locator('.selection-box')).toHaveCount(1);
  h.change();
  await expect(page.locator('.selection-box')).toHaveCount(0);
  await page.getByLabel('版本', { exact: true }).click();
  await expect(page.getByText('上次成功结果：', { exact: false })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.locator('.diagnostics')).toHaveCount(0);
  h.disconnect(true);
  await expect(page.getByText('连接中断，正在重连…')).toBeVisible();
  await expect(page.locator('.toast.is-error')).toBeVisible();
  await expect(page.locator('.message.error')).toHaveCount(0);
  h.disconnect(false);
  await expect(page.getByText('已连接')).toBeVisible();
  await page.reload();
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
});

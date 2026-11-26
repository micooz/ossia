import { expect, test } from '@playwright/test';

import type { GeometryElement } from '../../src/generated/protocol.ts';
import { firstId, mockScoreRuntime } from './score-runtime.ts';

const modifier = process.platform === 'darwin' ? 'Meta' : 'Control';

function element(id: string, kind: string, x: number, y: number): GeometryElement {
  return {
    systemIndex: null,
    bounds: { x0: x, x1: x + 2, y0: y, y1: y + 2 },
    priority: 0,
    hitBounds: [],
    graphicIds: [id],
    linkedTargets: [{ kind: 'element', elementId: id }],
    region: {
      target: { kind: 'element', elementId: id },
      label: kind,
      elementKind: kind,
      spans: [
        {
          byteStart: 0,
          byteEnd: 1,
          start: { byte: 0, line: 1, column: 1 },
          end: { byte: 1, line: 1, column: 2 },
        },
      ],
      occurrences: [],
      insertionAnchor: null,
      staffIndex: null,
      voice: null,
      reason: null,
    },
  };
}

async function clickScore(
  page: import('@playwright/test').Page,
  x: number,
  y: number,
  options: { modifiers?: Array<'Meta' | 'Control'> } = {},
  pageIndex = 0,
) {
  const overlay = page.locator('.overlay').nth(pageIndex);
  const box = (await overlay.boundingBox())!;
  await overlay.click({
    position: { x: box.width * x, y: (box.height * y) / 140 },
    modifiers: options.modifiers,
  });
}

async function scorePoint(
  page: import('@playwright/test').Page,
  pageIndex: number,
  x: number,
  y: number,
) {
  const box = (await page.locator('.overlay').nth(pageIndex).boundingBox())!;
  return {
    x: box.x + box.width * x,
    y: box.y + (box.height * y) / 140,
  };
}

async function dragScore(
  page: import('@playwright/test').Page,
  start: { x: number; y: number },
  end: { x: number; y: number },
  holdMs = 0,
) {
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 8 });
  if (holdMs) await page.waitForTimeout(holdMs);
  await page.mouse.up();
}

async function enableAnnotations(page: import('@playwright/test').Page) {
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.keyboard.press('e');
  await expect(page.getByRole('button', { name: '关闭标注' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('button', { name: '关闭标注' })).toHaveAttribute(
    'data-tooltip',
    '关闭标注（E）',
  );
}

async function submitText(page: import('@playwright/test').Page, text: string) {
  await page.getByRole('textbox', { name: '标注' }).fill(text);
  await page.getByRole('button', { name: '提交标注' }).click();
}

test('dragging blank space frames all touched elements into one annotation', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, {
    elementsByPage: {
      1: [
        element('first-note', 'NoteHead', 30, 24),
        element('second-note', 'NoteHead', 60, 24),
        element('outside-note', 'NoteHead', 80, 24),
      ],
    },
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await dragScore(page, await scorePoint(page, 0, 0.05, 10), await scorePoint(page, 0, 0.7, 30));
  await expect(page.getByRole('textbox', { name: '标注' })).toBeFocused();
  await submitText(page, '框住前两个音符');

  await expect(page.getByRole('button', { name: '1. 框住前两个音符' })).toHaveCount(1);
  await expect(page.locator('.score-annotation-box')).toHaveCount(1);
  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets)
    .toEqual([
      { kind: 'element', elementId: 'first-note' },
      { kind: 'element', elementId: 'second-note' },
    ]);
  const boxes = runtime.selections.get(firstId)?.annotations[0]?.boxes;
  expect(boxes).toHaveLength(1);
  expect(boxes?.[0]?.page).toBe(1);
  expect(boxes?.[0]?.pin).toBe(true);
  expect(runtime.selections.get(firstId)?.targets).toEqual([]);
});

test('@platform-keyboard modifier-drag accumulates separate marquee regions into one draft group', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, {
    elementsByPage: {
      1: [element('first-note', 'NoteHead', 30, 24), element('second-note', 'NoteHead', 60, 24)],
    },
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await page.keyboard.down(modifier);
  await dragScore(page, await scorePoint(page, 0, 0.05, 10), await scorePoint(page, 0, 0.4, 30));
  await expect(page.locator('.score-annotation-pin.is-draft')).toHaveCount(1);
  await expect(page.locator('.score-annotation-box.is-draft')).toHaveCount(1);
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveCount(0);
  await dragScore(page, await scorePoint(page, 0, 0.5, 10), await scorePoint(page, 0, 0.7, 30));
  await expect(page.locator('.score-annotation-pin.is-draft')).toHaveCount(2);
  await expect(page.locator('.score-annotation-box.is-draft')).toHaveCount(2);
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveCount(0);
  await page.keyboard.up(modifier);
  await submitText(page, '两次框选同组');

  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets)
    .toEqual([
      { kind: 'element', elementId: 'first-note' },
      { kind: 'element', elementId: 'second-note' },
    ]);
  expect(runtime.selections.get(firstId)?.annotations[0]?.boxes).toHaveLength(2);
});

test('marquee groups may share elements and expose each group through stacked pins', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, {
    elementsByPage: {
      1: [element('shared-note', 'NoteHead', 30, 24)],
    },
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  const start = await scorePoint(page, 0, 0.05, 10);
  const end = await scorePoint(page, 0, 0.4, 30);
  await dragScore(page, start, end);
  await submitText(page, '第一组');
  await dragScore(page, start, end);
  await submitText(page, '第二组');

  await expect.poll(() => runtime.selections.get(firstId)?.annotations).toHaveLength(2);
  expect(runtime.selections.get(firstId)?.annotations[0]?.targets).toEqual([
    { kind: 'element', elementId: 'shared-note' },
  ]);
  expect(runtime.selections.get(firstId)?.annotations[1]?.targets).toEqual([
    { kind: 'element', elementId: 'shared-note' },
  ]);
  const firstPin = page.getByRole('button', { name: '1. 第一组' });
  const secondPin = page.getByRole('button', { name: '2. 第二组' });
  const firstBox = (await firstPin.boundingBox())!;
  const secondBox = (await secondPin.boundingBox())!;
  const renderedBoxes = page.locator('.score-annotation-box');
  await expect(renderedBoxes).toHaveCount(2);
  const boxColors = await renderedBoxes.evaluateAll((nodes) =>
    nodes.map((node) => getComputedStyle(node).borderTopColor),
  );
  expect(new Set(boxColors).size).toBe(2);
  expect(firstBox.x).not.toBeCloseTo(secondBox.x, 0);
  expect(firstBox.y).not.toBeCloseTo(secondBox.y, 0);

  await firstPin.click();
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveValue('第一组');
  await page.keyboard.press('Escape');
  await secondPin.click();
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveValue('第二组');
});

test('marquee selects a small note it partially covers', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context, {
    elementsByPage: {
      1: [element('tab-note', 'NoteHead', 30, 55)],
    },
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await dragScore(page, await scorePoint(page, 0, 0.25, 50), await scorePoint(page, 0, 0.31, 56));
  await submitText(page, '只框住一半数字');

  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets)
    .toEqual([{ kind: 'element', elementId: 'tab-note' }]);
});

test('marquee target resolution spans pages in horizontal layout', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context, {
    elementsByPage: {
      1: [element('page-one-note', 'NoteHead', 30, 24)],
      2: [element('page-two-note', 'NoteHead', 30, 24)],
    },
  });
  await page.setViewportSize({ width: 1900, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.locator('.score-view').hover();
  await page.getByRole('button', { name: '横向' }).click();
  await expect(page.locator('.score-pages.horizontal')).toHaveCount(1);
  await enableAnnotations(page);
  await dragScore(page, await scorePoint(page, 0, 0.05, 10), await scorePoint(page, 1, 0.7, 30));
  await submitText(page, '跨页框选');

  await expect(page.getByRole('button', { name: '1. 跨页框选' })).toHaveCount(1);
  await expect(page.locator('.score-annotation-box')).toHaveCount(2);
  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets)
    .toEqual([
      { kind: 'element', elementId: 'page-one-note' },
      { kind: 'element', elementId: 'page-two-note' },
    ]);
  const boxes = runtime.selections.get(firstId)?.annotations[0]?.boxes;
  expect(boxes?.map((box) => box.page)).toEqual([1, 2]);
  expect(boxes?.map((box) => box.pin)).toEqual([true, false]);
});

test('marquee auto-pans to reach targets below the viewport', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context, {
    eventsDelay: 10_000,
    elementsByPage: {
      1: [],
      2: [element('page-two-note', 'NoteHead', 30, 24)],
    },
  });
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  const scroll = page.locator('.score-scroll');
  const firstPage = page.locator('.page-host').first();
  const initialTop = (await firstPage.boundingBox())!.y;
  const scrollBox = (await scroll.boundingBox())!;
  const start = await scorePoint(page, 0, 0.05, 5);
  await dragScore(
    page,
    start,
    { x: scrollBox.x + scrollBox.width * 0.6, y: scrollBox.y + scrollBox.height - 2 },
    3000,
  );
  await expect
    .poll(async () => initialTop - (await firstPage.boundingBox())!.y)
    .toBeGreaterThan(1200);
  await expect(page.locator('.score-annotation-editor')).toHaveCount(1, { timeout: 3000 });
  await submitText(page, '滚动到第二页');

  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets)
    .toEqual([{ kind: 'element', elementId: 'page-two-note' }]);
  expect((await firstPage.boundingBox())!.y).toBeLessThan(initialTop);
});

test('an empty marquee does not create an annotation', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await dragScore(page, await scorePoint(page, 0, 0.05, 120), await scorePoint(page, 0, 0.2, 135));
  await expect(page.locator('.score-annotation-editor')).toHaveCount(0);
  expect(runtime.selections.get(firstId)).toBeUndefined();
});

test('annotation editor avoids selected elements and stays inside the score viewport', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, {
    elementsByPage: {
      1: [element('lower-note', 'NoteHead', 30, 55), element('upper-note', 'NoteHead', 60, 24)],
    },
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await dragScore(page, await scorePoint(page, 0, 0.05, 10), await scorePoint(page, 0, 0.7, 65));

  const editor = (await page.locator('.score-annotation-editor').boundingBox())!;
  const scroll = (await page.locator('.score-scroll').boundingBox())!;
  const host = (await page.locator('.page-host').first().boundingBox())!;
  const noteBounds = [
    { x: 30, y: 55 },
    { x: 60, y: 24 },
  ].map((note) => ({
    x0: host.x + (host.width * note.x) / 100,
    x1: host.x + (host.width * (note.x + 2)) / 100,
    y0: host.y + (host.height * note.y) / 140,
    y1: host.y + (host.height * (note.y + 2)) / 140,
  }));
  const overlap = (bounds: (typeof noteBounds)[number]) => {
    const width = Math.max(
      0,
      Math.min(editor.x + editor.width, bounds.x1) - Math.max(editor.x, bounds.x0),
    );
    const height = Math.max(
      0,
      Math.min(editor.y + editor.height, bounds.y1) - Math.max(editor.y, bounds.y0),
    );
    return width * height;
  };

  expect(noteBounds.map(overlap)).toEqual([0, 0]);
  expect(editor.x).toBeGreaterThanOrEqual(scroll.x + 8);
  expect(editor.y).toBeGreaterThanOrEqual(scroll.y + 8);
  expect(editor.x + editor.width).toBeLessThanOrEqual(scroll.x + scroll.width - 8);
  expect(editor.y + editor.height).toBeLessThanOrEqual(scroll.y + scroll.height - 8);
});

test('annotation mode draws a single group and supports hover, edit and delete', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await expect(page.getByRole('textbox', { name: '标注' })).toBeFocused();
  await expect(page.locator('.annotation-box')).toHaveCount(1);
  const submit = page.getByRole('button', { name: '提交标注' });
  const disabledBackground = await submit.evaluate(
    (node) => getComputedStyle(node).backgroundColor,
  );
  await page.getByRole('textbox', { name: '标注' }).fill('  这里要改弱一点  ');
  await expect(submit).toBeEnabled();
  expect(await submit.evaluate((node) => getComputedStyle(node).backgroundColor)).not.toBe(
    disabledBackground,
  );
  await submit.click();

  const pin = page.getByRole('button', { name: '1. 这里要改弱一点' });
  await expect(pin).toHaveCount(1);
  await expect(page.locator('.annotation-box')).toHaveCount(1);
  await expect
    .poll(() => runtime.selections.get(firstId)?.annotations[0]?.text)
    .toBe('这里要改弱一点');
  expect(runtime.selections.get(firstId)?.annotations[0]?.targets).toEqual([
    {
      kind: 'measure',
      locator: { staffIndex: 0, measureStart: 1, measureEnd: 1, voice: null },
    },
  ]);
  expect(runtime.selections.get(firstId)?.targets).toEqual([]);

  await pin.hover();
  const tooltip = page.getByRole('tooltip');
  await expect(tooltip).toHaveCSS('opacity', '1');
  const arrow = await tooltip.evaluate((node) => {
    const tooltipStyle = getComputedStyle(node);
    const arrowStyle = getComputedStyle(node, '::after');
    return {
      backgroundColor: tooltipStyle.backgroundColor,
      borderTopColor: arrowStyle.borderTopColor,
      borderTopWidth: arrowStyle.borderTopWidth,
      content: arrowStyle.content,
    };
  });
  expect(arrow.content).toBe('""');
  expect(arrow.borderTopWidth).toBe('6px');
  expect(arrow.borderTopColor).toBe(arrow.backgroundColor);
  await pin.click();
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveValue('这里要改弱一点');
  await page.getByRole('textbox', { name: '标注' }).fill('改成渐弱');
  await page.getByRole('button', { name: '提交标注' }).click();
  await expect(page.getByRole('button', { name: '1. 改成渐弱' })).toBeVisible();

  await page.getByRole('button', { name: '1. 改成渐弱' }).click();
  await page.getByRole('button', { name: '删除这组标注' }).click();
  await expect(page.locator('.score-annotation-pin')).toHaveCount(0);
  await expect.poll(() => runtime.selections.get(firstId)?.annotations.length ?? 0).toBe(0);
});

test('escape exits annotation mode and discards the current editor', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await expect(page.getByRole('textbox', { name: '标注' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveCount(0);
  await expect(page.getByRole('main')).toBeFocused();
});

test('@platform-keyboard holding the annotation modifier defers one editor for multiple targets', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await page.keyboard.down(modifier);
  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await clickScore(page, 0.4, 55, { modifiers: [modifier] });
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveCount(0);
  await expect(page.locator('.score-annotation-pin')).toHaveCount(2);
  await expect(page.locator('.score-annotation-pin.is-draft')).toHaveText(['1', '1']);
  await page.keyboard.up(modifier);
  await expect(page.getByRole('textbox', { name: '标注' })).toBeVisible();
  await submitText(page, '这两处一起改');

  await expect(page.getByRole('button', { name: '1. 这两处一起改' })).toHaveCount(2);
  await expect.poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets.length).toBe(2);
});

test('clicking the same single annotation target keeps the editor and draft text', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  const input = page.getByRole('textbox', { name: '标注' });
  await input.fill('未提交的草稿');
  const editor = page.locator('.score-annotation-editor');
  await editor.evaluate((node) => node.setAttribute('data-test-editor', 'original'));

  await clickScore(page, 0.4, 25);

  await expect(input).toHaveValue('未提交的草稿');
  await expect(editor).toHaveAttribute('data-test-editor', 'original');
  await expect(page.locator('.annotation-box')).toHaveCount(1);
});

test('@platform-keyboard modifier-clicking a selected target removes it from the annotation group', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await page.keyboard.down(modifier);
  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await clickScore(page, 0.4, 55, { modifiers: [modifier] });
  const draftPins = page.locator('.score-annotation-pin.is-draft');
  await expect(draftPins).toHaveCount(2);

  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await expect(draftPins).toHaveCount(1);
  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await expect(draftPins).toHaveCount(2);
  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await expect(draftPins).toHaveCount(1);

  await page.keyboard.up(modifier);
  await submitText(page, '只保留第二处');
  await expect.poll(() => runtime.selections.get(firstId)?.annotations[0]?.targets.length).toBe(1);
  expect(runtime.selections.get(firstId)?.annotations[0]?.targets).toEqual([
    {
      kind: 'measure',
      locator: { staffIndex: 1, measureStart: 1, measureEnd: 1, voice: null },
    },
  ]);
});

test('submitted pins remain editable after annotation mode is closed', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await submitText(page, '保留这条');
  await page.keyboard.press('e');
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
  const pin = page.getByRole('button', { name: '1. 保留这条' });
  await expect(pin).toBeVisible();
  await pin.click();
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveValue('保留这条');
});

test('the bottom badge summarizes annotations and clears them', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlayBox = (await page.locator('.overlay').first().boundingBox())!;
  await page.mouse.click(
    overlayBox.x + overlayBox.width * 0.4,
    overlayBox.y + (overlayBox.height * 25) / 140,
  );
  await expect(page.locator('.annotation-box')).toHaveCount(0);
  await expect(page.locator('.score-annotation-badge')).toHaveCount(0);
  expect(runtime.selections.get(firstId)).toBeUndefined();

  await enableAnnotations(page);
  await clickScore(page, 0.4, 55);
  await submitText(page, '底部回显');
  const badge = page.locator('.score-annotation-badge');
  await expect(badge).toContainText('已添加 1 个标注');
  await expect(badge).not.toContainText('。');
  await expect(badge).not.toContainText('/ossia');
  await expect(badge).not.toContainText('已选择');
  await expect(badge).toHaveCSS('border-top-width', '0px');
  await expect(badge).toHaveCSS('background-color', 'rgba(30, 30, 34, 0.92)');
  await expect(badge).toHaveCSS('color', 'rgb(255, 255, 255)');
  const badgeBox = (await badge.boundingBox())!;
  const viewport = page.viewportSize()!;
  expect(badgeBox.x + badgeBox.width / 2).toBeCloseTo(viewport.width / 2, 0);
  expect(viewport.height - (badgeBox.y + badgeBox.height)).toBeCloseTo(16, 0);
  await expect(page.getByRole('button', { name: '1. 底部回显' })).toHaveCount(1);

  const clear = page.getByRole('button', { name: '清除标注' });
  await expect(clear).toHaveAttribute('data-tooltip', '清除标注');
  await clear.hover();
  await expect
    .poll(() => clear.evaluate((node) => getComputedStyle(node, '::after').opacity))
    .toBe('1');
  await clear.click();
  await expect(badge).toHaveCount(0);
  await expect(page.locator('.annotation-box')).toHaveCount(0);
  await expect(page.locator('.score-annotation-pin')).toHaveCount(0);
  await expect.poll(() => runtime.selections.get(firstId)).toBeNull();
});

test('annotation groups use distinct colors for pins, measures and source graphics', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  const draftBox = page.locator('.annotation-box').first();
  const draftColor = await draftBox.evaluate((node) => getComputedStyle(node).stroke);
  await submitText(page, '第一组');
  const firstPin = page.getByRole('button', { name: '1. 第一组' });
  await expect(firstPin).toHaveCSS('background-color', draftColor);
  await clickScore(page, 0.4, 55);
  await submitText(page, '第二组');
  const secondPin = page.getByRole('button', { name: '2. 第二组' });
  const firstColor = await firstPin.evaluate((node) => getComputedStyle(node).backgroundColor);
  const secondColor = await secondPin.evaluate((node) => getComputedStyle(node).backgroundColor);
  expect(secondColor).not.toBe(firstColor);
  const strokes = await page
    .locator('.annotation-box')
    .evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).stroke));
  expect(new Set(strokes).size).toBe(2);
});

test('different annotation groups paint their source graphics with distinct colors', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, {
    elements: [
      element('first-note', 'NoteHead', 30, 24),
      element('second-note', 'NoteHead', 60, 24),
    ],
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.31, 25);
  await submitText(page, '第一个音符');
  await clickScore(page, 0.61, 25);
  await submitText(page, '第二个音符');
  const firstFill = await page
    .locator('[data-ossia-graphic="first-note"] path')
    .first()
    .evaluate((node) => getComputedStyle(node).fill);
  const secondFill = await page
    .locator('[data-ossia-graphic="second-note"] path')
    .first()
    .evaluate((node) => getComputedStyle(node).fill);
  expect(firstFill).not.toBe(secondFill);
});

test('@platform-keyboard measure pins stay inside their measure and draft groups share one color', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await page.keyboard.down(modifier);
  await clickScore(page, 0.4, 25, { modifiers: [modifier] });
  await clickScore(page, 0.4, 55, { modifiers: [modifier] });
  const draftPins = page.locator('.score-annotation-pin.is-draft');
  await expect(draftPins).toHaveCount(2);
  const draftColors = await draftPins.evaluateAll((nodes) =>
    nodes.map((node) => getComputedStyle(node).backgroundColor),
  );
  expect(new Set(draftColors).size).toBe(1);
  await page.keyboard.up(modifier);
  await submitText(page, '同一组');

  const firstPin = page.getByRole('button', { name: '1. 同一组' }).first();
  const pinBox = (await firstPin.boundingBox())!;
  const hostBox = (await page.locator('.page-host').first().boundingBox())!;
  const measure = {
    x0: hostBox.x + hostBox.width * 0.1,
    x1: hostBox.x + hostBox.width * 0.8,
    y0: hostBox.y + (hostBox.height * 20) / 140,
    y1: hostBox.y + (hostBox.height * 30) / 140,
  };
  const pinX = pinBox.x + pinBox.width / 2;
  const pinY = pinBox.y + pinBox.height / 2;
  expect(pinX).toBeGreaterThan(measure.x0);
  expect(pinX).toBeLessThan(measure.x1);
  expect(pinY).toBeGreaterThan(measure.y0);
  expect(pinY).toBeLessThan(measure.y1);
});

test('note annotations color the note and place the pin away from it', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, {
    elements: [element('note', 'NoteHead', 30, 24), element('neighbor', 'NoteHead', 34, 24)],
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  const overlay = page.locator('.overlay').first();
  const overlayBox = (await overlay.boundingBox())!;
  await overlay.click({
    position: { x: (overlayBox.width * 31) / 100, y: (overlayBox.height * 25) / 140 },
  });
  await submitText(page, '标注音符');
  await page.keyboard.press('e');
  await expect(page.locator('.score-annotation-layer svg')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="note"]').first()).toHaveClass(
    'element-selection',
  );

  const pinBox = (await page.getByRole('button', { name: '1. 标注音符' }).first().boundingBox())!;
  const hostBox = (await page.locator('.page-host').first().boundingBox())!;
  const noteBox = {
    x0: hostBox.x + (hostBox.width * 30) / 100,
    x1: hostBox.x + (hostBox.width * 32) / 100,
    y0: hostBox.y + (hostBox.height * 24) / 140,
    y1: hostBox.y + (hostBox.height * 26) / 140,
  };
  expect(
    pinBox.x < noteBox.x1 &&
      pinBox.x + pinBox.width > noteBox.x0 &&
      pinBox.y < noteBox.y1 &&
      pinBox.y + pinBox.height > noteBox.y0,
  ).toBe(false);
});

test('non-note annotations use source coloring instead of a hit box', async ({ context, page }) => {
  await mockScoreRuntime(context, {
    elements: [element('title', 'header-title', 30, 5)],
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  const overlay = page.locator('.overlay').first();
  const overlayBox = (await overlay.boundingBox())!;
  await overlay.click({
    position: { x: (overlayBox.width * 31) / 100, y: (overlayBox.height * 6) / 140 },
  });
  await submitText(page, '标注标题');
  await expect(page.locator('[data-ossia-graphic="title"]').first()).toHaveClass(
    'element-selection',
  );
  await expect(page.locator('.score-annotation-layer svg')).toHaveCount(0);
  await expect(page.locator('.annotation-box')).toHaveCount(0);
});

test('annotation pins scale with the score zoom', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await submitText(page, '跟着缩放');
  const pin = page.getByRole('button', { name: '1. 跟着缩放' });
  const initial = await pin.boundingBox();
  expect(initial!.width).toBeCloseTo(20, 0);
  await page.locator('.score-view').hover();
  await page.getByLabel('谱面缩放').click();
  await page.getByRole('menuitemradio', { name: '200%', exact: true }).click();
  await expect
    .poll(async () => (await pin.boundingBox())!.width)
    .toBeCloseTo(initial!.width * 2, 0);
  await page.getByLabel('谱面缩放').click();
  await page.getByRole('menuitemradio', { name: '50%', exact: true }).click();
  await expect
    .poll(async () => (await pin.boundingBox())!.width)
    .toBeCloseTo(initial!.width / 2, 0);
});

test('history playback hides annotation controls and leaves annotation mode', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(page.locator('.score-feature-toolbar')).toBeVisible();
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeVisible();
  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 2/ }).click();
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
  await expect(page.getByRole('button', { name: '对比版本' })).toHaveCount(0);
});

test('annotation groups synchronize across tabs and clear when the source changes', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const other = await context.newPage();
  await other.goto(`/scores?documentId=${firstId}`);
  await expect(other.locator('.score-image')).toHaveCount(2);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await submitText(page, '跨标签同步');
  await expect(other.getByRole('button', { name: '1. 跨标签同步' })).toBeVisible();

  runtime.change();
  await expect(page.locator('.score-annotation-pin')).toHaveCount(0);
  await expect(page.locator('.score-feature-toolbar')).toHaveCount(0);
});

test('voice input appends browser speech recognition results', async ({ context, page }) => {
  await page.addInitScript(() => {
    class FakeSpeechRecognition {
      lang = '';
      onend: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onresult: ((event: unknown) => void) | null = null;
      abort() {
        this.onend?.();
      }
      start() {
        setTimeout(() => {
          this.onresult?.({
            results: { 0: { 0: { transcript: '更轻一点' } } },
          });
        }, 20);
      }
    }
    const candidate = window as unknown as {
      SpeechRecognition?: unknown;
      webkitSpeechRecognition?: unknown;
    };
    candidate.SpeechRecognition = FakeSpeechRecognition;
    candidate.webkitSpeechRecognition = FakeSpeechRecognition;
  });
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await enableAnnotations(page);
  await clickScore(page, 0.4, 25);
  await page.getByRole('button', { name: '语音输入' }).click();
  await expect(page.getByRole('textbox', { name: '标注' })).toHaveValue('更轻一点');
});

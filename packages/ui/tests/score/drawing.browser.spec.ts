import { expect, test } from '@playwright/test';

import type { Drawing, DrawingStroke } from '../../src/generated/protocol.ts';
import { firstId, hash, mockScoreRuntime } from './score-runtime.ts';

test('an existing drawing waits until its page score has painted', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  const stroke: DrawingStroke = {
    page: 1,
    width: 0.01,
    color: 'black',
    points: [
      { x: 0.2, y: 0.2 },
      { x: 0.6, y: 0.2 },
    ],
  };
  runtime.drawings.set(firstId, { revision: 1, strokes: [stroke] });
  let releasePage!: () => void;
  const pageBlocked = new Promise<void>((resolve) => {
    releasePage = resolve;
  });
  await context.route(`**/api/v1/renders/${hash}/pages/1`, async (route) => {
    await pageBlocked;
    await route.fallback();
  });
  const drawingRead = page.waitForResponse(
    (response) =>
      response.url().endsWith(`/documents/${firstId}/drawing`) &&
      response.request().method() === 'GET',
  );
  await page.goto(`/scores?documentId=${firstId}`, { waitUntil: 'commit' });
  await drawingRead;
  const host = page.locator('[data-page="1"]');
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toHaveAttribute(
    'aria-disabled',
    'false',
  );
  await expect(host.locator('.score-image')).toHaveCount(0);
  await expect(host.locator('.score-freehand-layer')).toHaveCount(0);
  await page.evaluate(() => {
    Reflect.set(window, 'renderFrames', []);
    Reflect.set(window, 'watchRenderFrames', true);
    const sample = () => {
      if (!Reflect.get(window, 'watchRenderFrames')) return;
      const page = document.querySelector('[data-page="1"]');
      const score = page?.querySelector('.score-image') ? 1 : 0;
      const drawing = page?.querySelector('.score-freehand-layer') ? 1 : 0;
      (Reflect.get(window, 'renderFrames') as string[]).push(`${score}:${drawing}`);
      requestAnimationFrame(sample);
    };
    requestAnimationFrame(sample);
  });
  releasePage();
  await expect(host.locator('.score-image')).toHaveCount(1);
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
  await page.evaluate(() => Reflect.set(window, 'watchRenderFrames', false));
  expect(await page.evaluate(() => Reflect.get(window, 'renderFrames'))).toContain('1:0');
});

test('drawing suppresses stylus touch defaults without blocking finger gestures', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const dispatch = (type: 'touchstart' | 'touchmove' | 'touchend', stylus: boolean) =>
    page
      .locator('.page-host')
      .first()
      .evaluate(
        (target, args) => {
          const event = new Event(args.type, { bubbles: true, cancelable: true });
          Object.defineProperty(event, 'changedTouches', {
            value: [{ touchType: args.stylus ? 'stylus' : 'direct' }],
          });
          return target.dispatchEvent(event);
        },
        { type, stylus },
      );
  expect(await dispatch('touchstart', true)).toBe(true);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  expect(await dispatch('touchstart', true)).toBe(false);
  expect(await dispatch('touchmove', true)).toBe(false);
  expect(await dispatch('touchend', true)).toBe(false);
  expect(await dispatch('touchstart', false)).toBe(true);
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  expect(await dispatch('touchstart', true)).toBe(false);
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  expect(await dispatch('touchstart', true)).toBe(true);
});

test('a delayed capture release does not interrupt the next pen or eraser stroke', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const x = box.x + box.width * 0.3;
  const y = box.y + box.height * 0.2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 100, y);
  await page.mouse.up();
  await page.mouse.move(x, y + 40);
  await page.mouse.down();
  await page
    .locator('.score-scroll')
    .dispatchEvent('lostpointercapture', { pointerId: 1, pointerType: 'mouse', bubbles: true });
  await page.mouse.move(x + 100, y + 40);
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(2);
  expect(runtime.drawings.get(firstId)!.strokes[1]!.points.length).toBeGreaterThan(1);
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await page.mouse.move(x + 30, y - 20);
  await page.mouse.down();
  await page.mouse.move(x + 30, y + 20);
  await page.mouse.up();
  await page.mouse.move(x + 70, y + 20);
  await page.mouse.down();
  await page
    .locator('.score-scroll')
    .dispatchEvent('lostpointercapture', { pointerId: 1, pointerType: 'mouse', bubbles: true });
  await page.mouse.move(x + 70, y + 60);
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(4);
});

test('starting a two-finger pinch does not leave a drawing dot', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const x = box.x + box.width * 0.4;
  const y = box.y + box.height * 0.2;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x, y, id: 1 }],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [
      { x, y, id: 1 },
      { x: x + 50, y, id: 2 },
    ],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [
      { x: x - 30, y, id: 1 },
      { x: x + 80, y, id: 2 },
    ],
  });
  await expect(page.locator('.score-pen-indicator')).toHaveCount(0);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect(page.locator('.score-pen-indicator')).toHaveCount(0);
  await expect(page.locator('.score-freehand-layer circle')).toHaveCount(0);
  expect(runtime.drawings.get(firstId)?.strokes ?? []).toEqual([]);
});

test('drawing keeps the dispatched pointer position after coalesced samples', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.addInitScript(() => {
    PointerEvent.prototype.getCoalescedEvents = function () {
      return [
        new PointerEvent('pointermove', { clientX: this.clientX - 20, clientY: this.clientY }),
      ];
    };
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const x = box.x + box.width * 0.4;
  const y = box.y + box.height * 0.2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 60, y);
  await page.mouse.move(x + 30, y);
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  const points = runtime.drawings.get(firstId)!.strokes[0]!.points;
  expect(Math.max(...points.map((point) => point.x))).toBeCloseTo((x + 60 - box.x) / box.width, 3);
});

test('drawing indicator follows the score when its containing page is shifted', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await page.addStyleTag({ content: 'body { transform: translateY(40px) }' });
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const x = box.x + box.width * 0.4;
  const y = box.y + box.height * 0.2;
  await page.mouse.move(x, y);
  const indicator = page.locator('.score-pen-indicator');
  await expect(indicator).toBeVisible();
  const bounds = (await indicator.boundingBox())!;
  expect(bounds.x + bounds.width / 2).toBeCloseTo(x, 0);
  expect(bounds.y + bounds.height / 2).toBeCloseTo(y, 0);
});

test('fast pen and eraser gestures include the release position', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const x = box.x + box.width * 0.4;
  const y = box.y + box.height * 0.2;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x,
    y,
    button: 'left',
    buttons: 1,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: x + 100,
    y,
    button: 'left',
    buttons: 0,
    clickCount: 1,
    pointerType: 'pen',
  });
  await expect
    .poll(() => runtime.drawings.get(firstId)?.strokes[0]?.points.at(-1)?.x)
    .toBeCloseTo((x + 100 - box.x) / box.width, 3);
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x: x + 50,
    y: y - 30,
    button: 'left',
    buttons: 1,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: x + 50,
    y: y + 30,
    button: 'left',
    buttons: 0,
    clickCount: 1,
    pointerType: 'pen',
  });
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(2);
});

test('drawing and erasing update the visible score before the pointer is released', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.addStyleTag({
    content: '.score-pen-indicator, .score-eraser-indicator { opacity: 0 }',
  });
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await page.getByRole('button', { name: '红色' }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  const x = box.x + box.width * 0.4;
  const y = box.y + box.height * 0.2;
  const clip = { x: x + 35, y: y - 5, width: 10, height: 10 };
  const slowClip = { x: x + 6, y: y - 3, width: 4, height: 6 };
  const farClip = { x: x + 65, y: y - 5, width: 10, height: 10 };
  const before = await page.screenshot({ clip });
  const slowBefore = await page.screenshot({ clip: slowClip });
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 12, y, { steps: 12 });
  expect((await page.screenshot({ clip: slowClip })).equals(slowBefore)).toBe(false);
  await page.mouse.move(x + 80, y, { steps: 20 });
  const drawing = await page.screenshot({ clip });
  const farDrawing = await page.screenshot({ clip: farClip });
  expect(drawing.equals(before)).toBe(false);
  await page.mouse.up();
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await page.mouse.move(x + 40, y);
  await page.mouse.down();
  const erasing = await page.screenshot({ clip });
  expect(erasing.equals(drawing)).toBe(false);
  expect((await page.screenshot({ clip: farClip })).equals(farDrawing)).toBe(true);
  await page.mouse.up();
  const erased = await page.screenshot({ clip });
  expect(erased.equals(erasing)).toBe(true);
});

test('drawing remains idle without polling while the event stream is connected', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, { eventsDelay: 30_000 });
  let reads = 0;
  await context.route(`**/api/v1/documents/${firstId}/drawing`, async (route) => {
    if (route.request().method() === 'GET') reads++;
    await route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toBeVisible();
  await page.waitForTimeout(2_300);
  expect(reads).toBe(1);
});

test('drawing refreshes again when a change arrives during an in-flight read', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, { eventsDelay: 30_000 });
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  let reads = 0;
  await context.route(`**/api/v1/documents/${firstId}/drawing`, async (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    reads++;
    if (reads === 2) {
      const snapshot = structuredClone(runtime.drawings.get(firstId));
      await blocked;
      return route.fulfill({ json: snapshot });
    }
    return route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toBeVisible();
  const stroke: DrawingStroke = {
    page: 1,
    width: 0.01,
    color: 'black',
    points: [
      { x: 0.2, y: 0.2 },
      { x: 0.3, y: 0.3 },
    ],
  };
  runtime.drawings.set(firstId, { revision: 1, strokes: [stroke] });
  await page.evaluate(
    (id) =>
      window.dispatchEvent(
        new CustomEvent('ossia-drawing-changed', { detail: { documentId: id, revision: 1 } }),
      ),
    firstId,
  );
  await expect.poll(() => reads).toBe(2);
  runtime.drawings.set(firstId, { revision: 2, strokes: [stroke, stroke] });
  await page.evaluate(
    (id) =>
      window.dispatchEvent(
        new CustomEvent('ossia-drawing-changed', { detail: { documentId: id, revision: 2 } }),
      ),
    firstId,
  );
  release();
  await expect(
    page.locator('.page-host').first().locator('.score-freehand-layer polyline'),
  ).toHaveCount(2, { timeout: 1_500 });
});

test('drawing catches up after the event stream reconnects', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  const connected = page.waitForResponse((response) => response.url().includes('/api/v1/events?'));
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toBeVisible();
  await connected;
  const failed = page.waitForEvent('requestfailed', (request) =>
    request.url().includes('/api/v1/events?'),
  );
  runtime.disconnect(true);
  await failed;
  runtime.drawings.set(firstId, {
    revision: 1,
    strokes: [
      {
        page: 1,
        width: 0.01,
        color: 'black',
        points: [
          { x: 0.2, y: 0.2 },
          { x: 0.3, y: 0.3 },
        ],
      },
    ],
  });
  runtime.disconnect(false);
  await expect(
    page.locator('.page-host').first().locator('.score-freehand-layer polyline'),
  ).toHaveCount(1, { timeout: 4_000 });
});

test('an initial drawing read can be retried after failure', async ({ context, page }) => {
  await mockScoreRuntime(context, { eventsDelay: 30_000 });
  let fail = true;
  await context.route(`**/api/v1/documents/${firstId}/drawing`, async (route) => {
    if (route.request().method() === 'GET' && fail) {
      fail = false;
      return route.fulfill({
        status: 500,
        json: { error: { code: 'storage-error', message: 'unavailable' } },
      });
    }
    return route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '绘画保存失败' }).click();
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toBeVisible();
});

test('a remote drawing change during a save is fetched after the save finishes', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context, { eventsDelay: 30_000 });
  let release!: () => void;
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  let saved: Drawing | undefined;
  await context.route(`**/api/v1/documents/${firstId}/drawing`, async (route) => {
    if (route.request().method() !== 'PUT') return route.fallback();
    saved = { ...route.request().postDataJSON(), revision: 1 };
    await blocked;
    return route.fulfill({ json: saved });
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.3);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.3);
  await page.mouse.up();
  await expect.poll(() => saved).toBeDefined();
  const remote: DrawingStroke = {
    page: 1,
    width: 0.01,
    color: 'black',
    points: [
      { x: 0.2, y: 0.4 },
      { x: 0.3, y: 0.4 },
    ],
  };
  runtime.drawings.set(firstId, { revision: 2, strokes: [...saved!.strokes, remote] });
  await page.evaluate(
    (id) =>
      window.dispatchEvent(
        new CustomEvent('ossia-drawing-changed', { detail: { documentId: id, revision: 2 } }),
      ),
    firstId,
  );
  release();
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(2, { timeout: 1_500 });
});

test('drawings persist, follow zoom, erase locally, and appear in export preview', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const pen = page.getByRole('button', { name: '画笔', exact: true });
  await expect(pen).toBeVisible();
  await page.keyboard.press('d');
  await expect(pen).toHaveAttribute('aria-pressed', 'true');
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.25);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.8, box.y + box.height * 0.25, { steps: 20 });
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
  const beforeZoom = await host.locator('.score-freehand-layer polyline').getAttribute('points');
  await page.locator('.score-view').hover();
  await page.getByRole('button', { name: '放大' }).click();
  await expect(page.getByLabel('谱面缩放')).toHaveText('110%');
  expect((await host.boundingBox())!.width).toBeGreaterThan(box.width);
  await expect(host.locator('.score-freehand-layer polyline')).toHaveAttribute(
    'points',
    beforeZoom!,
  );
  await page.reload();
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
  await page.keyboard.press('d');
  const restored = (await host.boundingBox())!;
  await page.keyboard.down('Control');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByRole('button', { name: '橡皮擦', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByRole('group', { name: '橡皮擦大小' })).toBeVisible();
  await page.mouse.move(restored.x + restored.width * 0.5, restored.y + restored.height * 0.2);
  await expect(page.locator('.score-eraser-indicator')).toBeVisible();
  await page.mouse.down();
  await page.mouse.move(restored.x + restored.width * 0.5, restored.y + restored.height * 0.3, {
    steps: 10,
  });
  await page.mouse.up();
  await page.keyboard.up('Control');
  await expect(pen).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('button', { name: '橡皮擦', exact: true })).toHaveAttribute(
    'aria-pressed',
    'false',
  );
  await expect(page.getByRole('group', { name: '画笔大小' })).toBeVisible();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(2);
  await page.getByRole('button', { name: '导出', exact: true }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog.getByLabel('包含绘画')).toBeChecked();
  await expect(dialog.locator('.export-preview-page .score-freehand-layer polyline')).toHaveCount(
    2,
  );
  await dialog.getByLabel('包含绘画').uncheck();
  await expect(dialog.locator('.export-preview-page .score-freehand-layer')).toHaveCount(0);
});

test('the same page drawing is visible in historical versions', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.3);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.3);
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
});

test('drawing shortcuts leave editable fields and open dialogs alone', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const pen = page.getByRole('button', { name: '画笔', exact: true });
  await expect(pen).toBeVisible();
  await page.getByRole('button', { name: '导出', exact: true }).click();
  const file = page.locator('#export-file-name');
  await expect(file).toHaveValue('合成钢琴练习');
  await file.fill('');
  await file.pressSequentially('dr');
  await expect(file).toHaveValue('dr');
  await expect(page.locator('.score-feature-toolbar button[aria-label="画笔"]')).toHaveAttribute(
    'aria-pressed',
    'false',
  );
});

test('escape exits pen and eraser modes', async ({ context, page }) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const pen = page.getByRole('button', { name: '画笔', exact: true });
  const eraser = page.getByRole('button', { name: '橡皮擦', exact: true });
  await expect(pen).toBeVisible();
  await page.keyboard.press('d');
  await expect(pen).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Escape');
  await expect(pen).toHaveAttribute('aria-pressed', 'false');
  await expect(page.getByRole('main')).toBeFocused();
  await page.keyboard.press('r');
  await expect(eraser).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Escape');
  await expect(eraser).toHaveAttribute('aria-pressed', 'false');
});

test('pen and eraser sizes stay independent across tool changes and reloads', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await page.getByRole('button', { name: '画笔大小 3' }).click();
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await page.getByRole('button', { name: '橡皮擦大小 4' }).click();
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await expect(page.getByRole('button', { name: '画笔大小 3' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await page.reload();
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await expect(page.getByRole('button', { name: '橡皮擦大小 4' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
});

test('pen colors persist and survive drawing, erasing and reloading', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const colors = page.getByRole('group', { name: '画笔颜色' });
  await expect(colors.getByRole('button')).toHaveCount(5);
  await expect(colors.getByRole('button', { name: '黑色' })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  for (const name of ['蓝色', '绿色', '黄色', '红色', '黑色']) {
    await colors.getByRole('button', { name }).click();
    await expect(colors.getByRole('button', { name })).toHaveAttribute('aria-pressed', 'true');
  }
  await colors.getByRole('button', { name: '蓝色' }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.3);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.8, box.y + box.height * 0.3, { steps: 20 });
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes[0]?.color).toBe('blue');
  await expect(host.locator('.score-freehand-layer polyline')).toHaveAttribute('stroke', '#007aff');
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.25);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.35, { steps: 10 });
  await page.mouse.up();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(2);
  expect(runtime.drawings.get(firstId)!.strokes.every((stroke) => stroke.color === 'blue')).toBe(
    true,
  );
  await page.reload();
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await expect(
    page.getByRole('group', { name: '画笔颜色' }).getByRole('button', { name: '蓝色' }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(2);
});

test('drawing cursor remains stable and eraser shows its selected circular range', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const pen = page.getByRole('button', { name: '画笔', exact: true });
  const eraser = page.getByRole('button', { name: '橡皮擦', exact: true });
  await pen.click();
  const selectedColor = page
    .getByRole('group', { name: '画笔颜色' })
    .getByRole('button', { name: '黑色' });
  await expect(selectedColor).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await selectedColor.hover();
  await expect(selectedColor).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await expect(pen).not.toHaveAttribute('data-tooltip');
  await expect(eraser).not.toHaveAttribute('data-tooltip');
  await eraser.hover();
  expect(await eraser.evaluate((element) => getComputedStyle(element, '::after').content)).toBe(
    'none',
  );
  const host = page.locator('.page-host').first();
  const scroll = page.locator('.score-scroll');
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.3, box.y + box.height * 0.3);
  await expect(page.locator('.score-pen-indicator')).toBeVisible();
  expect(await host.evaluate((element) => getComputedStyle(element).cursor)).toContain(
    'pencil.png',
  );
  await page.mouse.down();
  await expect(page.locator('.score-pen-indicator')).toBeVisible();
  expect(await scroll.evaluate((element) => getComputedStyle(element).cursor)).toContain(
    'pencil.png',
  );
  await page.mouse.up();
  await eraser.click();
  await page.getByRole('button', { name: '橡皮擦大小 3' }).click();
  await page.mouse.move(box.x + box.width * 0.4, box.y + box.height * 0.3);
  const indicator = page.locator('.score-eraser-indicator');
  await expect(indicator).toBeVisible();
  await expect(indicator).toHaveCSS('width', '32px');
  expect(await host.evaluate((element) => getComputedStyle(element).cursor)).toContain(
    'eraser.png',
  );
  await page.mouse.down();
  expect(await scroll.evaluate((element) => getComputedStyle(element).cursor)).toContain(
    'eraser.png',
  );
  await expect(indicator).toBeVisible();
  await page.mouse.up();
});

test('touch erasing displays the eraser range until the finger is lifted', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '橡皮擦', exact: true }).click();
  const box = (await page.locator('.page-host').first().boundingBox())!;
  const cdp = await context.newCDPSession(page);
  const point = { x: box.x + box.width * 0.4, y: box.y + box.height * 0.3, id: 1 };
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point] });
  await expect(page.locator('.score-eraser-indicator')).toBeVisible();
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect(page.locator('.score-eraser-indicator')).toHaveCount(0);
});

test('a second finger finishes the stroke and takes over pinch zoom', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const bounds = (await host.boundingBox())!;
  const x = bounds.x + bounds.width / 2;
  const y = bounds.y + bounds.height * 0.3;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x: x - 30, y, id: 1 }],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ x: x - 45, y, id: 1 }],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [
      { x: x - 45, y, id: 1 },
      { x: x + 45, y, id: 2 },
    ],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [
      { x: x - 90, y, id: 1 },
      { x: x + 90, y, id: 2 },
    ],
  });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  await expect
    .poll(async () => (await host.boundingBox())!.width)
    .toBeGreaterThan(bounds.width * 1.5);
});

test('Pencil draws while concurrent touch does not move the page', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const bounds = (await host.boundingBox())!;
  const x = bounds.x + bounds.width * 0.4;
  const y = bounds.y + bounds.height * 0.3;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x,
    y,
    button: 'left',
    buttons: 1,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x: x + 80, y: y + 50, id: 5 }],
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ x: x + 150, y: y + 90, id: 5 }],
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseMoved',
    x: x + 40,
    y,
    button: 'left',
    buttons: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: x + 40,
    y,
    button: 'left',
    buttons: 0,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  const after = (await host.boundingBox())!;
  expect(after.x).toBeCloseTo(bounds.x, 0);
  expect(after.y).toBeCloseTo(bounds.y, 0);
  expect(after.width).toBeCloseTo(bounds.width, 0);
});

test('Pencil takes priority when a hand touches the page first', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const bounds = (await host.boundingBox())!;
  const x = bounds.x + bounds.width * 0.4;
  const y = bounds.y + bounds.height * 0.3;
  const cdp = await context.newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchStart',
    touchPoints: [{ x: x + 80, y: y + 50, id: 5 }],
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x,
    y,
    button: 'left',
    buttons: 1,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchTouchEvent', {
    type: 'touchMove',
    touchPoints: [{ x: x + 150, y: y + 90, id: 5 }],
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseMoved',
    x: x + 40,
    y,
    button: 'left',
    buttons: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: x + 40,
    y,
    button: 'left',
    buttons: 0,
    clickCount: 1,
    pointerType: 'pen',
  });
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  expect(runtime.drawings.get(firstId)?.strokes[0]?.points.length).toBeGreaterThan(1);
  const after = (await host.boundingBox())!;
  expect(after.x).toBeCloseTo(bounds.x, 0);
  expect(after.y).toBeCloseTo(bounds.y, 0);
});

test('concurrent drawing edits are merged across pages after a revision conflict', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  const other = await context.newPage();
  await Promise.all([
    page.goto(`/scores?documentId=${firstId}`),
    other.goto(`/scores?documentId=${firstId}`),
  ]);
  await Promise.all(
    [page, other].map(async (tab) => {
      await tab.getByRole('button', { name: '画笔', exact: true }).click();
      await expect(
        tab.locator('.page-host').first().locator('.score-freehand-layer'),
      ).toBeVisible();
    }),
  );
  const draw = async (tab: typeof page, yFraction: number) => {
    const box = (await tab.locator('.page-host').first().boundingBox())!;
    await tab.mouse.move(box.x + box.width * 0.2, box.y + box.height * yFraction);
    await tab.mouse.down();
    await tab.mouse.move(box.x + box.width * 0.5, box.y + box.height * yFraction);
    await tab.mouse.up();
  };
  runtime.pauseEvents();
  await draw(page, 0.25);
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
  const conflict = other.waitForResponse(
    (response) =>
      response.url().endsWith(`/documents/${firstId}/drawing`) && response.status() === 409,
  );
  await draw(other, 0.35);
  await conflict;
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(2);
  runtime.resumeEvents();
  await expect(
    page.locator('.page-host').first().locator('.score-freehand-layer polyline'),
  ).toHaveCount(2);
  await expect(
    other.locator('.page-host').first().locator('.score-freehand-layer polyline'),
  ).toHaveCount(2);
});

test('failed saves retain the local stroke and can be retried', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  let fail = true;
  await context.route(`**/api/v1/documents/${firstId}/drawing`, async (route) => {
    if (route.request().method() === 'PUT' && fail) {
      fail = false;
      return route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: { code: 'storage-error', message: 'unavailable' } }),
      });
    }
    return route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.3);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.3);
  await page.mouse.up();
  const retry = page.getByRole('button', { name: '绘画保存失败' });
  await expect(retry).toBeVisible();
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
  await page.getByRole('button', { name: /选择乐谱/ }).click();
  await page.getByRole('menuitemradio', { name: '另一份乐谱' }).click();
  await expect(page).toHaveURL(new RegExp(`documentId=${firstId}`));
  await expect(page.getByText('请先保存绘画后再切换乐谱。')).toBeVisible();
  await retry.click();
  await expect(retry).toBeHidden();
  await expect.poll(() => runtime.drawings.get(firstId)?.strokes.length).toBe(1);
});

test('drawing locks wheel pan and zoom only while the pointer is down', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  const host = page.locator('.page-host').first();
  const box = (await host.boundingBox())!;
  await page.mouse.move(box.x + box.width * 0.4, box.y + box.height * 0.3);
  await page.mouse.down();
  await page.mouse.wheel(0, 120);
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -120);
  await page.keyboard.up('Control');
  const during = (await host.boundingBox())!;
  expect(during.x).toBeCloseTo(box.x, 0);
  expect(during.y).toBeCloseTo(box.y, 0);
  expect(during.width).toBeCloseTo(box.width, 0);
  await page.mouse.up();
  await page.mouse.wheel(0, 120);
  await expect.poll(async () => (await host.boundingBox())!.y).toBeLessThan(box.y - 50);
});

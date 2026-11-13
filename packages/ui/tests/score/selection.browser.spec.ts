import { expect, test } from '@playwright/test';

import { firstId, mockScoreRuntime } from './score-runtime.ts';

test('selection paints before saving and keeps rapid toggles while a save is pending', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let saves = 0;
  await page.route('**/selection', async (route) => {
    saves++;
    if (saves === 1) await held;
    await route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  const upper = { x: box.width * 0.4, y: (box.height * 25) / 140 };
  const lower = { x: box.width * 0.4, y: (box.height * 55) / 140 };
  try {
    await overlay.click({ position: upper });
    await expect(page.locator('.selection-box')).toHaveCount(1, { timeout: 750 });
    expect(runtime.selections.get(firstId)).toBeUndefined();
    await overlay.click({ position: lower });
    await expect(page.locator('.selection-box')).toHaveCount(2);
    await overlay.click({ position: upper });
    await expect(page.locator('.selection-box')).toHaveCount(1);
    await expect(page.locator('.selection-box')).toHaveAttribute('y', '50');
    expect(runtime.points).toHaveLength(0);
  } finally {
    release();
  }
  await expect
    .poll(() => runtime.selections.get(firstId)?.locators)
    .toEqual([{ staffIndex: 1, measureStart: 1, measureEnd: 1, voice: null }]);
  await expect(page.locator('.selection-box')).toHaveAttribute('y', '50');
});

test('rejected optimistic selection rolls back to the shared context', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  runtime.stale();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 25) / 140 } });
  await expect(page.locator('.toast.is-error')).toBeVisible();
  await expect(page.locator('.selection-box')).toHaveCount(0);
  expect(runtime.selections.get(firstId)).toBeUndefined();
});

test('an accepted local selection does not refetch its own context', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const requests: string[] = [];
  page.on('request', (request) => {
    requests.push(new URL(request.url()).pathname);
  });
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 25) / 140 } });
  await expect.poll(() => runtime.selections.get(firstId)?.locators.length).toBe(1);
  await page.waitForTimeout(100);
  expect(requests.filter((path) => path.endsWith(`/documents/${firstId}/context`))).toEqual([]);
});

test('clicks outside local rectangles retain server hit testing', async ({ context, page }) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 35) / 140 } });
  await expect(page.locator('.selection-box')).toHaveAttribute('y', '20');
  expect(runtime.points).toHaveLength(1);
});

test('selection events update another tab without refreshing status or version history', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  const other = await context.newPage();
  await other.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(other.locator('.score-image')).toHaveCount(2);
  const requests: string[] = [];
  page.on('request', (request) => {
    requests.push(new URL(request.url()).pathname);
  });
  const updated = page.waitForRequest(`**/documents/${firstId}/context`);
  const overlay = other.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 25) / 140 } });
  await updated;
  await expect(page.locator('.selection-box')).toHaveCount(1);
  expect(requests.filter((path) => path.endsWith('/status') || path.endsWith('/versions'))).toEqual(
    [],
  );
});

test('an older context refresh cannot overwrite an acknowledged selection', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let captured!: () => void;
  const started = new Promise<void>((resolve) => {
    captured = resolve;
  });
  await page.route(
    `**/documents/${firstId}/context`,
    async (route) => {
      const snapshot = JSON.stringify(runtime.contextFor(firstId));
      captured();
      await held;
      await route.fulfill({ contentType: 'application/json', body: snapshot });
    },
    { times: 1 },
  );
  runtime.emit('document-changed');
  await started;
  try {
    const overlay = page.locator('.overlay').first();
    const box = (await overlay.boundingBox())!;
    await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 25) / 140 } });
    await expect.poll(() => runtime.selections.get(firstId)?.locators.length).toBe(1);
  } finally {
    release();
  }
  await expect(page.locator('.selection-box')).toHaveCount(1);
  await expect(page.getByText('已连接')).toBeVisible();
  await expect(page.locator('.selection-box')).toHaveCount(1);
});

test('a source change cancels queued selection saves from the old render', async ({
  context,
  page,
}) => {
  const runtime = await mockScoreRuntime(context);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let saves = 0;
  await page.route('**/selection', async (route) => {
    saves++;
    if (saves === 1) await held;
    await route.fallback();
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  try {
    await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 25) / 140 } });
    await overlay.click({ position: { x: box.width * 0.4, y: (box.height * 55) / 140 } });
    await expect(page.locator('.selection-box')).toHaveCount(2);
    runtime.change();
    runtime.stale();
    await expect(page.locator('.selection-box')).toHaveCount(0);
  } finally {
    release();
  }
  await expect.poll(() => saves).toBe(1);
  await expect(page.locator('.selection-box')).toHaveCount(0);
});

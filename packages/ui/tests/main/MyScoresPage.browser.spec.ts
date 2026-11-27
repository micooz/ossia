import { expect, test, type Page } from '@playwright/test';

import type {
  ManagementScorePreview,
  ManagementScoreSummary,
} from '../../src/generated/protocol.ts';
import { mockManagement } from './management.ts';

const ALPHA = '00000000-0000-4000-8000-000000000001';
const BETA = '00000000-0000-4000-8000-000000000002';
const GAMMA = '00000000-0000-4000-8000-000000000003';
const READY_PREVIEW: ManagementScorePreview = {
  state: 'ready',
  dataUrl:
    'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiPjwvc3ZnPg==',
};

function score(
  documentId: string,
  title: string,
  createdAt: string,
  updatedAt: string,
): ManagementScoreSummary {
  return { documentId, title, createdAt, updatedAt, archivedAt: null };
}

const scores = [
  score(ALPHA, 'Nocturne A', '2026-11-01T08:00:00.000Z', '2026-11-03T08:00:00.000Z'),
  score(BETA, 'Nocturne B', '2026-11-02T08:00:00.000Z', '2026-11-05T08:00:00.000Z'),
  score(GAMMA, 'Nocturne C', '2026-11-04T08:00:00.000Z', '2026-11-01T08:00:00.000Z'),
];

async function openScores(page: Page) {
  await page.goto('/main.html');
  await page.getByRole('button', { name: '我的乐谱', exact: true }).click();
  await expect(page.getByRole('heading', { name: '我的乐谱' })).toBeVisible();
}

async function setPreview(page: Page, documentId: string, outcome: ManagementScorePreview) {
  const outcomeJson = JSON.stringify(outcome);
  await page.evaluate(
    ({ documentId, outcomeJson }) => {
      (
        window as Window & {
          setScorePreview?: (id: string, next: unknown) => void;
        }
      ).setScorePreview?.(documentId, JSON.parse(outcomeJson));
    },
    { documentId, outcomeJson },
  );
}

async function setScores(page: Page, next: ManagementScoreSummary[]) {
  const scoresJson = JSON.stringify(next);
  await page.evaluate((scoresJson) => {
    (
      window as Window & {
        setScores?: (scores: ManagementScoreSummary[]) => void;
      }
    ).setScores?.(JSON.parse(scoresJson));
  }, scoresJson);
}

test('my scores does not show an empty state before scores have loaded', async ({ page }) => {
  await mockManagement(page, { initialRuntime: true, scores: [scores[0]!] });
  await page.goto('/main.html');
  await page.evaluate(() => {
    const internals = (
      window as unknown as Window & {
        __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => Promise<unknown> };
        emptyStateFlashes?: number;
        loadingFlashes?: number;
      }
    ).__TAURI_INTERNALS__;
    const invoke = internals.invoke;
    internals.invoke = async (...args) => {
      if (args[0] === 'management_scores') {
        await new Promise((resolve) => setTimeout(resolve, 200));
      }
      return invoke(...args);
    };
    const ownWindow = window as Window & { emptyStateFlashes?: number; loadingFlashes?: number };
    ownWindow.emptyStateFlashes = 0;
    ownWindow.loadingFlashes = 0;
    new MutationObserver(() => {
      if (document.querySelector('.scores-state h2')?.textContent === '还没有乐谱') {
        ownWindow.emptyStateFlashes! += 1;
      }
      if (document.querySelector('.scores-state[role="status"]')) {
        ownWindow.loadingFlashes! += 1;
      }
    }).observe(document.body, { childList: true, subtree: true });
  });

  for (let attempt = 0; attempt < 2; attempt++) {
    await page.getByRole('button', { name: '我的乐谱', exact: true }).click();
    await expect(page.locator('.score-card-title')).toHaveText(['Nocturne A']);
    await page.getByRole('button', { name: 'Agent', exact: true }).click();
  }
  expect(
    await page.evaluate(
      () => (window as Window & { emptyStateFlashes?: number }).emptyStateFlashes,
    ),
  ).toBe(0);
  expect(
    await page.evaluate(() => (window as Window & { loadingFlashes?: number }).loadingFlashes),
  ).toBe(0);
});

test('my scores shows loading after a slow request', async ({ page }) => {
  await mockManagement(page, { initialRuntime: true, scores: [scores[0]!] });
  await page.goto('/main.html');
  await page.evaluate(() => {
    const target = window as unknown as Window & {
      __TAURI_INTERNALS__: { invoke: (command: string, args?: unknown) => Promise<unknown> };
      releaseScores?: () => void;
    };
    const invoke = target.__TAURI_INTERNALS__.invoke;
    target.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (command === 'management_scores') {
        await new Promise<void>((resolve) => {
          target.releaseScores = resolve;
        });
      }
      return invoke(command, args);
    };
  });
  await page.getByRole('button', { name: '我的乐谱', exact: true }).click();
  await expect(page.getByRole('status')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: '还没有乐谱' })).toHaveCount(0);
  await expect(page.getByRole('status')).toContainText('正在读取乐谱');
  await page.evaluate(() => (window as Window & { releaseScores?: () => void }).releaseScores?.());
  await expect(page.locator('.score-card-title')).toHaveText(['Nocturne A']);
});

test('my scores supports layouts, search, sort persistence and browser opening', async ({
  page,
}) => {
  await mockManagement(page, { initialRuntime: true, scores });
  await openScores(page);

  await expect(page.getByRole('button', { name: '我的乐谱', exact: true })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.locator('.breadcrumb')).toHaveAttribute('aria-label', '资料库 / 我的乐谱');
  await expect(page.locator('.score-card-title')).toHaveText([
    'Nocturne B',
    'Nocturne A',
    'Nocturne C',
  ]);
  await expect(page.locator('.score-card-time').first()).toContainText('修改于');
  await expect(page.locator('.score-collection')).toHaveClass(/is-grid/);
  await expect(
    page.locator('.score-view-controls').getByRole('button', { name: '已归档' }),
  ).toBeVisible();
  const modifiedSort = page.getByRole('button', { name: '修改时间：从新到旧' });
  await expect(modifiedSort.locator('svg.sort-arrows')).toHaveAttribute('data-direction', 'desc');
  await expect(modifiedSort.locator('path[data-arrow="asc"]')).toHaveCount(1);
  await expect(modifiedSort.locator('path[data-arrow="desc"]')).toHaveCount(1);

  await page.getByRole('button', { name: '列表' }).click();
  await expect(page.locator('.score-collection')).toHaveClass(/is-list/);
  await page.getByRole('button', { name: '按创建时间排序' }).click();
  await expect(page.locator('.score-card-title')).toHaveText([
    'Nocturne C',
    'Nocturne B',
    'Nocturne A',
  ]);
  await expect(page.locator('.score-card-time').first()).toContainText('创建于');
  await page.getByRole('button', { name: '创建时间：从新到旧' }).click();
  await expect(page.locator('.score-card-title')).toHaveText([
    'Nocturne A',
    'Nocturne B',
    'Nocturne C',
  ]);

  const search = page.getByPlaceholder('搜索乐谱');
  await search.fill('nocturne b');
  await expect(page.locator('.score-card-title')).toHaveText(['Nocturne B']);
  await page.getByRole('button', { name: '清除搜索' }).click();
  await expect(page.locator('.score-card-title')).toHaveCount(3);

  await page
    .locator('.score-card')
    .filter({ hasText: 'Nocturne A' })
    .locator('button.score-card-title')
    .click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as Window & { openedScores?: string[] }).openedScores?.includes(
            '00000000-0000-4000-8000-000000000001',
          ) ?? false,
      ),
    )
    .toBe(true);

  await page.reload();
  await expect(page.locator('.score-collection')).toHaveClass(/is-list/);
  await expect(page.getByRole('button', { name: '创建时间：从旧到新' })).toBeVisible();
  await expect(page.getByPlaceholder('搜索乐谱')).toHaveValue('');

  for (const width of [860, 1600]) {
    await page.setViewportSize({ width, height: 900 });
    expect(
      await page
        .locator('.page-content')
        .evaluate((element) => element.scrollWidth <= element.clientWidth),
    ).toBe(true);
  }
});

test('my scores discovers starter scores added after the page loads', async ({ page }) => {
  await page.clock.install({ time: new Date('2026-11-20T08:00:00Z') });
  await page.clock.pauseAt(new Date('2026-11-20T08:00:00Z'));
  await mockManagement(page, { initialRuntime: true, scores: [] });
  await openScores(page);
  await expect(page.getByRole('heading', { name: '还没有乐谱' })).toBeVisible();

  await setScores(page, [scores[0]!]);
  await page.clock.runFor(2001);
  await expect(page.locator('.score-card-title')).toHaveText(['Nocturne A']);
  await expect(page.locator('.score-preview-image')).toBeVisible();
});

test('visible score previews transition from pending to ready', async ({ page }) => {
  await mockManagement(page, {
    initialRuntime: true,
    scores: [scores[0]!],
    previewStates: { [ALPHA]: { state: 'pending', jobId: 'job-preview' } },
  });
  await openScores(page);
  await expect(page.getByText('正在生成预览…')).toBeVisible();
  await setPreview(page, ALPHA, READY_PREVIEW);
  await expect(page.locator('.score-preview-image')).toBeVisible();
  await expect(page.locator('.score-preview-image')).toHaveAttribute(
    'src',
    /^data:image\/svg\+xml/,
  );
});

test('score previews remain readable in dark mode in both layouts', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await mockManagement(page, { initialRuntime: true, scores: [scores[0]!] });
  await openScores(page);
  const preview = page
    .locator('.score-preview')
    .filter({ has: page.locator('.score-preview-image') });
  await expect(preview).toBeVisible();

  for (const layout of ['平铺', '列表']) {
    await page.getByRole('button', { name: layout, exact: true }).click();
    const background = await preview.evaluate(
      (element) => getComputedStyle(element).backgroundColor,
    );
    const channels = background.match(/\d+/g)?.slice(0, 3).map(Number) ?? [];
    expect(channels).toHaveLength(3);
    expect(Math.min(...channels)).toBeGreaterThan(180);
  }
});

test('failed previews expose a retry action', async ({ page }) => {
  await mockManagement(page, {
    initialRuntime: true,
    scores: [scores[0]!],
    previewStates: {
      [ALPHA]: {
        state: 'failed',
        error: { code: 'compile-failed', message: 'syntax error', detail: null },
      },
    },
  });
  await openScores(page);
  const retry = page.locator('.score-preview-message.is-action');
  await expect(retry).toContainText('预览生成失败');
  await setPreview(page, ALPHA, READY_PREVIEW);
  await retry.click();
  await expect(page.locator('.score-preview-image')).toBeVisible();
});

test('score previews reuse the window cache across page navigation', async ({ page }) => {
  await mockManagement(page, { initialRuntime: true, scores: [scores[0]!] });
  await openScores(page);
  await expect(page.locator('.score-preview-image')).toBeVisible();
  const before = await page.evaluate(
    () => (window as Window & { scorePreviewCalls?: string[] }).scorePreviewCalls?.length ?? 0,
  );

  await page.getByRole('button', { name: 'Agent', exact: true }).click();
  await page.getByRole('button', { name: '我的乐谱', exact: true }).click();
  await expect(page.locator('.score-preview-image')).toBeVisible();
  const after = await page.evaluate(
    () => (window as Window & { scorePreviewCalls?: string[] }).scorePreviewCalls?.length ?? 0,
  );
  expect(after).toBe(before);
});

test('archived scores restore or require a second click before permanent deletion', async ({
  page,
}) => {
  await page.clock.install();
  await mockManagement(page, { initialRuntime: true, scores: scores.slice(0, 2) });
  await openScores(page);

  const archive = page.getByRole('button', { name: '归档 Nocturne A' });
  await expect(archive).toHaveAttribute('data-tooltip', '归档');
  await archive.click();
  await expect(page.locator('.score-card-title')).toHaveText(['Nocturne B']);
  await page.getByRole('button', { name: '已归档' }).click();
  await expect(page.locator('.score-card-title')).toHaveText(['Nocturne A']);
  const restore = page.getByRole('button', { name: '恢复 Nocturne A' });
  await expect(restore).toHaveAttribute('data-tooltip', '恢复');
  await restore.click();
  await expect(page.getByRole('heading', { name: '没有已归档的乐谱' })).toBeVisible();

  await page.getByRole('button', { name: '已归档' }).click();
  await page.getByRole('button', { name: '归档 Nocturne A' }).click();
  await page.getByRole('button', { name: '已归档' }).click();
  const remove = page.getByRole('button', { name: '永久删除' });
  await expect(remove).toHaveAttribute('data-tooltip', '永久删除');
  await remove.click();
  const confirm = page.getByRole('button', { name: '确认删除' });
  await expect(confirm).toBeVisible();
  const color = await confirm.evaluate((element) => getComputedStyle(element).color);
  await confirm.hover();
  expect(await confirm.evaluate((element) => getComputedStyle(element).color)).toBe(color);
  await expect
    .poll(() => confirm.evaluate((element) => getComputedStyle(element, '::after').opacity))
    .toBe('1');
  await page.clock.fastForward(3_000);
  await expect(page.getByRole('button', { name: '永久删除' })).toBeVisible();
  await page.getByRole('button', { name: '永久删除' }).click();
  await page.getByRole('button', { name: '确认删除' }).click();
  await expect(page.getByRole('heading', { name: '没有已归档的乐谱' })).toBeVisible();
});

test('my scores asks for the local service when the backend is stopped', async ({ page }) => {
  await mockManagement(page);
  await page.goto('/main.html');
  await page.getByRole('button', { name: '我的乐谱', exact: true }).click();
  await expect(page.getByRole('heading', { name: '需要本地服务' })).toBeVisible();
  await expect(page.getByRole('button', { name: '启动服务' })).toBeEnabled();
});

test('opens my scores in the browser', async ({ page }) => {
  await mockManagement(page, { initialRuntime: true });
  await openScores(page);
  await page.getByRole('button', { name: '在浏览器中打开我的乐谱' }).click();
  await expect
    .poll(() => page.evaluate(() => (window as Window & { openedLinks?: string[] }).openedLinks))
    .toEqual(['http://127.0.0.1:3212']);
});

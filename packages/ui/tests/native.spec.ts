import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { test, expect, type Locator, type Page } from '@playwright/test';

import type {
  AgentOpenOutcome,
  DocumentContext,
  DocumentPage,
  Job,
  JobOutcome,
  RenderGeometry,
} from '../src/generated/protocol.ts';

const exec = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const binary = process.env.OSSIA_TEST_EXECUTABLE ?? path.join(root, 'target/debug/ossia-desktop');
const engine =
  process.env.OSSIA_TEST_LILYPOND_ROOT ?? path.join(root, 'vendor/lilypond/2.26.0-darwin-arm64');
let temporary: string;
let home: string;
const fixtures = path.join(root, 'tests/fixtures/inputs');
async function cli<T>(command: string, args?: unknown): Promise<T> {
  const { stdout } = await exec(
    binary,
    [
      '--home',
      home,
      '--port',
      '0',
      '--engine-root',
      engine,
      '--static-dir',
      path.join(root, 'packages/ui/dist'),
      command,
      ...(args === undefined ? [] : [JSON.stringify(args)]),
    ],
    { timeout: 45_000, maxBuffer: 8 * 1024 * 1024 },
  ).catch((error: { stdout?: string; stderr?: string; message: string }) => {
    throw new Error([error.message, error.stdout, error.stderr].filter(Boolean).join('\n'));
  });
  return JSON.parse(stdout);
}
async function waitJob<T>(initial: Job): Promise<T> {
  let job = initial;
  for (let count = 0; count < 600 && ['queued', 'running'].includes(job.state); count++) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    job = (await cli<JobOutcome>('job', { jobId: job.jobId, action: 'get' })).job;
  }
  expect(job.state, JSON.stringify(job.error)).toBe('done');
  return job.result as T;
}
async function clickScore(overlay: Locator, x: number, y: number) {
  await overlay.evaluate(
    (element, point) => {
      const bounds = element.getBoundingClientRect();
      element.dispatchEvent(
        new MouseEvent('click', {
          bubbles: true,
          clientX: bounds.left + bounds.width * point.x,
          clientY: bounds.top + bounds.height * point.y,
        }),
      );
    },
    { x, y },
  );
}
async function scoreZoom(page: Page) {
  const label = page.getByLabel('谱面缩放');
  return Number.parseInt((await label.textContent()) ?? '100', 10);
}
async function zoomWithMouse(page: Page, direction: 1 | -1) {
  const scroll = page.locator('.score-scroll');
  await scroll.hover();
  await page.keyboard.down('Control');
  await page.mouse.wheel(0, -100 * direction);
  await page.waitForTimeout(15);
  await page.keyboard.up('Control');
}
async function ensureZoom(page: Page, condition: 'base' | 'zoomed') {
  if (condition === 'base') {
    while ((await scoreZoom(page)) > 100) await zoomWithMouse(page, -1);
  } else {
    while ((await scoreZoom(page)) < 140) await zoomWithMouse(page, 1);
  }
}
async function expectDocumentHint(page: Page, text: string, timeout = 5_000) {
  await page.getByLabel('版本', { exact: true }).click();
  await expect(page.getByText(text, { exact: false })).toBeVisible({ timeout });
  await page.keyboard.press('Escape');
}
test.beforeAll(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'ossia-p05-'));
  home = path.join(temporary, 'library with spaces');
  await cli('status');
});
test.afterAll(async () => {
  if (home) await cli('stop').catch(() => {});
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
test('real browser, LilyPond, Agent editing, history, restore and exports', async ({
  page,
  context,
}) => {
  await waitJob(
    await cli<Job>('import', {
      title: '合成钢琴练习',
      path: path.join(fixtures, 'polyphonic-pages.ly'),
      format: 'ly',
      confirmation: null,
      author: 'test',
    }),
  );
  const docs = await cli<DocumentPage>('list');
  const documentId = docs.items[0]!.documentId;
  const opened = await cli<AgentOpenOutcome>('open', { documentId });
  await page.goto(opened.url);
  await expect(page.getByText('已连接')).toBeVisible();
  const origin = new URL(opened.url).origin;
  const initial: DocumentContext = await (
    await page.request.get(`${origin}/api/v1/documents/${documentId}/context`)
  ).json();
  const fullContext = async (): Promise<DocumentContext> =>
    (await page.request.get(`${origin}/api/v1/documents/${documentId}/context`)).json();
  const geometryResponse = await page.request.get(
    `${origin}/api/v1/renders/${initial.render!.renderKey}/geometry`,
  );
  expect(geometryResponse.ok()).toBeTruthy();
  const geometry: RenderGeometry = await geometryResponse.json();
  expect(geometry.pages.length).toBeGreaterThan(1);
  expect(geometry.pages[1]!.regions.some((r) => r.locator.measureStart >= 3)).toBeTruthy();
  const lower = geometry.pages[0]!.regions.find((r) => r.locator.staffIndex === 1)!;
  expect(lower).toBeTruthy();
  await expect(page.locator('.score-image')).toHaveCount(geometry.pages.length);
  await expect(page).toHaveTitle('合成钢琴练习');
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    for (const zoom of ['base', 'zoomed'] as const) {
      await ensureZoom(page, zoom);
      const viewBox = geometry.pages[0]!.viewBox!;
      const overlay = page.locator('.overlay').first();
      const x = ((lower.bounds.x0 + lower.bounds.x1) / 2 - viewBox.x0) / (viewBox.x1 - viewBox.x0);
      const y = ((lower.bounds.y0 + lower.bounds.y1) / 2 - viewBox.y0) / (viewBox.y1 - viewBox.y0);
      await clickScore(overlay, x, y);
      await expect(page.locator('.selection-box').first()).toHaveAttribute(
        'y',
        String(lower.bounds.y0),
      );
      await clickScore(overlay, x, y);
      await expect(page.locator('.selection-box')).toHaveCount(0);
    }
    await ensureZoom(page, 'base');
  }
  await page.setViewportSize({ width: 1440, height: 1000 });
  const firstViewBox = geometry.pages[0]!.viewBox!;
  await clickScore(
    page.locator('.overlay').first(),
    ((lower.bounds.x0 + lower.bounds.x1) / 2 - firstViewBox.x0) /
      (firstViewBox.x1 - firstViewBox.x0),
    ((lower.bounds.y0 + lower.bounds.y1) / 2 - firstViewBox.y0) /
      (firstViewBox.y1 - firstViewBox.y0),
  );
  await expect(page.locator('.selection-box')).toHaveCount(1);
  const other = await context.newPage();
  const second = await cli<AgentOpenOutcome>('open', { documentId });
  await other.goto(second.url);
  await expect(other.locator('.selection-box')).toHaveCount(1);
  await page.reload();
  await expect(page.locator('.selection-box')).toHaveCount(1);
  const edit = await cli<{ editId: string }>('edit', {
    action: 'begin',
    documentId,
    baseVersionId: initial.document.headVersionId,
    expectedSourceHash: initial.document.sourceHash,
    editId: null,
    author: 'P05 test agent',
    adoptWorkspace: false,
  });
  const sourcePath = (await fullContext()).document.sourcePath;
  const source = await fs.readFile(sourcePath, 'utf8');
  const changed = source.replace('c1', 'd1') + '\n% P05 synthetic edit\n';
  await fs.writeFile(sourcePath, changed);
  const hash = createHash('sha256').update(changed).digest('hex');
  await expect(page.locator('.selection-box')).toHaveCount(0);
  await expect
    .poll(async () => (await fullContext()).render?.manifest.sourceHash, { timeout: 30_000 })
    .toBe(hash);
  await expectDocumentHint(page, '工作区有未提交修改', 30_000);
  await waitJob(
    await cli<Job>('check', {
      editId: edit.editId,
      expectedSourceHash: hash,
    }),
  );
  await cli('commit', {
    editId: edit.editId,
    expectedSourceHash: hash,
    summary: 'P05 合成编辑',
  });
  await page.getByLabel('版本', { exact: true }).click();
  await expect(page.getByRole('menuitemradio')).toHaveCount(3);
  await page.keyboard.press('Escape');
  await page.getByLabel('对比版本', { exact: true }).click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(page.locator('.compare .score-image')).toHaveCount(geometry.pages.length * 2);
  await page.getByLabel('对比版本', { exact: true }).click();
  await page.getByRole('menuitem', { name: '退出对比' }).click();
  await expect(page.locator('.compare')).toHaveCount(0);
  await fs.writeFile(sourcePath, changed + '\n\\notAnOssiaCommand\n');
  await expectDocumentHint(page, '上次成功结果：', 30_000);
  await expect(page.locator('.diagnostics')).toHaveCount(0);
  await expect(page.locator('.score-image')).toHaveCount(geometry.pages.length);
  const dirty = await fullContext();
  const discarded = await page.request.post(`${origin}/api/v1/documents/${documentId}/edits`, {
    data: {
      action: 'discard',
      documentId,
      baseVersionId: dirty.document.headVersionId,
      expectedSourceHash: dirty.document.sourceHash,
      editId: null,
      author: 'test',
      adoptWorkspace: false,
    },
  });
  expect(discarded.ok()).toBeTruthy();
  await expectDocumentHint(page, '当前工作区乐谱与最新版本一致', 30_000);
  await page.getByLabel('版本', { exact: true }).click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await page.getByRole('button', { name: '恢复至版本 1', exact: true }).click();
  await page.getByRole('button', { name: '确认恢复' }).click();
  await page.getByLabel('版本', { exact: true }).click();
  await expect(page.getByRole('menuitemradio')).toHaveCount(4);
  await page.keyboard.press('Escape');
  await page.route('**/api/v1/outputs', async (route) => {
    const args = route.request().postDataJSON();
    const response = await route.fetch({
      postData: JSON.stringify({
        ...args,
        outputPath: path.join(
          temporary,
          args.page ? `export-${args.page}.${args.format}` : `export.${args.format}`,
        ),
      }),
    });
    await route.fulfill({ response });
  });
  await page.locator('.topbar-right').getByRole('button', { name: '导出', exact: true }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog).toBeVisible();
  for (const [format, label] of [
    ['pdf', 'PDF'],
    ['png', 'PNG'],
    ['svg', 'SVG'],
    ['midi', 'MIDI'],
  ] as const) {
    await dialog.getByRole('button', { name: new RegExp(label) }).click();
    if (format === 'pdf') {
      const review = path.join(root, '.impeccable/review');
      await fs.mkdir(review, { recursive: true });
      await page.screenshot({
        path: path.join(review, 'export-dialog-desktop.png'),
        fullPage: true,
      });
      await page.setViewportSize({ width: 390, height: 1000 });
      await page.screenshot({
        path: path.join(review, 'export-dialog-mobile.png'),
        fullPage: true,
      });
      await page.setViewportSize({ width: 1440, height: 1000 });
    }
    const downloadPromise = page.waitForEvent('download', { timeout: 60_000 });
    await dialog.getByRole('button', { name: '导出', exact: true }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toMatch(
      new RegExp(
        `\\.${format === 'midi' ? 'mid' : ['png', 'svg'].includes(format) ? 'zip' : format}$`,
      ),
    );
    const downloaded = await download.path();
    expect(downloaded).not.toBeNull();
    expect((await fs.stat(downloaded!)).size).toBeGreaterThan(0);
    if (format === 'png' || format === 'svg') {
      for (const page of geometry.pages) {
        expect(
          (await fs.stat(path.join(temporary, `export-${page.page}.${format}`))).size,
        ).toBeGreaterThan(0);
      }
    } else {
      expect((await fs.stat(path.join(temporary, `export.${format}`))).size).toBeGreaterThan(0);
    }
    await expect(dialog).toBeVisible();
  }
  await dialog.getByRole('button', { name: '关闭' }).click();
  await expect(dialog).not.toBeVisible();
  const review = path.join(root, '.impeccable/review');
  await fs.mkdir(review, { recursive: true });
  for (const [width, name] of [
    [1440, 'desktop'],
    [390, 'mobile'],
  ] as const) {
    await page.setViewportSize({ width, height: 1000 });
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.screenshot({ path: path.join(review, `${name}.png`), fullPage: true });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
    ).toBeTruthy();
  }
  await fs.writeFile(
    path.join(review, 'p05-native.json'),
    JSON.stringify(
      {
        pages: geometry.pages.length,
        regions: geometry.pages.map((p) => p.regions.length),
        widths: [390, 1440],
        zooms: [100, 150],
        versions: 3,
        formats: ['pdf', 'midi', 'png', 'svg'],
        status: 'passed',
      },
      null,
      2,
    ),
  );
});

import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { test, expect } from '@playwright/test';

import type {
  AgentOpenOutcome,
  DocumentPage,
  Job,
  JobOutcome,
  RenderGeometry,
  ViewSnapshot,
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
  );
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
  await expect(page.getByText('本地服务已连接')).toBeVisible();
  await expect(page.locator('.score-image')).toBeVisible();
  const token = await page.evaluate(
    (viewId) => JSON.parse(sessionStorage.getItem(`ossia:view:${viewId}`)!).token as string,
    opened.viewId,
  );
  const origin = new URL(opened.url).origin;
  const initial: ViewSnapshot = await (
    await page.request.get(`${origin}/api/v1/views/${opened.viewId}`, {
      headers: { Authorization: `Bearer ${token}` },
    })
  ).json();
  const fullView = async (): Promise<ViewSnapshot> =>
    (
      await page.request.get(`${origin}/api/v1/views/${opened.viewId}`, {
        headers: { Authorization: `Bearer ${token}` },
      })
    ).json();
  const geometryResponse = await page.request.get(
    `${origin}/api/v1/renders/${initial.render!.renderKey}/geometry`,
    { headers: { Authorization: `Bearer ${token}` } },
  );
  expect(geometryResponse.ok()).toBeTruthy();
  const geometry: RenderGeometry = await geometryResponse.json();
  expect(geometry.pages.length).toBeGreaterThan(1);
  expect(geometry.pages[1]!.regions.some((r) => r.locator.measureStart >= 3)).toBeTruthy();
  const lower = geometry.pages[0]!.regions.find((r) => r.locator.staffIndex === 1)!;
  expect(lower).toBeTruthy();
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    for (const zoom of ['100', '150']) {
      await page.getByLabel('谱面缩放').selectOption(zoom);
      const viewBox = geometry.pages[0]!.viewBox!;
      const screen = await page.locator('.overlay').boundingBox();
      await page.locator('.overlay').click({
        position: {
          x:
            (((lower.bounds.x0 + lower.bounds.x1) / 2 - viewBox.x0) / (viewBox.x1 - viewBox.x0)) *
            screen!.width,
          y:
            (((lower.bounds.y0 + lower.bounds.y1) / 2 - viewBox.y0) / (viewBox.y1 - viewBox.y0)) *
            screen!.height,
        },
      });
      await expect(page.locator('.selection li')).toContainText('谱表 2');
      await expect(page.locator('.selection-box').first()).toHaveAttribute(
        'y',
        String(lower.bounds.y0),
      );
      await page.getByRole('button', { name: '清空选区' }).click();
      await expect(page.locator('.selection li')).toHaveCount(0);
    }
  }
  await page.getByLabel('谱面缩放').selectOption('100');
  await page.getByText('使用键盘选择小节').click();
  const measure = page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true });
  await measure.focus();
  await page.keyboard.press('Space');
  await expect(page.locator('.selection li')).toContainText('谱表 1');
  const other = await context.newPage();
  const second = await cli<AgentOpenOutcome>('open', {});
  await other.goto(second.url);
  await expect(other.getByRole('heading', { name: '选择或导入一份乐谱' })).toBeVisible();
  await page.reload();
  await expect(page.locator('.selection li')).toContainText('谱表 1');
  const edit = await cli<{ editId: string }>('edit', {
    action: 'begin',
    documentId,
    viewId: opened.viewId,
    baseVersionId: initial.document!.headVersionId,
    expectedSourceHash: initial.document!.sourceHash,
    editId: null,
    author: 'P05 test agent',
    adoptWorkspace: false,
  });
  const sourcePath = (await fullView()).document!.sourcePath;
  const source = await fs.readFile(sourcePath, 'utf8');
  const changed = source.replace('c1', 'd1') + '\n% P05 synthetic edit\n';
  await fs.writeFile(sourcePath, changed);
  const hash = createHash('sha256').update(changed).digest('hex');
  await expect(page.locator('.document-status')).toContainText('工作区有未提交修改');
  await expect(page.locator('.selection li')).toHaveCount(0);
  await expect
    .poll(async () => (await fullView()).render?.manifest.sourceHash, { timeout: 30_000 })
    .toBe(hash);
  await expect(page.locator('.document-status')).toContainText(`谱面 ${hash.slice(0, 12)}`);
  await waitJob(
    await cli<Job>('check', {
      viewId: opened.viewId,
      editId: edit.editId,
      expectedSourceHash: hash,
    }),
  );
  await cli('commit', {
    viewId: opened.viewId,
    editId: edit.editId,
    expectedSourceHash: hash,
    summary: 'P05 合成编辑',
  });
  await expect(page.locator('.version-list li')).toHaveCount(2);
  await page.getByRole('button', { name: '版本对比', exact: true }).click();
  await expect(page.locator('.compare .score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '工作区谱面' }).click();
  await fs.writeFile(sourcePath, changed + '\n\\notAnOssiaCommand\n');
  await expect(page.getByText('上次成功结果：', { exact: false })).toBeVisible();
  await expect(page.locator('.diagnostics')).toContainText('notAnOssiaCommand', {
    timeout: 30_000,
  });
  await page.getByLabel('谱面页码').selectOption('2');
  await expect(page.locator('.score-image')).toHaveAttribute('alt', /第 2 页/);
  await page.getByRole('button', { name: '恢复至版本 1', exact: true }).click();
  await page.getByRole('button', { name: '确认恢复' }).click();
  await expect(page.getByRole('alert').first()).toBeVisible();
  await page.getByRole('button', { name: '丢弃工作区修改' }).click();
  await page.getByRole('button', { name: '确认丢弃' }).click();
  await expect(page.locator('.document-status')).toContainText('工作区与已提交版本一致');
  await page.getByRole('button', { name: '恢复至版本 1', exact: true }).click();
  await page.getByRole('button', { name: '确认恢复' }).click();
  await expect(page.locator('.version-list li')).toHaveCount(3);
  await page.route('**/api/v1/outputs', async (route) => {
    const args = route.request().postDataJSON();
    const response = await route.fetch({
      postData: JSON.stringify({
        ...args,
        outputPath: path.join(temporary, `export.${args.format}`),
      }),
    });
    await route.fulfill({ response });
  });
  for (const format of ['ly', 'pdf', 'midi']) {
    await page.getByLabel('导出格式').selectOption(format);
    await page.getByRole('button', { name: '导出', exact: true }).click();
    await expect(page.locator('.export .path')).toContainText(`export.${format}`, {
      timeout: 30_000,
    });
    expect((await fs.stat(path.join(temporary, `export.${format}`))).size).toBeGreaterThan(0);
  }
  await page.getByLabel('谱面页码').selectOption('1');
  if (await page.getByRole('button', { name: '关闭提示' }).count())
    await page.getByRole('button', { name: '关闭提示' }).click();
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
        formats: ['ly', 'pdf', 'midi'],
        status: 'passed',
      },
      null,
      2,
    ),
  );
});

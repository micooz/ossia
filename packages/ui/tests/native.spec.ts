import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { test, expect, type Locator, type Page } from '@playwright/test';
import { unzipSync } from 'fflate';

import type {
  AgentDocumentContext,
  DocumentContext,
  DocumentPage,
  Job,
  JobOutcome,
  RenderGeometry,
  ScorePlayback,
  GeometryPage,
  GeometryRegion,
  SelectionTarget,
} from '../src/generated/protocol.ts';

const exec = promisify(execFile);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const binary = process.env.OSSIA_TEST_EXECUTABLE ?? path.join(root, 'target/debug/ossia-desktop');
const engine =
  process.env.OSSIA_TEST_LILYPOND_ROOT ?? path.join(root, 'vendor/lilypond/2.26.0-darwin-arm64');
let temporary: string;
let home: string;

function measureWhitespace(page: GeometryPage, region: GeometryRegion) {
  for (const dy of [0.2, 0.5, 0.8]) {
    for (const dx of [0.5, 0.3, 0.7, 0.1, 0.9]) {
      const x = region.bounds.x0 + (region.bounds.x1 - region.bounds.x0) * dx;
      const y = region.bounds.y0 + (region.bounds.y1 - region.bounds.y0) * dy;
      if (
        !page.elements.some(
          ({ bounds }) => x >= bounds.x0 && x <= bounds.x1 && y >= bounds.y0 && y <= bounds.y1,
        )
      )
        return { x, y };
    }
  }
  throw new Error('measure fixture has no selectable whitespace');
}
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
function inkCount(
  geometry: RenderGeometry,
  targets: GeometryPage['elements'][number]['linkedTargets'],
) {
  const keys = new Set(targets.map((target) => JSON.stringify(target)));
  return geometry.pages.reduce(
    (sum, page) =>
      sum +
      new Set(
        page.elements
          .filter((e) => keys.has(JSON.stringify(e.region.target)))
          .flatMap((e) => e.graphicIds),
      ).size,
    0,
  );
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
async function expectDocumentHint(page: Page, text: string, timeout = 5_000) {
  await page.locator('.score-version-trigger').click();
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

test('Jianpu starts at a selected measure and follows the score onto the next page', async ({
  page,
}) => {
  const source = await fs.readFile(
    path.join(root, 'tests/fixtures/playback/piano-prelude.ly'),
    'utf8',
  );
  const candidate = path.join(temporary, 'prelude-jianpu.ly');
  await fs.writeFile(candidate, `% ossia:notation=jianpu\n${source}`);
  const title = 'BWV 846 简谱播放测试';
  await waitJob(await cli<Job>('import', { title, path: candidate }));
  const score = (await cli<DocumentPage>('list')).items.find((doc) => doc.title === title)!;
  await page.goto(score.url!);
  const api = `${new URL(score.url!).origin}/api/v1`;
  const context: DocumentContext = await (
    await page.request.get(`${api}/documents/${score.documentId}/context`)
  ).json();
  const key = context.render!.renderKey;
  const geometry: RenderGeometry = await (
    await page.request.get(`${api}/renders/${key}/geometry`)
  ).json();
  const playback: ScorePlayback = await (
    await page.request.get(`${api}/renders/${key}/playback`)
  ).json();
  const boundary = playback.systems.findIndex((span) => span.page === 2);
  expect(boundary).toBeGreaterThan(0);
  const previous = playback.systems[boundary - 1]!;
  const first = geometry.pages[0]!;
  const region = first.regions.find(
    (candidate) =>
      candidate.locator.staffIndex === 0 &&
      candidate.locator.measureStart === previous.measureStart,
  )!;
  const { x, y } = measureWhitespace(first, region);
  const box = first.viewBox!;
  const firstPage = page.locator('.page-host[data-page="1"]');
  const secondPage = page.locator('.page-host[data-page="2"]');
  const play = page.getByRole('button', { name: '播放', exact: true });
  await expect(play).toBeEnabled();
  await expect(firstPage.locator('.overlay.interactive')).toBeVisible();
  await clickScore(
    firstPage.locator('.overlay'),
    (x - box.x0) / (box.x1 - box.x0),
    (y - box.y0) / (box.y1 - box.y0),
  );
  const line = firstPage.locator('.score-playhead');
  await expect(line).toHaveCount(1);
  expect(Number(await line.getAttribute('x1'))).toBeGreaterThanOrEqual(previous.x0 - 0.1);
  await play.click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
  await expect(secondPage.locator('.score-playhead')).toHaveCount(1, { timeout: 15_000 });
});

test('real score drawing persists and exports with the score', async ({ page }) => {
  await waitJob(
    await cli<Job>('import', {
      title: '自由绘画回归',
      path: path.join(fixtures, 'inline-staff.ly'),
      format: 'ly',
      confirmation: null,
      author: 'test',
    }),
  );
  const docs = await cli<DocumentPage>('list');
  const score = docs.items.find((doc) => doc.title === '自由绘画回归')!;
  let drawingReads = 0;
  page.on('request', (request) => {
    if (
      request.method() === 'GET' &&
      request.url().endsWith(`/documents/${score.documentId}/drawing`)
    )
      drawingReads++;
  });
  await page.goto(score.url!);
  const origin = new URL(score.url!).origin;
  const host = page.locator('.page-host').first();
  await expect(host.locator('.score-image')).toBeVisible();
  await expect(page.getByRole('button', { name: '画笔', exact: true })).toBeVisible();
  expect(drawingReads).toBe(1);
  await page.waitForTimeout(2_300);
  expect(drawingReads).toBe(1);
  await page.getByRole('button', { name: '画笔', exact: true }).click();
  await page.getByRole('group', { name: '画笔颜色' }).getByRole('button', { name: '红色' }).click();
  const bounds = (await host.boundingBox())!;
  await page.mouse.move(bounds.x + bounds.width * 0.25, bounds.y + bounds.height * 0.3);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * 0.7, bounds.y + bounds.height * 0.3, {
    steps: 10,
  });
  await page.mouse.up();
  await expect
    .poll(
      async () =>
        (
          await (
            await page.request.get(`${origin}/api/v1/documents/${score.documentId}/drawing`)
          ).json()
        ).strokes.length,
    )
    .toBe(1);
  await page.reload();
  await expect(host.locator('.score-freehand-layer polyline')).toHaveCount(1);
  await expect(host.locator('.score-freehand-layer polyline')).toHaveAttribute('stroke', '#ff3b30');
  await page.getByRole('button', { name: '导出', exact: true }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog.locator('.export-preview-page .score-freehand-layer polyline')).toHaveCount(
    1,
  );
  for (const format of ['SVG', 'PNG', 'PDF']) {
    await dialog.getByRole('button', { name: new RegExp(format) }).click();
    if (format === 'PNG') await dialog.getByLabel('透明背景').check();
    const downloadPromise = page.waitForEvent('download');
    await dialog.getByRole('button', { name: '导出', exact: true }).click();
    const download = await downloadPromise;
    const bytes = await fs.readFile((await download.path())!);
    if (format === 'SVG') expect(bytes.toString()).toContain('stroke="#ff3b30"');
    if (format === 'PNG') {
      expect(bytes.subarray(0, 8)).toEqual(Buffer.from('89504e470d0a1a0a', 'hex'));
      const pixel = await page.evaluate(async (encoded) => {
        const binary = Uint8Array.from(atob(encoded), (character) => character.charCodeAt(0));
        const image = await createImageBitmap(new Blob([binary], { type: 'image/png' }));
        const canvas = document.createElement('canvas');
        canvas.width = image.width;
        canvas.height = image.height;
        const context = canvas.getContext('2d')!;
        context.drawImage(image, 0, 0);
        return [
          ...context.getImageData(
            Math.floor(image.width * 0.4),
            Math.floor(image.height * 0.3),
            1,
            1,
          ).data,
        ];
      }, bytes.toString('base64'));
      expect(pixel[0]).toBeGreaterThan(200);
      expect(pixel[1]).toBeLessThan(100);
      expect(pixel[2]).toBeLessThan(100);
    }
    if (format === 'PDF')
      expect(bytes.toString('latin1')).toContain('1 0.23137254901960785 0.18823529411764706 RG');
  }
});
test('a supplied score auditions notes without creating click selections', async ({ page }) => {
  const source = process.env.OSSIA_TEST_SELECTION_SOURCE;
  test.skip(!source, 'requires OSSIA_TEST_SELECTION_SOURCE');
  await waitJob(
    await cli<Job>('import', {
      title: '实际谱面点选回归',
      path: source,
      format: 'ly',
      confirmation: null,
      author: 'test',
    }),
  );
  const docs = await cli<DocumentPage>('list');
  const score = docs.items.find((doc) => doc.title === '实际谱面点选回归')!;
  const documentId = score.documentId;
  await page.goto(score.url!);
  await expect(page.locator('.overlay.interactive').first()).toBeVisible();
  const origin = new URL(score.url!).origin;
  const context: DocumentContext = await (
    await page.request.get(`${origin}/api/v1/documents/${documentId}/context`)
  ).json();
  const geometry: RenderGeometry = await (
    await page.request.get(`${origin}/api/v1/renders/${context.render!.renderKey}/geometry`)
  ).json();
  await expect(page.locator('.score-image')).toHaveCount(geometry.pages.length);
  const first = geometry.pages[0]!;
  const box = first.viewBox!;
  const region = first.regions.find((r) => r.locator.staffIndex === 0)!;
  const note = first.elements
    .filter(
      (e) =>
        e.region.elementKind === 'NoteHead' &&
        e.region.staffIndex === 0 &&
        e.bounds.x0 > region.bounds.x0 &&
        e.bounds.x1 < region.bounds.x1 &&
        e.bounds.y0 > region.bounds.y0 - 5 &&
        e.bounds.y1 < region.bounds.y1 + 5,
    )
    .sort((a, b) => a.bounds.x0 - b.bounds.x0)[0]!;
  expect(note).toBeTruthy();
  const click = (x: number, y: number) =>
    clickScore(
      page.locator('.overlay').first(),
      (x - box.x0) / (box.x1 - box.x0),
      (y - box.y0) / (box.y1 - box.y0),
    );
  const x = (note.bounds.x0 + note.bounds.x1) / 2,
    y = (note.bounds.y0 + note.bounds.y1) / 2;
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeEnabled({
    timeout: 30_000,
  });
  await click(x, y);
  await expect(page.locator('.playback-active').first()).toBeVisible();
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await expect(page.locator('.selection-box')).toHaveCount(0);
  await page
    .locator('.page-host')
    .first()
    .screenshot({ path: test.info().outputPath('supplied-score-note.png') });
  const whitespace = measureWhitespace(first, region);
  await click(whitespace.x, whitespace.y);
  await expect(page.locator('.selection-box')).toHaveCount(0);
  await page
    .locator('.page-host')
    .first()
    .screenshot({ path: test.info().outputPath('supplied-score-no-selection.png') });
});
test('real browser, LilyPond, Agent editing, history, restore and exports', async ({ page }) => {
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
  const score = docs.items.find((doc) => doc.title === '合成钢琴练习')!;
  const documentId = score.documentId;
  await page.goto(score.url!);
  await expect(page.locator('.overlay.interactive').first()).toBeVisible();
  const origin = new URL(score.url!).origin;
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
  await expect(page.locator('.score-image')).toHaveCount(geometry.pages.length);
  await expect(page).toHaveTitle('合成钢琴练习');
  await expect(page.locator('.topbar-title')).toHaveText('合成钢琴练习');
  const note = geometry.pages[0]!.elements.find((e) => e.region.elementKind === 'NoteHead')!;
  expect(note).toBeTruthy();
  const initialBox = geometry.pages[0]!.viewBox!;
  const noteX =
    ((note.bounds.x0 + note.bounds.x1) / 2 - initialBox.x0) / (initialBox.x1 - initialBox.x0);
  const noteY =
    ((note.bounds.y0 + note.bounds.y1) / 2 - initialBox.y0) / (initialBox.y1 - initialBox.y0);
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeEnabled({
    timeout: 30_000,
  });
  await clickScore(page.locator('.overlay').first(), noteX, noteY);
  await expect(page.locator('.playback-active').first()).toBeVisible();
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await expect(page.locator('.selection-box')).toHaveCount(0);
  await page
    .locator('.page-host')
    .first()
    .screenshot({ path: test.info().outputPath('note-audition.png') });
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
  await expectDocumentHint(page, 'P05 test agent 正在编辑乐谱', 30_000);
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
  await page.locator('.score-version-trigger').click();
  await expect(page.getByRole('menuitemradio')).toHaveCount(2);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: '对比版本' })).toHaveCount(0);
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
  await expect
    .poll(async () => (await fullContext()).document.dirty, { timeout: 30_000 })
    .toBe(false);
  await expect(page.locator('.score-version-trigger')).not.toContainText('当前草稿');
  await page.locator('.score-version-trigger').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await page.getByRole('button', { name: '恢复至此版本', exact: true }).click();
  await page.getByRole('button', { name: '确认恢复' }).click();
  await page.locator('.score-version-trigger').click();
  await expect(page.getByRole('menuitemradio')).toHaveCount(3);
  await page.keyboard.press('Escape');
  await page.route('**/api/v1/outputs', async (route) => {
    const args = route.request().postDataJSON();
    const page = args.pages?.[0];
    const range = args.audioRange ? `-m${args.audioRange.start}-${args.audioRange.end}` : '';
    const batch = ['png', 'svg'].includes(args.format) && (args.pages?.length ?? 0) > 1;
    const response = await route.fetch({
      postData: JSON.stringify({
        ...args,
        outputPath: batch
          ? null
          : path.join(
              temporary,
              page ? `export-${page}${range}.${args.format}` : `export${range}.${args.format}`,
            ),
      }),
    });
    await route.fulfill({ response });
  });
  await page.locator('.topbar-right').getByRole('button', { name: '导出', exact: true }).click();
  const dialog = page.locator('.export-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel('文件名')).not.toHaveValue('');
  for (const [format, label] of [
    ['pdf', 'PDF'],
    ['png', 'PNG'],
    ['svg', 'SVG'],
    ['midi', 'MIDI'],
    ['wav', 'WAV'],
  ] as const) {
    await dialog.getByRole('button', { name: new RegExp(label) }).click();
    if (format === 'pdf') {
      const password = dialog.getByLabel('PDF 密码');
      await password.pressSequentially('native-secret');
      await expect(password).toHaveValue('native-secret');
    }
    if (format === 'png') {
      await dialog.getByLabel('透明背景').check();
      const review = path.join(root, '.impeccable/review');
      await page.screenshot({
        path: path.join(review, 'export-dialog-png.png'),
        fullPage: true,
      });
    }
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
    if (format === 'pdf') {
      expect((await fs.readFile(downloaded!)).includes('/Encrypt')).toBeTruthy();
    }
    if (format === 'wav') {
      const bytes = await fs.readFile(downloaded!);
      expect(bytes.subarray(0, 4).toString()).toBe('RIFF');
      expect(bytes.subarray(8, 12).toString()).toBe('WAVE');
    }
    if (format === 'png' || format === 'svg') {
      const archive = unzipSync(new Uint8Array(await fs.readFile(downloaded!)));
      const files = Object.keys(archive)
        .filter((name) => name.endsWith(`.${format}`))
        .sort();
      expect(files).toHaveLength(geometry.pages.length);
      if (format === 'png') {
        const png = Buffer.from(archive[files[0]!]!);
        expect(png[25]).toBe(3);
        expect(png.includes('tRNS')).toBeTruthy();
      }
      if (format === 'svg') {
        const svg = Buffer.from(archive[files[0]!]!).toString('utf8');
        expect(svg).not.toContain('textedit://');
      }
    } else {
      expect((await fs.stat(path.join(temporary, `export.${format}`))).size).toBeGreaterThan(0);
    }
    await expect(dialog).toBeVisible();
  }
  for (const [format, extension] of [
    ['midi', 'mid'],
    ['wav', 'wav'],
  ] as const) {
    await dialog.getByRole('button', { name: new RegExp(format.toUpperCase()) }).click();
    const range = dialog.getByLabel('导出指定小节');
    await expect(range).toBeEnabled();
    if (!(await range.isChecked())) await range.check();
    await dialog.getByLabel('结束小节').fill('1');
    const rangeDownload = page.waitForEvent('download');
    await dialog.getByRole('button', { name: '导出', exact: true }).click();
    const download = await rangeDownload;
    expect(download.suggestedFilename()).toMatch(new RegExp(`\\.${extension}$`));
    const downloaded = await download.path();
    expect(downloaded).not.toBeNull();
    const bytes = await fs.readFile(downloaded!);
    expect(bytes.length).toBeGreaterThan(0);
    if (format === 'midi') expect(bytes.subarray(0, 4).toString()).toBe('MThd');
    if (format === 'wav') {
      expect(bytes.subarray(0, 4).toString()).toBe('RIFF');
      expect(bytes.subarray(8, 12).toString()).toBe('WAVE');
    }
    const outputExtension = format === 'midi' ? 'midi' : 'wav';
    expect(
      (await fs.stat(path.join(temporary, `export-m1-1.${outputExtension}`))).size,
    ).toBeGreaterThan(0);
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

test('native SVG colors chord parts, ornaments and a complete cross-page tie chain', async ({
  page,
}) => {
  await waitJob(
    await cli<Job>('import', {
      title: '原生染色回归',
      path: path.join(fixtures, 'selection-ink.ly'),
      format: 'ly',
      confirmation: null,
      author: 'test',
    }),
  );
  const docs = await cli<DocumentPage>('list');
  const score = docs.items.find((doc) => doc.title === '原生染色回归')!;
  const documentId = score.documentId;
  await page.goto(score.url!);
  await expect(page.locator('.overlay.interactive').first()).toBeVisible();
  const origin = new URL(score.url!).origin;
  const context: DocumentContext = await (
    await page.request.get(`${origin}/api/v1/documents/${documentId}/context`)
  ).json();
  const geometry: RenderGeometry = await (
    await page.request.get(`${origin}/api/v1/renders/${context.render!.renderKey}/geometry`)
  ).json();
  await expect(page.locator('.score-image')).toHaveCount(2);
  const notes = geometry.pages
    .flatMap((p) => p.elements.map((e) => ({ page: p, element: e })))
    .filter(({ element: e }) => e.region.elementKind === 'NoteHead');
  const tied = notes.filter(({ element: e }) => e.region.label.startsWith("NoteHead: g'"));
  expect(tied).toHaveLength(3);
  const select = async (targets: SelectionTarget[]) => {
    const response = await page.request.put(`${origin}/api/v1/documents/${documentId}/selection`, {
      data: {
        documentId,
        sourceHash: context.document.sourceHash,
        targets,
        annotations: [],
      },
    });
    expect(response.ok()).toBeTruthy();
  };
  for (const layout of ['纵向', '横向']) {
    await page.locator('.score-scroll').hover();
    await page.getByRole('button', { name: layout }).click();
    for (const zoom of ['100%', '200%']) {
      await page.locator('.score-scroll').hover();
      await page.getByLabel('谱面缩放').click();
      await page.getByRole('menuitemradio', { name: zoom, exact: true }).click();
      await select(tied[0]!.element.linkedTargets);
      await expect(page.locator('.element-selection')).toHaveCount(
        inkCount(geometry, tied[0]!.element.linkedTargets),
      );
      await expect(page.locator('.selection-box')).toHaveCount(0);
      const agent = await cli<AgentDocumentContext>('context', { documentId });
      expect(agent.regions.filter((r) => r.elementKind === 'NoteHead')).toHaveLength(3);
      expect(agent.regions.filter((r) => r.elementKind === 'Tie')).toHaveLength(2);
      await select([]);
      await expect(page.locator('.element-selection')).toHaveCount(0);
    }
  }
  await page.locator('.score-scroll').hover();
  await page.getByRole('button', { name: '纵向' }).click();
  await page.getByLabel('谱面缩放').click();
  await page.getByRole('menuitemradio', { name: '100%', exact: true }).click();
  const chord = notes.find(({ element: e }) => e.region.label === "NoteHead: c'")!;
  await select(chord.element.linkedTargets);
  await expect(page.locator('.element-selection')).toHaveCount(
    inkCount(geometry, chord.element.linkedTargets),
  );
  await page
    .locator('.page-host')
    .first()
    .screenshot({ path: test.info().outputPath('native-chord-color.png') });
  await select(tied[0]!.element.linkedTargets);
  await page
    .locator('.page-host')
    .first()
    .screenshot({ path: test.info().outputPath('native-tie-color.png') });
  await select([]);
  const title = geometry.pages[0]!.elements.find((e) => e.region.elementKind === 'header-title')!;
  await select(title.linkedTargets);
  const coloredTitle = page.locator('[data-ossia-graphic].element-selection text').first();
  await expect(coloredTitle).not.toHaveCSS('fill', 'rgb(0, 0, 0)');
  await expect(page.locator('.selection-box')).toHaveCount(0);
});

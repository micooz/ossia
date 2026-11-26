import { expect, test } from '@playwright/test';

import type {
  GeometryElement,
  PlaybackSystemSpan,
  ScorePlayback,
  SelectionTarget,
} from '../../src/generated/protocol.ts';
import { scheduleWindow } from '../../src/score/utils/playbackSchedule.ts';
import { firstId, secondId, hash, mockScoreRuntime, newer } from './score-runtime.ts';

const target = (elementId: string): SelectionTarget => ({ kind: 'element', elementId });

function scoreElement(id: string, x: number, kind: string, pitch = 0): GeometryElement {
  return {
    systemIndex: 0,
    bounds: {
      x0: x,
      x1: x + 3,
      y0: kind === 'NoteHead' ? 24 : 20,
      y1: kind === 'NoteHead' ? 26 : 22,
    },
    priority: 0,
    hitBounds: [],
    graphicIds: [`${id}-graphic`],
    linkedTargets: [target(id)],
    region: {
      target: target(id),
      label: `${id}: ${pitch}`,
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
      staffIndex: 0,
      voice: null,
      reason: null,
    },
  };
}

function note(id: string, x: number, pitch: number): GeometryElement {
  return scoreElement(id, x, 'NoteHead', pitch);
}

function positionedNote(id: string, x: number, y: number, pitch: number): GeometryElement {
  return {
    ...note(id, x, pitch),
    bounds: { x0: x, x1: x + 3, y0: y, y1: y + 2 },
  };
}

function rest(id: string, x: number): GeometryElement {
  return scoreElement(id, x, 'Rest');
}

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140">
<g data-ossia-graphic="first-graphic"><path d="M30 24h3v3h-3z" fill="black"/></g>
<g data-ossia-graphic="second-graphic"><path d="M60 24h3v3h-3z" fill="black"/></g>
</svg>`;

function system(page: number, startSeconds: number, endSeconds: number): PlaybackSystemSpan {
  return {
    page,
    systemIndex: 0,
    measureStart: page,
    measureEnd: page,
    startSeconds,
    endSeconds,
    x0: page === 1 ? 10 : 20,
    x1: page === 1 ? 80 : 90,
    y0: 18,
    y1: 32,
  };
}

function measureSpan(
  measure: number,
  startSeconds: number,
  endSeconds: number,
  x0: number,
  x1: number,
): PlaybackSystemSpan {
  return {
    page: 1,
    systemIndex: 0,
    measureStart: measure,
    measureEnd: measure,
    startSeconds,
    endSeconds,
    x0,
    x1,
    y0: 18,
    y1: 32,
  };
}

function playback(systems: PlaybackSystemSpan[]): ScorePlayback {
  return {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['first'],
        pitches: [60],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['second'],
        pitches: [67],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    systems,
  };
}

test('the scheduler includes every note sharing an onset', () => {
  const notes = [
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 36,
      startSeconds: 0,
      durationSeconds: 1,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 48,
      startSeconds: 0,
      durationSeconds: 1,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 64,
      startSeconds: 0,
      durationSeconds: 1,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 67,
      startSeconds: 1,
      durationSeconds: 1,
    },
  ];
  const window = scheduleWindow(notes, 0, -0.01, 0.2);
  expect(window.notes.map((note) => note.pitch)).toEqual([36, 48, 64]);
  expect(window.nextIndex).toBe(3);
});

async function pageWithPlayback(
  context: Parameters<typeof mockScoreRuntime>[0],
  timeline: ScorePlayback,
) {
  const runtime = await mockScoreRuntime(context, {
    elements: [note('first', 30, 60), note('second', 60, 67)],
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  return { runtime, page };
}

test('a refreshed render waits for the current playback to finish', async ({ context }) => {
  const timeline = playback([system(1, 0, 3)]);
  timeline.durationSeconds = 3;
  const { runtime, page } = await pageWithPlayback(context, timeline);
  const play = page.getByRole('button', { name: '播放', exact: true });
  const pause = page.getByRole('button', { name: '暂停', exact: true });
  const line = page.locator('.score-playhead');
  await play.click();
  await expect(line).toHaveAttribute('x1', '31.5');

  const refreshed = page.waitForResponse(`**/documents/${firstId}/context`);
  runtime.rerender(newer);
  expect((await (await refreshed).json()).render.renderKey).toBe(newer);
  await expect(line).toHaveAttribute('x1', '61.5', { timeout: 2_000 });
  await pause.click();
  await expect(play).toBeVisible();
  await expect(line).toHaveAttribute('x1', '61.5');
  expect(runtime.pageRequests.every((path) => !path.includes(newer))).toBe(true);

  await play.click();
  await expect(pause).toBeVisible();
  await expect(line).toHaveAttribute('x1', '61.5');
  await expect.poll(() => runtime.pageRequests.some((path) => path.includes(newer))).toBe(true);
  await expect(line).toHaveCount(0);
  await expect(play).toBeEnabled();
  expect(runtime.playbackRequests).toContain(`/renders/${newer}/playback`);
});

test('score settings remember each part and apply its sample during playback', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.parts = [
    { id: 'part', name: 'Melody', instrument: 'violin' },
    { id: 'part-b', name: 'Accompaniment', instrument: 'piano' },
  ];
  timeline.notes = [
    { partId: 'part', instrument: 'violin', pitch: 60, startSeconds: 0, durationSeconds: 0.8 },
    { partId: 'part-b', instrument: 'piano', pitch: 67, startSeconds: 1, durationSeconds: 0.8 },
  ];
  const { runtime, page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '乐谱设置' }).click();
  const dialog = page.getByRole('dialog', { name: '乐谱设置' });
  await expect(dialog.getByText('播放音色')).toBeVisible();
  await expect(dialog.getByRole('columnheader', { name: '声部' })).toBeVisible();
  await expect(dialog.getByRole('columnheader', { name: '音色' })).toBeVisible();
  const melody = dialog.getByLabel('Melody');
  await expect(melody.locator('option').first()).toHaveText('自动（小提琴）');
  await melody.selectOption('guitar');
  await expect.poll(() => runtime.scoreSettings.get(firstId)?.instruments.part).toBe('guitar');
  await dialog.getByRole('button', { name: '关闭' }).click();
  const sampleRequests: string[] = [];
  page.on('response', (response) => {
    if (response.ok() && response.url().includes('/audio/')) sampleRequests.push(response.url());
  });
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect.poll(() => sampleRequests.some((url) => url.includes('/audio/guitar/'))).toBe(true);
  await page.getByRole('button', { name: '乐谱设置' }).click();
  await dialog.getByLabel('Melody').selectOption('ukulele');
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeVisible();
  await expect.poll(() => runtime.scoreSettings.get(firstId)?.instruments.part).toBe('ukulele');
  await page.reload();
  await page.getByRole('button', { name: '乐谱设置' }).click();
  await expect(page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('Melody')).toHaveValue(
    'ukulele',
  );
  await page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('Melody').selectOption('auto');
  await expect.poll(() => runtime.scoreSettings.get(firstId)?.instruments.part).toBeUndefined();
});

test('score settings synchronize across tabs and remain isolated between scores', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  const { runtime, page } = await pageWithPlayback(context, timeline);
  const secondPage = await context.newPage();
  await secondPage.goto(`/scores?documentId=${firstId}`);
  await expect(secondPage.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '乐谱设置' }).click();
  await secondPage.getByRole('button', { name: '乐谱设置' }).click();
  const firstSelect = page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴');
  const secondSelect = secondPage.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴');
  await firstSelect.selectOption('violin');
  await expect(secondSelect).toHaveValue('violin', { timeout: 6000 });
  await secondSelect.selectOption('guitar');
  await expect(firstSelect).toHaveValue('guitar', { timeout: 6000 });
  await secondPage.goto(`/scores?documentId=${secondId}`);
  await secondPage.getByRole('button', { name: '乐谱设置' }).click();
  await expect(secondPage.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴')).toHaveValue(
    'auto',
  );
  expect(runtime.scoreSettings.get(firstId)?.instruments.part).toBe('guitar');
  expect(runtime.scoreSettings.get(secondId)).toBeUndefined();
});

test('unnamed parts use localized labels in the settings table', async ({ context }) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.parts = [
    { id: 'one', name: '', instrument: 'piano' },
    { id: 'two', name: '', instrument: 'guitar' },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '乐谱设置' }).click();
  const dialog = page.getByRole('dialog', { name: '乐谱设置' });
  await expect(dialog.getByLabel('声部 1')).toBeVisible();
  await expect(dialog.getByLabel('声部 2')).toBeVisible();
});

test('audition uses the selected part sound for simultaneous notes', async ({ context }) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.parts = [
    { id: 'part', name: 'Violin', instrument: 'violin' },
    { id: 'part-b', name: 'Guitar', instrument: 'guitar' },
  ];
  timeline.notes = [
    { partId: 'part', instrument: 'violin', pitch: 60, startSeconds: 0, durationSeconds: 0.8 },
    { partId: 'part-b', instrument: 'guitar', pitch: 67, startSeconds: 0, durationSeconds: 0.8 },
  ];
  timeline.groups = [
    { ...timeline.groups[0]!, partId: 'part', startSeconds: 0 },
    { ...timeline.groups[1]!, partId: 'part-b', startSeconds: 0 },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  const requests: string[] = [];
  page.on('response', (response) => {
    if (response.ok()) requests.push(response.url());
  });
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.31, y: box.height * 0.18 } });
  await expect.poll(() => requests.some((url) => url.includes('/audio/violin/'))).toBe(true);
  expect(requests.some((url) => url.includes('/audio/guitar/'))).toBe(false);
  await overlay.click({ position: { x: box.width * 0.61, y: box.height * 0.18 } });
  await expect.poll(() => requests.some((url) => url.includes('/audio/guitar/'))).toBe(true);
});

test('score settings remain usable on a narrow screen and return keyboard focus to the score', async ({
  context,
}) => {
  const { page } = await pageWithPlayback(context, playback([system(1, 0, 2)]));
  await page.setViewportSize({ width: 360, height: 640 });
  const trigger = page.getByRole('button', { name: '乐谱设置' });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: '乐谱设置' });
  const select = dialog.getByLabel('钢琴');
  await expect(select).toBeVisible();
  const bounds = (await select.boundingBox())!;
  expect(bounds.x).toBeGreaterThanOrEqual(0);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(360);
  await select.selectOption('guitar');
  await expect(select).toHaveValue('guitar');
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('main')).toBeFocused();
  await page.keyboard.press('Space');
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
});

for (const format of ['ogg', 'mp3'] as const) {
  test(`loads and decodes piano ${format} samples from the catalog`, async ({ context }) => {
    await context.addInitScript((selected) => {
      HTMLMediaElement.prototype.canPlayType = () => (selected === 'ogg' ? 'probably' : '');
    }, format);
    const loaded: string[] = [];
    context.on('response', (response) => {
      if (response.url().includes('/audio/salamander/') && response.ok())
        loaded.push(response.url());
    });
    const { page } = await pageWithPlayback(context, playback([system(1, 0, 2)]));
    await page.getByRole('button', { name: '播放', exact: true }).click();
    await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
    expect(loaded).toHaveLength(30);
    expect(loaded.every((url) => new URL(url).pathname.endsWith(`.${format}`))).toBe(true);
  });
}

function noteSvg(elements: GeometryElement[]) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140">${elements
    .map(
      (element) =>
        `<g data-ossia-graphic="${element.graphicIds[0]}"><path d="M${element.bounds.x0} ${element.bounds.y0}h3v3h-3z" fill="black"/></g>`,
    )
    .join('')}</svg>`;
}

async function pageWithNotes(
  context: Parameters<typeof mockScoreRuntime>[0],
  timeline: ScorePlayback,
  elements: GeometryElement[],
) {
  const runtime = await mockScoreRuntime(context, {
    elements,
    playback: timeline,
    svg: noteSvg(elements),
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  return { runtime, page };
}

test('play button starts at the first note, pause keeps the playhead and space resumes', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.durationSeconds = 30;
  const { page } = await pageWithPlayback(context, timeline);
  const play = page.getByRole('button', { name: '播放', exact: true });
  await expect(play).toBeEnabled();
  await play.click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const line = page.locator('.score-playhead').first();
  await expect(line).toHaveCount(1);
  await expect(page.locator('.playback-active')).toHaveCount(0);
  const started = Number(await line.getAttribute('x1'));
  expect(started).toBeCloseTo(31.5, 1);
  await page.getByRole('button', { name: '暂停', exact: true }).click();
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeVisible();
  const paused = Number(await line.getAttribute('x1'));
  await page.waitForTimeout(300);
  expect(Number(await line.getAttribute('x1'))).toBe(paused);
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Space');
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
  await expect(line).toHaveCount(1);
  await expect(page.locator('.playback-active')).toHaveCount(0);
});

test('a concurrent staff rest does not capture the first playhead frame', async ({ context }) => {
  const elements = [rest('upper-rest', 20), positionedNote('lower-note', 30, 24, 60)];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 1,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['upper-rest'],
        pitches: [],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 0,
        durationSeconds: 2,
      },
      {
        partId: 'part',
        elementIds: ['lower-note'],
        pitches: [60],
        staffIndex: 1,
        voice: 'lower',
        startSeconds: 0,
        durationSeconds: 1,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const line = page.locator('.score-playhead').first();
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeCloseTo(31.5, 1);
});

test('historical versions expose playback, audition and restore controls', async ({ context }) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.durationSeconds = 30;
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '乐谱设置' }).click();
  await page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴').selectOption('violin');
  await expect(page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴')).toHaveValue(
    'violin',
  );
  await page
    .getByRole('dialog', { name: '乐谱设置' })
    .getByRole('button', { name: '关闭' })
    .click();
  await page.locator('.score-version-control button').click();
  await page.getByRole('menuitemradio', { name: /版本 1/ }).click();
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(page.getByRole('button', { name: '打开标注' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '恢复至此版本' })).toBeVisible();
  await page.getByRole('button', { name: '乐谱设置' }).click();
  await expect(page.getByRole('dialog', { name: '乐谱设置' }).getByLabel('钢琴')).toHaveValue(
    'violin',
  );
  await page
    .getByRole('dialog', { name: '乐谱设置' })
    .getByRole('button', { name: '关闭' })
    .click();

  const play = page.getByRole('button', { name: '播放', exact: true });
  await expect(play).toBeEnabled();
  await play.click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.getByRole('button', { name: '暂停', exact: true }).click();
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );
});

test('clicking notes auditions and keeps only the latest notehead active', async ({ context }) => {
  const { page } = await pageWithPlayback(context, playback([system(1, 0, 2)]));
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="first-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="first-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(page.locator('.element-selection')).toHaveCount(0);
  const line = page.locator('.score-playhead').first();
  await expect(line).toHaveCount(0);

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await expect(line).toHaveCount(1);
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeGreaterThan(60);
});

test('escape clears the audition highlight but keeps the next playback start', async ({
  context,
}) => {
  const { page } = await pageWithPlayback(context, playback([system(1, 0, 2)]));
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );

  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Escape');
  await expect(page.locator('.playback-active')).toHaveCount(0);
  await expect(page.locator('.score-playhead')).toHaveCount(0);

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const line = page.locator('.score-playhead').first();
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeGreaterThan(60);
});

test('escape leaves the audition highlight alone while playing or annotating', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.durationSeconds = 30;
  const { page } = await pageWithPlayback(context, timeline);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Escape');
  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );

  await page.getByRole('button', { name: '暂停', exact: true }).click();
  await page.keyboard.press('e');
  await expect(page.getByRole('button', { name: '关闭标注' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: '打开标注' })).toBeVisible();
  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );
});

test('clicking a note keeps a paused playhead until playback resumes from it', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.durationSeconds = 30;
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.getByRole('button', { name: '暂停', exact: true }).click();

  const line = page.locator('.score-playhead').first();
  const pausedX = await line.getAttribute('x1');
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });

  await expect(page.locator('[data-ossia-graphic="second-graphic"].playback-active')).toHaveCount(
    2,
  );
  expect(await line.getAttribute('x1')).toBe(pausedX);

  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('Space');
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeGreaterThan(60);
});

test('starting playback places the first playhead frame on the auditioned note', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 2)]);
  timeline.groups[0] = { ...timeline.groups[0]!, durationSeconds: 1 };
  const { page } = await pageWithPlayback(context, timeline);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.61, y: (box.height * 25) / 140 } });
  await expect(page.locator('.score-playhead')).toHaveCount(0);

  await page.evaluate(() => {
    const samples: string[] = [];
    (window as unknown as { playheadSamples: string[] }).playheadSamples = samples;
    const record = () => {
      const x = document.querySelector('.score-playhead')?.getAttribute('x1');
      if (x && samples.at(-1) !== x) samples.push(x);
    };
    new MutationObserver(record).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['x1', 'x2'],
    });
  });

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const samples = await page.evaluate(
    () => (window as unknown as { playheadSamples: string[] }).playheadSamples,
  );
  expect(Number(samples[0])).toBeCloseTo(61.5, 1);
});

for (const [side, y] of [
  ['above', 12],
  ['below', 42],
] as const) {
  test(`playhead follows a note ${side} its staff system`, async ({ context }) => {
    const timeline = playback([system(1, 0, 3)]);
    timeline.durationSeconds = 3;
    await mockScoreRuntime(context, {
      elementsByPage: { 1: [note('first', 30, 60), positionedNote('second', 60, y, 86)], 2: [] },
      playback: timeline,
      svg,
    });
    const page = await context.newPage();
    await page.goto(`/scores?documentId=${firstId}`);
    await expect(page.locator('.score-image')).toHaveCount(2);
    await page.getByRole('button', { name: '播放', exact: true }).click();
    const line = page.locator('.score-playhead');
    await expect(line).toHaveAttribute('x1', '31.5', { timeout: 20_000 });
    await expect(line).toHaveAttribute('x1', '61.5', { timeout: 3_000 });
  });
}

test('playhead chooses the earlier page for a tie spanning pages', async ({ context }) => {
  const timeline = playback([system(1, 0, 1), system(2, 1, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [{ ...timeline.groups[0]!, elementIds: ['second', 'first'], startSeconds: 0 }];
  await mockScoreRuntime(context, {
    elementsByPage: { 1: [note('first', 30, 60)], 2: [note('second', 10, 60)] },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.locator('.page-host').nth(0).locator('.score-playhead')).toHaveAttribute(
    'x1',
    '31.5',
    { timeout: 20_000 },
  );
  await expect(page.locator('.page-host').nth(1).locator('.score-playhead')).toHaveCount(0);
});

test('playhead holds position when a tie member has no known position', async ({ context }) => {
  const timeline = playback([system(1, 0, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, startSeconds: 0 },
    { ...timeline.groups[1]!, elementIds: ['second', 'missing'], startSeconds: 0.5 },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const line = page.locator('.score-playhead');
  await expect(line).toHaveAttribute('x1', '31.5', { timeout: 20_000 });
  await page.waitForTimeout(900);
  await expect(line).toHaveAttribute('x1', '31.5');
});

test('playhead holds its last position when an upcoming note has no geometry', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, startSeconds: 0 },
    { ...timeline.groups[1]!, elementIds: ['missing'], startSeconds: 0.5 },
    { ...timeline.groups[1]!, startSeconds: 1.8 },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const line = page.locator('.score-playhead').first();
  await expect(line).toHaveAttribute('x1', '31.5', { timeout: 20_000 });
  await page.waitForTimeout(900);
  await expect(line).toHaveAttribute('x1', '31.5');
  await expect(line).toHaveAttribute('x1', '61.5', { timeout: 3_000 });
});

test('playhead holds its last position when a note has no known system', async ({ context }) => {
  const timeline = playback([system(1, 0, 3)]);
  timeline.durationSeconds = 3;
  await mockScoreRuntime(context, {
    elementsByPage: {
      1: [note('first', 30, 60), { ...positionedNote('second', 60, 80, 67), systemIndex: null }],
      2: [],
    },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const line = page.locator('.score-playhead');
  await expect(line).toHaveAttribute('x1', '31.5', { timeout: 20_000 });
  await page.waitForTimeout(1400);
  await expect(line).toHaveAttribute('x1', '31.5');
});

test('playhead stays hidden until a note has a known position', async ({ context }) => {
  const timeline = playback([system(1, 0, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, elementIds: ['missing'], startSeconds: 0 },
    { ...timeline.groups[1]!, startSeconds: 1.5 },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
  await page.waitForTimeout(600);
  await expect(page.locator('.score-playhead')).toHaveCount(0);
  await expect(page.locator('.score-playhead')).toHaveAttribute('x1', '61.5', {
    timeout: 3_000,
  });
});

test('playhead holds its page until a note on the next page has a known position', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 0.5), system(2, 0.5, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, startSeconds: 0 },
    { ...timeline.groups[1]!, elementIds: ['missing'], startSeconds: 0.5 },
    { ...timeline.groups[1]!, startSeconds: 1.8 },
  ];
  await mockScoreRuntime(context, {
    elementsByPage: { 1: [note('first', 30, 60)], 2: [note('second', 60, 67)] },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const firstPage = page.locator('.page-host').nth(0);
  const secondPage = page.locator('.page-host').nth(1);
  await expect(firstPage.locator('.score-playhead')).toHaveAttribute('x1', '31.5', {
    timeout: 20_000,
  });
  await page.waitForTimeout(900);
  await expect(firstPage.locator('.score-playhead')).toHaveAttribute('x1', '31.5');
  await expect(secondPage.locator('.score-playhead')).toHaveCount(0);
  await expect(secondPage.locator('.score-playhead')).toHaveAttribute('x1', '61.5', {
    timeout: 3_000,
  });
});

test('playhead holds its system until the next system has a known note position', async ({
  context,
}) => {
  const timeline = playback([
    system(1, 0, 0.5),
    { ...system(1, 0.5, 3), systemIndex: 1, y0: 55, y1: 70 },
  ]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, startSeconds: 0 },
    { ...timeline.groups[1]!, elementIds: ['missing'], startSeconds: 0.5 },
    { ...timeline.groups[1]!, startSeconds: 1.8 },
  ];
  await mockScoreRuntime(context, {
    elementsByPage: {
      1: [note('first', 30, 60), { ...positionedNote('second', 60, 60, 67), systemIndex: 1 }],
      2: [],
    },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const line = page.locator('.score-playhead');
  await expect(line).toHaveAttribute('y1', '18', { timeout: 20_000 });
  await page.waitForTimeout(900);
  await expect(line).toHaveAttribute('x1', '31.5');
  await expect(line).toHaveAttribute('y1', '18');
  await expect(line).toHaveAttribute('y1', '55', { timeout: 3_000 });
  await expect(line).toHaveAttribute('x1', '61.5');
});

test('playhead holds its position when the same element occurs on multiple pages', async ({
  context,
}) => {
  const timeline = playback([system(1, 0, 0.5), system(2, 0.5, 3)]);
  timeline.durationSeconds = 3;
  timeline.groups = [
    { ...timeline.groups[0]!, startSeconds: 0 },
    { ...timeline.groups[1]!, elementIds: ['repeated'], startSeconds: 0.5 },
  ];
  await mockScoreRuntime(context, {
    elementsByPage: {
      1: [note('first', 30, 60), note('repeated', 45, 67)],
      2: [note('repeated', 60, 67)],
    },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  const firstPage = page.locator('.page-host').nth(0);
  await expect(firstPage.locator('.score-playhead')).toHaveAttribute('x1', '31.5', {
    timeout: 20_000,
  });
  await page.waitForTimeout(900);
  await expect(firstPage.locator('.score-playhead')).toHaveAttribute('x1', '31.5');
  await expect(page.locator('.page-host').nth(1).locator('.score-playhead')).toHaveCount(0);
});

test('playhead snaps to the active notehead instead of moving continuously', async ({
  context,
}) => {
  const timeline = playback([{ ...system(1, 0, 3), x0: 10, x1: 90 }]);
  timeline.durationSeconds = 3;
  timeline.notes = [
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 60,
      startSeconds: 0,
      durationSeconds: 0.5,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 67,
      startSeconds: 0.6,
      durationSeconds: 0.5,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 60,
      startSeconds: 1.2,
      durationSeconds: 0.5,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 67,
      startSeconds: 1.8,
      durationSeconds: 0.5,
    },
  ];
  timeline.groups = [
    { ...timeline.groups[0]!, elementIds: ['first'], pitches: [60] },
    {
      ...timeline.groups[1]!,
      partId: 'part',
      elementIds: ['second'],
      pitches: [67],
      startSeconds: 0.6,
    },
    {
      ...timeline.groups[0]!,
      partId: 'part',
      elementIds: ['first'],
      pitches: [60],
      startSeconds: 1.2,
    },
    {
      ...timeline.groups[1]!,
      partId: 'part',
      elementIds: ['second'],
      pitches: [67],
      startSeconds: 1.8,
    },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.evaluate(() => {
    const samples: { time: number; x: number }[] = [];
    const started = performance.now();
    (window as unknown as { playheadSamples: typeof samples }).playheadSamples = samples;
    const record = () => {
      const x = Number(document.querySelector('.score-playhead')?.getAttribute('x1'));
      if (Number.isFinite(x)) samples.push({ time: performance.now() - started, x });
    };
    new MutationObserver(record).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['x1', 'x2'],
    });
  });

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.waitForTimeout(1200);
  const samples = await page.evaluate(
    () => (window as unknown as { playheadSamples: { time: number; x: number }[] }).playheadSamples,
  );
  const positions = [...new Set(samples.map((sample) => Math.round(sample.x * 10) / 10))].sort(
    (a, b) => a - b,
  );
  expect(positions).toEqual([31.5, 61.5]);
});

test('playhead does not fall back to an earlier sustained note', async ({ context }) => {
  const timeline = playback([{ ...system(1, 0, 1), x0: 10, x1: 90 }]);
  timeline.durationSeconds = 1;
  timeline.notes = [
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 60,
      startSeconds: 0,
      durationSeconds: 1,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 64,
      startSeconds: 0.2,
      durationSeconds: 0.1,
    },
    {
      partId: 'part',
      instrument: 'piano' as const,
      pitch: 65,
      startSeconds: 0.6,
      durationSeconds: 0.1,
    },
  ];
  timeline.groups = [
    { ...timeline.groups[0]!, elementIds: ['first'], pitches: [60], durationSeconds: 1 },
    {
      ...timeline.groups[1]!,
      partId: 'part',
      elementIds: ['second'],
      pitches: [64],
      startSeconds: 0.2,
      durationSeconds: 0.1,
    },
    {
      ...timeline.groups[1]!,
      partId: 'part',
      elementIds: ['second'],
      pitches: [65],
      startSeconds: 0.6,
      durationSeconds: 0.1,
    },
  ];
  const { page } = await pageWithPlayback(context, timeline);
  await page.evaluate(() => {
    const samples: number[] = [];
    (window as unknown as { playheadSamples: number[] }).playheadSamples = samples;
    const record = () => {
      const x = Number(document.querySelector('.score-playhead')?.getAttribute('x1'));
      if (Number.isFinite(x) && samples.at(-1) !== x) samples.push(x);
    };
    new MutationObserver(record).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['x1', 'x2'],
    });
  });

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.waitForTimeout(800);
  const samples = await page.evaluate(
    () => (window as unknown as { playheadSamples: number[] }).playheadSamples,
  );
  expect(samples.map((x) => Math.round(x * 10) / 10)).toEqual([31.5, 61.5]);
});

test('clicking one notehead at a shared onset activates only that note', async ({ context }) => {
  const elements = [positionedNote('low', 30, 24, 60), positionedNote('high', 31, 19, 67)];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['low'],
        pitches: [60],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['high'],
        pitches: [67],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;

  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(0);

  await overlay.click({ position: { x: box.width * 0.32, y: (box.height * 20) / 140 } });
  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('.score-playhead')).toHaveCount(0);
});

test('clicking one notation of the same note activates only that notation', async ({ context }) => {
  const staff = positionedNote('staff-note', 30, 24, 60);
  const tabBase = positionedNote('tab-note', 30, 55, 60);
  const tab = {
    ...tabBase,
    region: {
      ...tabBase.region,
      elementKind: 'TabNoteHead',
      staffIndex: 1,
    },
  };
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['staff-note', 'tab-note'],
        pitches: [60],
        staffIndex: 0,
        voice: 'music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, [staff, tab]);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;

  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 56) / 140 } });
  await expect(page.locator('[data-ossia-graphic="tab-note-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(
    page.locator('[data-ossia-graphic="staff-note-graphic"].playback-active'),
  ).toHaveCount(0);

  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
  await expect(
    page.locator('[data-ossia-graphic="staff-note-graphic"].playback-active'),
  ).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="tab-note-graphic"].playback-active')).toHaveCount(
    0,
  );
});

test('alt-clicking a note auditions every sounding note at that onset', async ({ context }) => {
  const high = positionedNote('high', 31, 19, 67);
  high.region.staffIndex = 1;
  const elements = [positionedNote('low', 30, 24, 60), high, positionedNote('later', 60, 24, 72)];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 0,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 72,
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['low'],
        pitches: [60],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['high'],
        pitches: [67],
        staffIndex: 1,
        voice: 'lower',
        startSeconds: 0,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        elementIds: ['later'],
        pitches: [72],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;

  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(0);

  await overlay.click({
    position: { x: box.width * 0.31, y: (box.height * 25) / 140 },
    modifiers: ['Alt'],
  });
  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="later-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('.score-playhead')).toHaveCount(0);

  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="later-graphic"].playback-active')).toHaveCount(2);
});

test('alt-arrow keys audition the previous and next simultaneous onset', async ({ context }) => {
  const elements = [
    positionedNote('first-a', 30, 24, 60),
    positionedNote('first-b', 31, 19, 64),
    positionedNote('next-a', 60, 24, 67),
    positionedNote('next-b', 61, 19, 72),
  ];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 64,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 1,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 72,
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['first-a'],
        pitches: [60],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['first-b'],
        pitches: [64],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['next-a'],
        pitches: [67],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 1,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['next-b'],
        pitches: [72],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 1,
        durationSeconds: 0.8,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;

  await overlay.click({
    position: { x: box.width * 0.31, y: (box.height * 25) / 140 },
    modifiers: ['Alt'],
  });
  await expect(page.locator('[data-ossia-graphic="first-a-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(page.locator('[data-ossia-graphic="first-b-graphic"].playback-active')).toHaveCount(
    2,
  );

  await page.keyboard.press('Alt+ArrowRight');
  await expect(page.locator('[data-ossia-graphic="first-a-graphic"].playback-active')).toHaveCount(
    0,
  );
  await expect(page.locator('[data-ossia-graphic="first-b-graphic"].playback-active')).toHaveCount(
    0,
  );
  await expect(page.locator('[data-ossia-graphic="next-a-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(page.locator('[data-ossia-graphic="next-b-graphic"].playback-active')).toHaveCount(
    2,
  );

  await page.keyboard.press('Alt+ArrowLeft');
  await expect(page.locator('[data-ossia-graphic="first-a-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(page.locator('[data-ossia-graphic="first-b-graphic"].playback-active')).toHaveCount(
    2,
  );
  await expect(page.locator('[data-ossia-graphic="next-a-graphic"].playback-active')).toHaveCount(
    0,
  );
  await expect(page.locator('[data-ossia-graphic="next-b-graphic"].playback-active')).toHaveCount(
    0,
  );
});

test('playback shortcuts do not fire while an annotation input is focused', async ({ context }) => {
  const { page } = await pageWithPlayback(context, playback([system(1, 0, 2)]));
  await page.keyboard.press('e');
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
  const input = page.getByRole('textbox', { name: '标注' });
  await input.focus();
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeVisible();
  await expect(page.locator('.playback-active')).toHaveCount(0);
  await expect(input).toHaveValue(' ');
});

test('arrow keys audition adjacent notes in the same voice from the latest audition', async ({
  context,
}) => {
  const elements = [
    note('a1', 20, 60),
    note('b1', 32, 62),
    note('a2', 50, 64),
    note('b2', 62, 65),
    note('a3', 80, 67),
  ];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 3,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 62,
        startSeconds: 0.5,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 64,
        startSeconds: 1,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 65,
        startSeconds: 1.5,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 2,
        durationSeconds: 0.4,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['a1'],
        pitches: [60],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 0,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        elementIds: ['b1'],
        pitches: [62],
        staffIndex: 0,
        voice: 'lower',
        startSeconds: 0.5,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        elementIds: ['a2'],
        pitches: [64],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 1,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        elementIds: ['b2'],
        pitches: [65],
        staffIndex: 0,
        voice: 'lower',
        startSeconds: 1.5,
        durationSeconds: 0.4,
      },
      {
        partId: 'part',
        elementIds: ['a3'],
        pitches: [67],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 2,
        durationSeconds: 0.4,
      },
    ],
    systems: [system(1, 0, 3)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());

  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="a1-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="a1-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="a2-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="a3-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="a3-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('[data-ossia-graphic="a2-graphic"].playback-active')).toHaveCount(2);

  await page.reload();
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('[data-ossia-graphic="a1-graphic"].playback-active')).toHaveCount(2);

  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.63, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="b2-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowLeft');
  await expect(page.locator('[data-ossia-graphic="b2-graphic"].playback-active')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="b1-graphic"].playback-active')).toHaveCount(2);
});

test('rests participate in click, arrow and automatic playback without sound', async ({
  context,
}) => {
  const elements = [note('a1', 20, 60), rest('r1', 35), note('a2', 50, 64)];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.1,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 64,
        startSeconds: 1.5,
        durationSeconds: 0.3,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['a1'],
        pitches: [60],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0,
        durationSeconds: 0.1,
      },
      {
        partId: 'part',
        elementIds: ['r1'],
        pitches: [],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 0.1,
        durationSeconds: 1.4,
      },
      {
        partId: 'part',
        elementIds: ['a2'],
        pitches: [64],
        staffIndex: 0,
        voice: 'Music',
        startSeconds: 1.5,
        durationSeconds: 0.3,
      },
    ],
    systems: [measureSpan(1, 0, 2, 10, 90)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.36, y: (box.height * 21) / 140 } });
  await expect(page.locator('[data-ossia-graphic="r1-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('.score-playhead')).toHaveCount(0);

  await overlay.click({ position: { x: box.width * 0.21, y: (box.height * 25) / 140 } });
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="r1-graphic"].playback-active')).toHaveCount(2);
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="a2-graphic"].playback-active')).toHaveCount(2);

  await page.reload();
  await expect(page.locator('.score-image')).toHaveCount(2);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const line = page.locator('.score-playhead').first();
  await expect
    .poll(async () => {
      const x = Number(await line.getAttribute('x1'));
      return x > 30 && x < 45;
    })
    .toBe(true);
  await page.getByRole('button', { name: '暂停', exact: true }).click();
});

test('alt-clicking a note while paused keeps the paused playhead', async ({ context }) => {
  const elements = [positionedNote('low', 30, 24, 60), positionedNote('high', 31, 19, 67)];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 2,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['low'],
        pitches: [60],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
      {
        partId: 'part',
        elementIds: ['high'],
        pitches: [67],
        staffIndex: 0,
        voice: 'lower',
        startSeconds: 0,
        durationSeconds: 0.8,
      },
    ],
    systems: [system(1, 0, 2)],
  };
  const { page } = await pageWithNotes(context, timeline, elements);
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await page.getByRole('button', { name: '暂停', exact: true }).click();

  const line = page.locator('.score-playhead').first();
  const pausedX = await line.getAttribute('x1');
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({
    position: { x: box.width * 0.31, y: (box.height * 25) / 140 },
    modifiers: ['Alt'],
  });

  await expect(page.locator('[data-ossia-graphic="low-graphic"].playback-active')).toHaveCount(2);
  await expect(page.locator('[data-ossia-graphic="high-graphic"].playback-active')).toHaveCount(2);
  expect(await line.getAttribute('x1')).toBe(pausedX);
});

test('arrow keys seek by measure during playback without changing the audition anchor', async ({
  context,
}) => {
  const elements = [
    note('a1', 20, 60),
    note('b1', 32, 62),
    note('a2', 50, 64),
    note('b2', 62, 65),
    note('a3', 80, 67),
  ];
  const timeline: ScorePlayback = {
    renderKey: hash,
    sourceHash: hash,
    durationSeconds: 3,
    parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
    notes: [
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 60,
        startSeconds: 0,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 62,
        startSeconds: 0.5,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 64,
        startSeconds: 1,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 65,
        startSeconds: 1.5,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        instrument: 'piano' as const,
        pitch: 67,
        startSeconds: 2,
        durationSeconds: 0.6,
      },
    ],
    groups: [
      {
        partId: 'part',
        elementIds: ['a1'],
        pitches: [60],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 0,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        elementIds: ['b1'],
        pitches: [62],
        staffIndex: 0,
        voice: 'lower',
        startSeconds: 0.5,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        elementIds: ['a2'],
        pitches: [64],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 1,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        elementIds: ['b2'],
        pitches: [65],
        staffIndex: 0,
        voice: 'lower',
        startSeconds: 1.5,
        durationSeconds: 0.6,
      },
      {
        partId: 'part',
        elementIds: ['a3'],
        pitches: [67],
        staffIndex: 0,
        voice: 'upper',
        startSeconds: 2,
        durationSeconds: 0.6,
      },
    ],
    systems: [
      measureSpan(1, 0, 1, 10, 40),
      measureSpan(2, 1, 2, 40, 80),
      measureSpan(3, 2, 3, 80, 90),
    ],
  };
  timeline.durationSeconds *= 20;
  for (const note of timeline.notes) {
    note.startSeconds *= 20;
    note.durationSeconds *= 20;
  }
  for (const group of timeline.groups) {
    if (group.startSeconds !== null) group.startSeconds *= 20;
    if (group.durationSeconds !== null) group.durationSeconds *= 20;
  }
  for (const span of timeline.systems) {
    span.startSeconds *= 20;
    span.endSeconds *= 20;
  }
  const { page } = await pageWithNotes(context, timeline, elements);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  await overlay.click({ position: { x: box.width * 0.33, y: (box.height * 25) / 140 } });
  await expect(page.locator('[data-ossia-graphic="b1-graphic"].playback-active')).toHaveCount(2);

  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  const line = page.locator('.score-playhead').first();
  await page.keyboard.press('ArrowRight');
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeGreaterThan(45);
  await page.keyboard.press('ArrowLeft');
  await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeLessThan(40);

  await page.getByRole('button', { name: '暂停', exact: true }).click();
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('[data-ossia-graphic="b2-graphic"].playback-active')).toHaveCount(2);
});

test('playback moves the playhead without panning the score canvas', async ({ context }) => {
  const timeline = playback([system(1, 0, 0.1), system(2, 0.1, 20)]);
  timeline.durationSeconds = 20;
  await mockScoreRuntime(context, {
    elementsByPage: { 1: [note('first', 30, 60)], 2: [note('second', 60, 67)] },
    playback: timeline,
    svg,
  });
  const page = await context.newPage();
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const firstPage = page.locator('.page-host').first();
  const initialTop = (await firstPage.boundingBox())!.y;
  await page.getByRole('button', { name: '播放', exact: true }).click();
  await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible({
    timeout: 20_000,
  });
  await expect(page.locator('.page-host').nth(1).locator('.score-playhead')).toHaveCount(1);
  expect(Math.abs((await firstPage.boundingBox())!.y - initialTop)).toBeLessThan(1);
  await page.getByRole('button', { name: '暂停', exact: true }).click();
});

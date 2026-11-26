import { readFileSync } from 'node:fs';

import { expect, test, type BrowserContext, type Page } from '@playwright/test';

import type {
  GeometryElement,
  RenderGeometry,
  ScorePlayback,
} from '../../src/generated/protocol.ts';
import { hitElement } from '../../src/score/utils/hitElement.ts';
import { sameVoice } from '../../src/score/utils/playbackTimeline.ts';
import { firstId, hash, mockScoreRuntime } from './score-runtime.ts';

interface Fixture {
  id: string;
  geometry: RenderGeometry;
  playback: ScorePlayback;
  pages: [number, string][];
  trace: { time: number; page: number; system: number; x: number }[];
  auditionAttacks: Record<number, number[]>;
}
const input = process.env.OSSIA_AUDITION_FIXTURES;
const loadedFixtures: Fixture[] = input ? JSON.parse(readFileSync(input, 'utf8')) : [];
// Spread the long complete scores across Playwright's contiguous test shards.
const midpoint = Math.ceil(loadedFixtures.length / 2);
const fixtures = loadedFixtures.flatMap((_, index) =>
  index < midpoint
    ? [loadedFixtures[index], loadedFixtures[index + midpoint]].filter(
        (fixture): fixture is Fixture => fixture !== undefined,
      )
    : [],
);
if (!input) test.skip('engraved audition fixtures require pnpm verify:audition', () => {});

async function openFixture(context: BrowserContext, page: Page, fixture: Fixture) {
  const timeline = { ...fixture.playback, renderKey: hash, sourceHash: hash };
  const geometry = { ...fixture.geometry, renderKey: hash, sourceHash: hash };
  await mockScoreRuntime(context, { playback: timeline, pageCount: fixture.pages.length });
  await context.route('**/api/v1/renders/*/geometry', (route) => route.fulfill({ json: geometry }));
  await context.route('**/api/v1/renders/*/pages/*', (route) => {
    const number = Number(new URL(route.request().url()).pathname.split('/').at(-1));
    const svg = fixture.pages.find(([page]) => page === number)?.[1];
    return route.fulfill({
      status: svg ? 200 : 404,
      contentType: 'image/svg+xml',
      body: svg ?? '',
    });
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('[data-page="1"] .score-image')).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole('button', { name: '播放', exact: true })).toBeEnabled();
  return { timeline, geometry };
}

async function showPage(page: Page, number: number) {
  const host = page.locator(`[data-page="${number}"]`);
  const box = (await host.boundingBox())!;
  const viewport = (await page.locator('.score-scroll').boundingBox())!;
  await page.locator('.score-scroll').dispatchEvent('wheel', { deltaY: box.y - viewport.y - 32 });
  await expect.poll(async () => (await host.boundingBox())!.y).toBeCloseTo(viewport.y + 32, 0);
  await expect(host.locator('.score-image')).toBeVisible();
}

async function audioModuleUrl(page: Page) {
  return page.evaluate(() => {
    const entry = performance
      .getEntriesByType('resource')
      .find(({ name }) => new URL(name).pathname === '/src/score/scoreAudio.ts');
    if (!entry) throw new Error('Score audio module was not loaded');
    return entry.name;
  });
}

test.describe.configure({ mode: 'parallel' });
// Repeated SVG snapshots dominate full-score traversal; retain actions and failure screenshots.
test.use({
  trace: { mode: 'retain-on-failure', snapshots: false, screenshots: false },
  screenshot: 'only-on-failure',
});

for (const fixture of fixtures) {
  if (fixture.id === 'bach-goldberg-aria') {
    test('Goldberg mordent and appoggiatura use their performed durations', async ({
      context,
      page,
    }) => {
      await openFixture(context, page, fixture);
      await page.evaluate(
        async (modulePath) => {
          const { scoreAudio } = await import(modulePath);
          let origin: number | undefined;
          Reflect.set(window, 'fixtureClock', 0);
          Reflect.set(window, 'ornamentAttacks', []);
          scoreAudio.now = () => Reflect.get(window, 'fixtureClock');
          Reflect.set(scoreAudio, 'load', async () => {});
          Reflect.set(scoreAudio, 'resume', async () => {});
          scoreAudio.play = async (
            [pitch]: number[],
            start: number,
            duration: number,
            _instrument: string,
            exact: boolean,
          ) => {
            origin ??= start;
            if (exact && start - origin < 4) {
              Reflect.get(window, 'ornamentAttacks').push([
                pitch,
                Math.round((start - origin) * 1e3),
                Math.round(duration * 1e3),
              ]);
            }
          };
        },
        await audioModuleUrl(page),
      );
      await page.getByRole('button', { name: '播放', exact: true }).click();
      await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
      const attacks = () =>
        page.evaluate(() => Reflect.get(window, 'ornamentAttacks') as number[][]);
      for (const [clock, count] of [
        [1.96, 2],
        [2.1, 4],
        [2.23, 6],
        [3.42, 7],
        [3.67, 8],
      ]) {
        await page.evaluate((time) => Reflect.set(window, 'fixtureClock', time), clock);
        await expect.poll(async () => (await attacks()).length).toBeGreaterThanOrEqual(count!);
      }
      expect(await attacks()).toEqual([
        [83, 2000, 65],
        [81, 2065, 65],
        [83, 2130, 65],
        [81, 2195, 65],
        [79, 2260, 65],
        [81, 2325, 425],
        [79, 3500, 250],
        [78, 3750, 250],
      ]);
    });
  }

  for (const scorePage of fixture.geometry.pages) {
    test(`${fixture.id}: page ${scorePage.page} noteheads, arrows and Alt audition`, async ({
      context,
      page,
    }) => {
      test.setTimeout(900_000);
      const { timeline } = await openFixture(context, page, fixture);
      await page.evaluate(
        async (modulePath) => {
          const { scoreAudio } = await import(modulePath);
          const play = scoreAudio.play.bind(scoreAudio);
          const calls: number[][] = [];
          Reflect.set(window, 'auditionCalls', calls);
          scoreAudio.play = async (
            pitches: number[],
            start: number,
            duration: number,
            instrument: string,
          ) => {
            calls.push([...pitches]);
            await play(pitches, start, duration, instrument);
          };
        },
        await audioModuleUrl(page),
      );
      {
        await showPage(page, scorePage.page);
        const playable = scorePage.elements.filter(
          (element) =>
            ['NoteHead', 'TabNoteHead', 'Rest', 'MultiMeasureRest'].includes(
              element.region.elementKind ?? '',
            ) &&
            element.region.target.kind === 'element' &&
            timeline.groups.some(
              (group) =>
                element.region.target.kind === 'element' &&
                group.elementIds.includes(element.region.target.elementId),
            ),
        );
        const heads = scorePage.elements.filter((element) =>
          ['NoteHead', 'TabNoteHead'].includes(element.region.elementKind ?? ''),
        );
        expect(playable.length).toBeGreaterThan(0);
        const groupFor = (element: GeometryElement) =>
          timeline.groups.find(
            (group) =>
              element.region.target.kind === 'element' &&
              group.elementIds.includes(element.region.target.elementId),
          )!;
        const reset = () => page.evaluate(() => Reflect.get(window, 'auditionCalls').splice(0));
        const sounds = () =>
          page.evaluate(() =>
            (Reflect.get(window, 'auditionCalls') as number[][]).flat().sort((a, b) => a - b),
          );
        const click = async (element: GeometryElement, alt = false) => {
          const points = [element.bounds, ...element.hitBounds].flatMap((bounds) =>
            [0.25, 0.5, 0.75].flatMap((x) =>
              [0.25, 0.5, 0.75].map((y) => ({
                x: bounds.x0 + (bounds.x1 - bounds.x0) * x,
                y: bounds.y0 + (bounds.y1 - bounds.y0) * y,
              })),
            ),
          );
          const point =
            points.find((p) => {
              return hitElement(scorePage, p.x, p.y) === element;
            }) ?? points[4]!;
          const overlay = page.locator(`[data-page="${scorePage.page}"] .overlay`);
          const box = (await overlay.boundingBox())!;
          const view = scorePage.viewBox!;
          const viewport = (await page.locator('.score-scroll').boundingBox())!;
          const targetY = box.y + ((point.y - view.y0) / (view.y1 - view.y0)) * box.height;
          if (targetY < viewport.y + 32 || targetY > viewport.y + viewport.height - 32) {
            await page
              .locator('.score-scroll')
              .dispatchEvent('wheel', { deltaY: targetY - viewport.y - viewport.height / 2 });
            await expect
              .poll(async () => {
                const current = (await overlay.boundingBox())!;
                return current.y + ((point.y - view.y0) / (view.y1 - view.y0)) * current.height;
              })
              .toBeCloseTo(viewport.y + viewport.height / 2, 0);
          }
          // The browser can round a fractional click outside a stem's hit region.
          const pixels = await overlay.evaluate(
            (node, points) => {
              if (!(node instanceof SVGSVGElement)) throw new Error('Missing score overlay');
              const transform = node.getScreenCTM()!;
              const inverse = transform.inverse();
              return points.flatMap((point) => {
                const screen = new DOMPoint(point.x, point.y).matrixTransform(transform);
                return [Math.floor(screen.x), Math.ceil(screen.x)].flatMap((x) =>
                  [Math.floor(screen.y), Math.ceil(screen.y)].map((y) => {
                    const score = new DOMPoint(x, y).matrixTransform(inverse);
                    return { x, y, scoreX: score.x, scoreY: score.y };
                  }),
                );
              });
            },
            [point, ...points],
          );
          const targets = pixels.map((pixel) => ({
            ...pixel,
            hit: hitElement(scorePage, pixel.scoreX, pixel.scoreY),
          }));
          const target =
            targets.find(({ hit }) => hit === element) ??
            targets.find(({ hit }) => {
              if (!hit || !heads.includes(hit) || !heads.includes(element)) return false;
              const { x0, x1, y0, y1 } = element.bounds;
              return (
                hit.bounds.x0 === x0 &&
                hit.bounds.x1 === x1 &&
                hit.bounds.y0 === y0 &&
                hit.bounds.y1 === y1 &&
                groupFor(hit).startSeconds === groupFor(element).startSeconds &&
                groupFor(hit).pitches[0] === groupFor(element).pitches[0]
              );
            });
          expect(target, `Clickable pixel for ${element.region.label}`).toBeDefined();
          if (alt) await page.keyboard.down('Alt');
          try {
            await page.mouse.click(target!.x, target!.y);
          } finally {
            if (alt) await page.keyboard.up('Alt');
          }
          return target!.hit!;
        };
        const assertGroup = async (
          group: ScorePlayback['groups'][number],
          clicked?: GeometryElement,
        ) => {
          const element =
            clicked ??
            playable.find(
              (element) =>
                element.region.target.kind === 'element' &&
                group.elementIds.includes(element.region.target.elementId),
            )!;
          await expect(
            page
              .locator(
                `[data-page="${scorePage.page}"] [data-ossia-graphic="${element.graphicIds[0]}"].playback-active`,
              )
              .first(),
          ).toBeAttached();
          await expect
            .poll(sounds, { message: element.region.label })
            .toEqual([...group.pitches].sort((a, b) => a - b));
        };
        for (const head of heads) {
          await reset();
          const selected = await click(head);
          expect(groupFor(selected).pitches).toEqual(groupFor(head).pitches);
          expect(groupFor(selected).startSeconds).toEqual(groupFor(head).startSeconds);
          await assertGroup(groupFor(head), selected);
        }
        const visited = new Set<string>();
        for (const head of playable) {
          const anchor = groupFor(head);
          const key = `${anchor.staffIndex}:${anchor.voice}`;
          if (visited.has(key)) continue;
          visited.add(key);
          const sequence = timeline.groups.filter((group) => sameVoice(group, anchor));
          const visible = new Set(playable.map(groupFor));
          const first = sequence.findIndex((group) => visible.has(group));
          const startHead = playable.find((element) => groupFor(element) === sequence[first])!;
          await click(startHead);
          let last = first;
          for (let i = first + 1; i < sequence.length && visible.has(sequence[i]!); i++) {
            await reset();
            await page.keyboard.press('ArrowRight');
            await assertGroup(sequence[i]!);
            last = i;
          }
          for (let i = last - 1; i >= first; i--) {
            await reset();
            await page.keyboard.press('ArrowLeft');
            await assertGroup(sequence[i]!);
          }
        }
        const onsets = new Set<number>();
        for (const head of heads) {
          const group = groupFor(head);
          if (group.startSeconds === null || onsets.has(group.startSeconds)) continue;
          onsets.add(group.startSeconds);
          const expected = group.pitches.length ? fixture.auditionAttacks[group.startSeconds] : [];
          await reset();
          await click(head, true);
          await expect.poll(sounds, { message: `Alt at ${group.startSeconds}` }).toEqual(expected);
        }
      }
    });
  }

  test(`${fixture.id}: playhead visits every onset on every page`, async ({ context, page }) => {
    test.setTimeout(180_000);
    await openFixture(context, page, fixture);
    await page.evaluate(
      async ({ modulePath, firstOnset }) => {
        const { scoreAudio } = await import(modulePath);
        Reflect.set(window, 'fixtureClock', 0);
        Reflect.set(window, 'fixtureLead', null);
        Reflect.set(scoreAudio, 'load', async () => {});
        Reflect.set(scoreAudio, 'resume', async () => {});
        scoreAudio.now = () => Reflect.get(window, 'fixtureClock');
        scoreAudio.play = async (_pitches: number[], start: number) => {
          if (Reflect.get(window, 'fixtureLead') === null) {
            const lead = start - firstOnset;
            Reflect.set(window, 'fixtureLead', lead);
            Reflect.set(window, 'fixtureClock', Reflect.get(window, 'fixtureClock') + lead);
          }
        };
      },
      {
        modulePath: await audioModuleUrl(page),
        firstOnset: fixture.playback.notes[0]?.startSeconds ?? 0,
      },
    );
    await page.getByRole('button', { name: '播放', exact: true }).click();
    await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
    let currentPage = 0;
    for (const frame of fixture.trace) {
      if (currentPage !== frame.page) {
        await showPage(page, frame.page);
        currentPage = frame.page;
      }
      await page.evaluate((time) => {
        Reflect.set(window, 'fixtureClock', time + Reflect.get(window, 'fixtureLead'));
      }, frame.time);
      const line = page.locator(`[data-page="${frame.page}"] .score-playhead`);
      await expect
        .poll(async () => Number(await line.getAttribute('x1')), {
          message: `playhead at ${frame.time}`,
          intervals: [16],
        })
        .toBeCloseTo(frame.x, 4);
    }
    if (fixture.id === 'bach-goldberg-aria') {
      const last = fixture.trace.at(-1)!;
      await page.evaluate(async () => {
        Reflect.set(window, 'fixtureClock', 96.75 + Reflect.get(window, 'fixtureLead'));
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        );
      });
      await expect(page.getByRole('button', { name: '暂停', exact: true })).toBeVisible();
      const line = page.locator(`[data-page="${last.page}"] .score-playhead`);
      await expect.poll(async () => Number(await line.getAttribute('x1'))).toBeCloseTo(last.x, 4);
    }
    await page.getByRole('button', { name: '暂停', exact: true }).click();
  });
}

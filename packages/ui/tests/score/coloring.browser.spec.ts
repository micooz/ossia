import { expect, test } from '@playwright/test';

import type { GeometryElement, SelectionTarget } from '../../src/generated/protocol.ts';
import { firstId, mockScoreRuntime } from './score-runtime.ts';

const target = (elementId: string): SelectionTarget => ({ kind: 'element', elementId });
function note(
  id: string,
  x: number,
  y: number,
  ids = [id],
  linked = [target(id)],
): GeometryElement {
  return {
    bounds: { x0: x, x1: x + 2, y0: y, y1: y + 2 },
    priority: 0,
    hitBounds: [],
    graphicIds: ids,
    linkedTargets: linked,
    region: {
      target: target(id),
      label: id,
      elementKind: 'NoteHead',
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

const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140">
<g data-ossia-graphic="c"><path d="M30 24h2v2h-2z" fill="none" stroke="black"/></g>
<g data-ossia-graphic="e"><path d="M30 21h2v2h-2z" fill="black"/></g>
<g data-ossia-graphic="stem"><path d="M32 25v-7" fill="none" stroke="black"/></g>
<g data-ossia-graphic="beam"><path d="M32 18h20v1h-20z" fill="black"/></g>
<g data-ossia-graphic="next"><path d="M60 24h2v2h-2z" fill="black"/></g>
<g data-ossia-graphic="tie"><path d="M32 27Q45 32 60 27" fill="none" stroke="black"/></g>
<g data-ossia-graphic="script"><path d="M31 29h1v1h-1z" fill="black"/></g>
<g data-ossia-graphic="neighbor"><path d="M50 24h2v2h-2z" fill="black"/></g>
</svg>`;

test('note ink, shared beams and a tied continuation color together and restore exactly', async ({
  context,
  page,
}) => {
  const linked = [target('c'), target('next'), target('tie'), target('script')];
  const runtime = await mockScoreRuntime(context, {
    svg,
    elements: [
      note('c', 30, 24, ['c', 'stem', 'beam', 'script', 'tie'], linked),
      note('e', 30, 21, ['e', 'stem', 'beam']),
      note('next', 60, 24, ['next', 'tie'], linked),
      note('neighbor', 50, 24),
      { ...note('tie', 32, 27), priority: 1 },
      note('script', 31, 29),
    ],
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const other = await context.newPage();
  await other.goto(`/scores?documentId=${firstId}`);
  const overlay = page.locator('.overlay').first();
  const click = async (x: number, y: number) => {
    const box = (await overlay.boundingBox())!;
    await overlay.click({ position: { x: (box.width * x) / 100, y: (box.height * y) / 140 } });
  };
  const ink = (id: string) => page.locator(`[data-ossia-graphic="${id}"] path`).first();
  await click(31, 25);
  await expect(page.locator('rect.element-selection')).toHaveCount(0);
  await expect(page.locator('.selection-box')).toHaveCount(0);
  for (const id of ['c', 'stem', 'beam', 'next', 'tie', 'script'])
    await expect(page.locator(`[data-ossia-graphic="${id}"]`).first()).toHaveClass(
      'element-selection',
    );
  await expect(ink('c')).toHaveCSS('fill', 'none');
  await expect(ink('c')).not.toHaveCSS('stroke', 'rgb(0, 0, 0)');
  await expect(ink('stem')).not.toHaveCSS('stroke', 'rgb(0, 0, 0)');
  await expect(ink('e')).toHaveCSS('fill', 'rgb(0, 0, 0)');
  await expect(ink('neighbor')).toHaveCSS('fill', 'rgb(0, 0, 0)');
  await expect(other.locator('[data-ossia-graphic="next"]').first()).toHaveClass(
    'element-selection',
  );
  expect(runtime.selections.get(firstId)?.targets).toEqual(linked);
  await click(31, 22);
  await click(61, 25);
  await expect(page.locator('[data-ossia-graphic="c"]').first()).not.toHaveClass(
    'element-selection',
  );
  await expect(ink('c')).toHaveCSS('stroke', 'rgb(0, 0, 0)');
  await expect(page.locator('[data-ossia-graphic="stem"]').first()).toHaveClass(
    'element-selection',
  );
  await click(31, 22);
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await expect(ink('stem')).toHaveAttribute('stroke', 'black');
  await expect.poll(() => ink('stem').getAttribute('style')).toBeNull();
  await expect(ink('beam')).toHaveCSS('fill', 'rgb(0, 0, 0)');
  await click(31, 25);
  await click(32, 30);
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await page.keyboard.down('Control');
  await click(31, 25);
  await page.keyboard.up('Control');
  await expect(page.locator('.element-selection')).toHaveCount(0);
});

test('inline SVG strips active content and namespaces every page reference', async ({
  context,
  page,
}) => {
  await mockScoreRuntime(context, {
    svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140" onload="document.body.dataset.injected='yes'">
    <script>document.body.dataset.injected='yes'</script>
    <style>body { display:none }</style>
    <foreignObject><div xmlns="http://www.w3.org/1999/xhtml">unsafe</div></foreignObject>
    <defs><path id="glyph" d="M10 10h1v1z"/><clipPath id="clip"><rect width="5" height="5"/></clipPath></defs>
    <a href="javascript:alert(1)"><path d="M20 20h2v2z"/></a>
    <use href="#glyph" clip-path="url(#clip)"/>
    <use href="https://example.com/unsafe.svg#glyph"/>
    <path d="M1 1h1v1z" style="fill:url(https://example.com/unsafe.svg);stroke:black"/>
    <text x="1" y="5"><tspan>中文  多行</tspan></text>
  </svg>`,
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  await expect(
    page.locator(
      '.score-image script,.score-image style,.score-image foreignObject,.score-image a,.score-image [onload]',
    ),
  ).toHaveCount(0);
  expect(await page.locator('body').getAttribute('data-injected')).toBeNull();
  const references = await page.locator('.score-image').evaluateAll((svgs) =>
    svgs.map((svg) => ({
      id: svg.querySelector('defs path')!.id,
      href: svg.querySelector('use')!.getAttribute('href'),
      clip: svg.querySelector('clipPath')!.id,
      clipReference: svg.querySelector('use')!.getAttribute('clip-path'),
      external: svg.querySelectorAll('use[href^="http"]').length,
    })),
  );
  expect(new Set(references.map((r) => r.id)).size).toBe(2);
  for (const ref of references) {
    expect(ref.href).toBe(`#${ref.id}`);
    expect(ref.clipReference).toBe(`url(#${ref.clip})`);
    expect(ref.external).toBe(0);
  }
  await expect(page.locator('.score-image tspan').first()).toHaveCSS('white-space', 'pre');
});

test('a rejected note group rolls back every colored part', async ({ context, page }) => {
  const linked = [target('c'), target('next'), target('tie')];
  await mockScoreRuntime(context, {
    svg,
    elements: [
      note('c', 30, 24, ['c', 'stem', 'beam', 'tie'], linked),
      note('next', 60, 24, ['next', 'tie'], linked),
      { ...note('tie', 32, 27), priority: 1 },
    ],
  });
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/selection', async (route) => {
    await held;
    await route.fulfill({
      status: 409,
      contentType: 'application/json',
      body: JSON.stringify({ error: { code: 'stale-selection', message: 'source changed' } }),
    });
  });
  await page.goto(`/scores?documentId=${firstId}`);
  await expect(page.locator('.score-image')).toHaveCount(2);
  const overlay = page.locator('.overlay').first();
  const box = (await overlay.boundingBox())!;
  try {
    await overlay.click({ position: { x: box.width * 0.31, y: (box.height * 25) / 140 } });
    await expect(page.locator('.element-selection')).toHaveCount(10);
  } finally {
    release();
  }
  await expect(page.locator('.element-selection')).toHaveCount(0);
  await expect(page.locator('[data-ossia-graphic="c"] path').first()).toHaveCSS(
    'stroke',
    'rgb(0, 0, 0)',
  );
});

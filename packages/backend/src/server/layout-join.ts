// Chain 2 measurement: is "column k of system s" the same thing as "measure m of a voice"?
//
// The server never stores measure numbers, so the join between L2's columns and L1's measure spans
// has to be *measured*, not assumed. `hitTest` returns the column index inside a system; L1 returns
// `[measureStart, measureEnd]` ranges whose authority is the `% N` comment (ADR-0002 §3.2). This
// module checks the assumption the locator is built on: that measure columns enumerate the score in
// order, page after page, so the global measure number is the running column index + 1.
//
// It is also what turns a clicked page-2 column into a global number, which is why the index is
// built once per version over every rendered page.
import { buildLayout, type LayoutIndex } from '@ossia/ly';

export interface PageLayout {
  page: number;
  layout: LayoutIndex;
}

/** One column per measure, in reading order across pages. `page`/`system` address the client's view. */
export interface MeasureColumnRef {
  /** 1-based global measure number, i.e. the running column index. */
  measure: number;
  page: number;
  system: number;
  staff: number;
  /** Column index inside that system (what `hitTest` yields). */
  column: number;
  x0: number;
  x1: number;
}

export interface MeasureGrid {
  columns: MeasureColumnRef[];
  /** First column index of each page, so a click's page + local column maps to one number. */
  pageStart: Record<number, number>;
  staffSpace: number;
}

/**
 * Flatten every page's L2 index into one grid. Columns are counted per system (the gap between two
 * barlines), staff-parallel columns collapse to one measure, and a page whose layout could not be
 * read contributes nothing rather than shifting every later measure by its column count.
 */
export function measureGrid(pages: PageLayout[]): MeasureGrid {
  const columns: MeasureColumnRef[] = [];
  const pageStart: Record<number, number> = {};
  let staffSpace = 0;
  for (const { page, layout } of pages) {
    staffSpace ||= layout.staffSpace;
    pageStart[page] = columns.length;
    const perSystem = layout.systems.map((s) => Math.max(0, s.barXs.length - 1));
    for (let si = 0; si < perSystem.length; si++) {
      for (let c = 0; c < perSystem[si]!; c++) {
        const staffCol = layout.columns.find((x) => x.system === si && x.measure === c + 1);
        columns.push({
          measure: columns.length + 1,
          page,
          system: si,
          staff: staffCol?.staff ?? 0,
          column: c + 1,
          x0: staffCol?.x0 ?? 0,
          x1: staffCol?.x1 ?? 0,
        });
      }
    }
  }
  return { columns, pageStart, staffSpace };
}

/**
 * Global measure number for a click. `local` is `hitTest`'s column index inside `system` of `page`.
 * Returns null when the page/system/column is outside the grid, which the caller reports as
 * "no hit" rather than guessing a measure.
 */
export function globalMeasure(
  grid: MeasureGrid,
  page: number,
  system: number,
  local: number,
): number | null {
  if (grid.pageStart[page] === undefined) return null;
  const hit = grid.columns.find(
    (c) => c.page === page && c.system === system && c.column === local,
  );
  return hit?.measure ?? null;
}

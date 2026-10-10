// L2 layout index (ADR-0002 §4): system × staff × measure-column geometry from a LilyPond SVG
// alone. LilyPond emits no id/class hooks, so every locator is derived from coordinates.
// Ported from `work/p1-5/{svg,measures}.mjs`, which is validated against L1 in ADR-0002 §4.3.
// Pure string/number work: no Node APIs, so the same module can run in a worker or the client.

const STREAM = new RegExp(
  [
    '<g\\b(?<g>[^>]*)>',
    '</g>',
    '<line\\b(?<line>[^>]*)/>',
    '<rect\\b(?<rect>[^>]*)/>',
    '<path\\b(?<path>[^>]*?)/?>',
    '<text\\b(?<text>[^>]*)>',
    '</text>',
    '<a\\b(?<a>[^>]*)>',
    '</a>',
  ].join('|'),
  'g',
);

const numOf = (s: string, k: string): number => {
  const i = s.indexOf(k + '=');
  if (i < 0) return NaN;
  const q = s.indexOf('"', i);
  return parseFloat(s.slice(q + 1, s.indexOf('"', q + 1)));
};
const strOf = (s: string, k: string): string | null => {
  const i = s.indexOf(k + '=');
  if (i < 0) return null;
  const q = s.indexOf('"', i);
  return s.slice(q + 1, s.indexOf('"', q + 1));
};

export interface Anchor {
  line: number;
  c1: number;
  c2: number;
  href: string;
}

export type SvgElement =
  | { kind: 'line'; x: number; y: number; x1: number; y1: number; anchor: Anchor | null }
  | {
      kind: 'rect';
      x: number;
      y: number;
      w: number;
      h: number;
      x1: number;
      y1: number;
      anchor: Anchor | null;
    }
  | { kind: 'path'; x: number; y: number; x1: number; y1: number; anchor: Anchor | null }
  | { kind: 'text'; x: number; y: number; anchor: Anchor | null };

/** Path bbox from `d`: LilyPond emits only M/L/C/Z; control points are included so the box
 *  over-approximates the curve, which is safe for hit testing. */
export function pathBBox(d: string) {
  const toks = d.match(/[MLCZmlcz]|-?\d*\.?\d+(?:[eE]-?\d+)?/g) || [];
  let x = 0,
    y = 0,
    sx = 0,
    sy = 0,
    rel = false;
  let cmd: string | null = null;
  const xs = [0],
    ys = [0],
    nums: number[] = [];
  const flush = () => {
    if (!cmd || !nums.length) {
      if (cmd === 'Z') {
        x = sx;
        y = sy;
      }
      nums.length = 0;
      return;
    }
    const at = (i: number, base: number) => nums[i]! + (rel ? base : 0);
    if (cmd === 'M' || cmd === 'L') {
      for (let i = 0; i + 1 < nums.length; i += 2) {
        x = at(i, x);
        y = at(i + 1, y);
        xs.push(x);
        ys.push(y);
      }
      if (cmd === 'M') {
        sx = x;
        sy = y;
      }
    } else if (cmd === 'C') {
      for (let i = 0; i + 5 < nums.length; i += 6) {
        for (const j of [0, 2, 4]) {
          xs.push(at(i + j, x));
          ys.push(at(i + j + 1, y));
        }
        x = at(i + 4, x);
        y = at(i + 5, y);
      }
    } else if (cmd === 'Z') {
      x = sx;
      y = sy;
    }
    nums.length = 0;
  };
  for (const t of toks) {
    if (/^[MLCZmlcz]$/.test(t)) {
      flush();
      cmd = t.toUpperCase();
      rel = t !== t.toUpperCase();
    } else nums.push(parseFloat(t));
  }
  flush();
  return { x0: Math.min(...xs), x1: Math.max(...xs), y0: Math.min(...ys), y1: Math.max(...ys) };
}

/** Flatten nested `<g transform="translate(x, y)">` into page-space coordinates and keep the
 *  point-and-click anchor (`textedit:///…ly:line:col1:col2`) each element belongs to. */
export function readSvg(src: string): SvgElement[] {
  const els: SvgElement[] = [];
  const st = [{ dx: 0, dy: 0 }];
  let anchor: Anchor | null = null;
  const re = new RegExp(STREAM.source, 'g');
  let m: RegExpExecArray | null;
  while ((m = re.exec(src))) {
    const tok = m[0]!,
      g = m.groups!;
    if (tok === '</g>') {
      if (st.length > 1) st.pop();
      continue;
    }
    if (tok === '</a>') {
      anchor = null;
      continue;
    }
    if (tok === '</text>') continue;
    if (tok.startsWith('<a')) {
      const href = strOf(g.a!, 'xlink:href') || strOf(g.a!, 'href') || '';
      const mm = /:(\d+):(\d+):(\d+)$/.exec(href);
      anchor = mm ? { line: +mm[1]!, c1: +mm[2]!, c2: +mm[3]!, href } : null;
      continue;
    }
    if (tok.startsWith('<g')) {
      const tr = /translate\(\s*([-\d.eE]+)[ ,]+([-\d.eE]+)\s*\)/.exec(
        strOf(g.g!, 'transform') || '',
      );
      const p = st[st.length - 1]!;
      st.push({ dx: p.dx + (tr ? +tr[1]! : 0), dy: p.dy + (tr ? +tr[2]! : 0) });
      continue;
    }
    const P = st[st.length - 1]!;
    if (tok.startsWith('<text')) {
      els.push({
        kind: 'text',
        x: P.dx + numOf(g.text!, 'x'),
        y: P.dy + numOf(g.text!, 'y'),
        anchor,
      });
    } else if (tok.startsWith('<line')) {
      const a = g.line!;
      els.push({
        kind: 'line',
        x: P.dx + numOf(a, 'x1'),
        y: P.dy + numOf(a, 'y1'),
        x1: P.dx + numOf(a, 'x2'),
        y1: P.dy + numOf(a, 'y2'),
        anchor,
      });
    } else if (tok.startsWith('<rect')) {
      const a = g.rect!;
      const x = P.dx + numOf(a, 'x'),
        y = P.dy + numOf(a, 'y');
      const w = numOf(a, 'width'),
        h = numOf(a, 'height');
      els.push({ kind: 'rect', x, y, w, h, x1: x + w, y1: y + h, anchor });
    } else if (tok.startsWith('<path')) {
      const d = strOf(g.path!, 'd');
      if (!d) continue;
      // Glyph em scale sits on the <path> itself (`scale(0.0040, -0.0040)`); without it, bboxes
      // are ~250x too large.
      const sc = /scale\(\s*([-\d.eE]+)(?:[ ,]+([-\d.eE]+))?\s*\)/.exec(
        strOf(g.path!, 'transform') || '',
      );
      const sx = sc ? +sc[1]! : 1;
      const sy = sc ? +(sc[2] === undefined ? sc[1] : sc[2])! : 1;
      const bb = pathBBox(d);
      const ax = P.dx + bb.x0 * sx,
        bx = P.dx + bb.x1 * sx;
      const ay = P.dy + bb.y0 * sy,
        by = P.dy + bb.y1 * sy;
      els.push({
        kind: 'path',
        x: Math.min(ax, bx),
        y: Math.min(ay, by),
        x1: Math.max(ax, bx),
        y1: Math.max(ay, by),
        anchor,
      });
    }
  }
  return els;
}

/** Narrow an element list to one kind. Preferred over `as` casts spread across lines, which ASI
 *  turns into stray statements (this file had that bug once already). */
export const onlyLines = (els: SvgElement[]) =>
  els.filter((e): e is Extract<SvgElement, { kind: 'line' }> => e.kind === 'line');
export const onlyRects = (els: SvgElement[]) =>
  els.filter((e): e is Extract<SvgElement, { kind: 'rect' }> => e.kind === 'rect');

export interface StaffGeom {
  top: number;
  bot: number;
  x0: number;
  x1: number;
}
export interface SystemGeom {
  staffs: StaffGeom[];
  top: number;
  bot: number;
}

// Thresholds are the empirical constants registered in ADR-0002 §4.2; they must be re-tuned on
// more instrument layouts rather than replaced with heuristics per score.
const THIN_MAX = 0.45; // a barline rect is at most this wide (pt)
const X_CLUSTER = 0.25; // segments this close in x belong to one barline
const COVERAGE = 0.85; // merged y coverage of the staff height required
const STAFF_RUN = 1.6; // adjacent lines further apart than this end a staff

/** Staff lines -> staves (exactly 5 lines) -> systems, grouped by the bimodal inter-staff gap
 *  rather than a magic threshold (ADR-0002 §4.2 rule 4). */
export function systems(els: SvgElement[]): { systems: SystemGeom[]; staffSpace: number } {
  const lines = onlyLines(els).filter((l) => Math.abs(l.y - l.y1) < 1e-6 && l.x1 - l.x > 5);
  if (!lines.length) return { systems: [], staffSpace: 0 };
  const ys = [...new Set(lines.map((l) => +l.y.toFixed(3)))].sort((a, b) => a - b);
  // Staff space = the modal gap between adjacent staff lines (5 lines => 4 gaps).
  const gaps = ys
    .slice(1)
    .map((v, i) => +(v - ys[i]!).toFixed(3))
    .filter((g) => g > 0);
  if (!gaps.length) return { systems: [], staffSpace: 0 };
  const staffSpace = [
    ...gaps.reduce((m, g) => m.set(g, (m.get(g) || 0) + 1), new Map<number, number>()),
  ].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0]![0];
  const staves: number[][] = [];
  let run = [ys[0]!];
  for (let k = 1; k < ys.length; k++) {
    if (ys[k]! - ys[k - 1]! < staffSpace * STAFF_RUN) run.push(ys[k]!);
    else {
      if (run.length === 5) staves.push(run);
      run = [ys[k]!];
    }
  }
  if (run.length === 5) staves.push(run);
  const geom: StaffGeom[] = staves.map((s) => {
    const top = Math.min(...s),
      bot = Math.max(...s);
    const ls = lines.filter((l) => Math.abs(l.y - top) < 1e-3 || Math.abs(l.y - bot) < 1e-3);
    return { top, bot, x0: Math.min(...ls.map((l) => l.x)), x1: Math.max(...ls.map((l) => l.x1)) };
  });
  if (geom.length < 2) {
    return {
      systems: geom.length ? [{ staffs: geom, top: geom[0]!.top, bot: geom[0]!.bot }] : [],
      staffSpace,
    };
  }
  // Bimodal split: distance from one staff's bottom line to the next staff's top line is small
  // inside a system and large between systems. Cut at the biggest jump in the distinct values.
  const d = geom.slice(1).map((g, i) => +(g.top - geom[i]!.bot).toFixed(3));
  const uniq = [...new Set(d)].sort((a, b) => a - b);
  const groups: StaffGeom[][] = [[]];
  if (uniq.length < 2) groups[0]!.push(...geom);
  else {
    let bi = 0,
      bj = -1;
    for (let k = 1; k < uniq.length; k++) {
      const jump = uniq[k]! - uniq[k - 1]!;
      if (jump > bj) {
        bj = jump;
        bi = k - 1;
      }
    }
    const cut = (uniq[bi]! + uniq[bi + 1]!) / 2;
    groups[0]!.push(geom[0]!);
    for (let k = 1; k < geom.length; k++) {
      if (d[k - 1]! > cut) groups.push([]);
      groups[groups.length - 1]!.push(geom[k]!);
    }
  }
  return {
    staffSpace,
    systems: groups
      .filter((g) => g.length)
      .map((staffs) => ({
        staffs,
        top: Math.min(...staffs.map((s) => s.top)),
        bot: Math.max(...staffs.map((s) => s.bot)),
      })),
  };
}

interface Interval {
  a: number;
  b: number;
}

// Merged vertical coverage of one x-cluster, clamped to the staff. A system-spanning barline is
// drawn either as one tall rect that overshoots every staff or as several stacked segments that
// bridge the inter-staff gap; clamping handles both (ADR-0002 §4.2).
const covers = (
  segs: { y: number; y1: number }[],
  top: number,
  bot: number,
  tol: number,
): number => {
  const iv = segs
    .map((s) => ({ a: Math.max(s.y, top - tol), b: Math.min(s.y1, bot + tol) }))
    .filter((x) => x.b > x.a)
    .sort((x, y) => x.a - y.a);
  if (!iv.length) return 0;
  const merged: Interval[] = [{ ...iv[0]! }];
  for (const x of iv.slice(1)) {
    const last = merged[merged.length - 1]!;
    if (x.a <= last.b + tol) last.b = Math.max(last.b, x.b);
    else merged.push({ ...x });
  }
  return merged.reduce((acc, x) => acc + (x.b - x.a), 0);
};

export interface Barlines {
  xs: number[];
  left: number;
  right: number;
}

/** Barline x positions for one system: thin rects clustered in x whose merged vertical coverage
 *  reaches COVERAGE of every staff in the system. Handles both a system-spanning barline drawn as
 *  several stacked segments and per-staff barlines (`allow-span-bar = ##f`). */
export function systemBarlines(els: SvgElement[], sys: SystemGeom): Barlines {
  const left = Math.min(...sys.staffs.map((s) => s.x0)),
    right = Math.max(...sys.staffs.map((s) => s.x1));
  const thin = onlyRects(els).filter((r) => r.x1 - r.x <= THIN_MAX);
  const cand = thin.filter((r) => r.x >= left - 1 && r.x <= right + 1);
  const clusters: { x: number; segs: { y: number; y1: number }[] }[] = [];
  for (const r of cand.slice().sort((a, b) => a.x - b.x)) {
    const c = clusters.find((k) => Math.abs(k.x - r.x) <= X_CLUSTER);
    if (c) {
      c.segs.push(r);
      c.x = (c.x * (c.segs.length - 1) + r.x) / c.segs.length;
    } else clusters.push({ x: r.x, segs: [r] });
  }
  const xs = clusters
    .filter((c) =>
      sys.staffs.every((st) => covers(c.segs, st.top, st.bot, 0.5) >= (st.bot - st.top) * COVERAGE),
    )
    .map((c) => +c.x.toFixed(2))
    .sort((a, b) => a - b);
  return { xs, left, right };
}

export interface MeasureColumn {
  system: number;
  staff: number;
  measure: number;
  x0: number;
  x1: number;
}

export interface LayoutIndex {
  staffSpace: number;
  /** Per system: barline x positions; measures are the gaps between consecutive barlines. */
  systems: { top: number; bot: number; barXs: number[]; staffs: StaffGeom[] }[];
  columns: MeasureColumn[];
  pageCount: number;
}

/** Build the full L2 index for one page of SVG text. */
export function buildLayout(svg: string): LayoutIndex {
  const els = readSvg(svg);
  const { systems: sys, staffSpace } = systems(els);
  const out: LayoutIndex['systems'] = [];
  const columns: MeasureColumn[] = [];
  for (let i = 0; i < sys.length; i++) {
    const s = sys[i]!;
    const { xs } = systemBarlines(els, s);
    out.push({ top: +s.top.toFixed(2), bot: +s.bot.toFixed(2), barXs: xs, staffs: s.staffs });
    for (let k = 0; k + 1 < xs.length; k++) {
      const measure = k + 1;
      s.staffs.forEach((_, staff) =>
        columns.push({
          system: i,
          staff,
          measure,
          x0: xs[k]!,
          x1: xs[k + 1]!,
        }),
      );
    }
  }
  return { staffSpace, systems: out, columns, pageCount: 1 };
}

export interface Hit {
  system: number;
  staff: number;
  measure: number;
}

/** (x, y) in page points -> system / staff / measure column. */
export function hitTest(layout: LayoutIndex, x: number, y: number): Hit | null {
  for (let si = 0; si < layout.systems.length; si++) {
    const sys = layout.systems[si]!;
    if (y < sys.top - layout.staffSpace * 2 || y > sys.bot + layout.staffSpace * 2) continue;
    let staff = -1;
    for (let k = 0; k < sys.staffs.length; k++) {
      const st = sys.staffs[k]!;
      if (y >= st.top - layout.staffSpace * 1.2 && y <= st.bot + layout.staffSpace * 1.2) {
        staff = k;
        break;
      }
    }
    if (staff < 0) {
      // Between two staves: attribute to the nearer one, which is what a player means.
      staff = sys.staffs.findIndex(
        (_, k) => k > 0 && y < (sys.staffs[k]!.top + sys.staffs[k - 1]!.bot) / 2,
      );
      if (staff < 0) staff = sys.staffs.length - 1;
    }
    const barXs = sys.barXs;
    let measure = -1;
    for (let k = 0; k + 1 < barXs.length; k++) {
      if (x >= barXs[k]! - 0.5 && x <= barXs[k + 1]! + 0.5) {
        measure = k + 1;
        break;
      }
    }
    if (measure < 0) {
      if (x < barXs[0]! - 0.5) measure = 1;
      else if (barXs.length && x > barXs[barXs.length - 1]!)
        measure = Math.max(1, barXs.length - 1);
      else return null;
    }
    return { system: si, staff, measure };
  }
  return null;
}

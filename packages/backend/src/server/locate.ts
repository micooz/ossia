// Service C — locating: L1 semantic index × L2 layout columns × the score block that says
// which voice is engraved on which staff.
//
// The client only ever exchanges locators (ADR-0001 §6.2 / G1): it sends a page-space point and
// gets `{ doc, version, staff, measure, voice }`; it never sees the source. Measure authority
// stays with L1's `% N` comments (ADR-0002 §3.2), L2 only supplies the column geometry.
import { buildIndex } from '@ossia/ly';
import type { Measure, VoiceRegion } from '@ossia/ly';
import { buildLayout, hitTest } from '@ossia/ly';
import type { LayoutIndex } from '@ossia/ly';
import { tokenize, TT, type Token } from '@ossia/ly';

import { globalMeasure, measureGrid, type MeasureGrid } from './layout-join.ts';

export interface StaffVoices {
  staff: number;
  voices: { voice: string; name: string }[];
}

/**
 * Read the engraving order out of the score block. Both importers have to be understood, because
 * they write different shapes and each shape decides which voice sits on which staff:
 *
 *   musicxml2ly  \score { << \new Staff = "P1" << \context Staff <<
 *                \context Voice = "PartPOneVoiceOne" { \voiceOne \PartPOneVoiceOne } >> >> >> }
 *   midi2ly      \score { << \context Staff=trackB \trackA \context Staff=trackB \trackB >> }
 *                with trackB = << \context Voice = voiceA \trackBchannelA >> declared above it
 *
 * The staff count is also the visual staff count per system, which L2 cannot tell us (it only sees
 * line groups) - so this table is the join between geometry and voices. Without it the client can
 * click a measure and be told its locator, then have the edit refused because no voice is known
 * for that staff.
 */
export function parseStaves(src: string): StaffVoices[] {
  // Whitespace/comment tokens would sit between the keyword, the context type, `=` and the label,
  // so the pattern is read from a meaningful-token stream.
  const toks = meaningful(src);
  const groups = topLevelGroups(toks);
  const staves: StaffVoices[] = [];
  /** midi2ly hands two groups to the same staff, so the label is the staff's identity. */
  const byLabel = new Map<string, StaffVoices>();
  let cur: StaffVoices | null = null;
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i]!;
    if (t.t !== TT.CMD || (t.raw !== '\\context' && t.raw !== '\\new')) continue;
    const what = toks[i + 1];
    if (!what || what.t !== TT.IDENT) continue;

    if (what.raw === 'Staff') {
      const label = labelled(toks, i);
      if (label?.t === TT.STR) {
        // `\new Staff = "x"` / `\context Staff = "1"`: a staff of its own.
        cur = { staff: staves.length, voices: [] };
        staves.push(cur);
      } else if (label?.t === TT.IDENT) {
        // `\context Staff=trackB \trackA`: a context *name*, so the same label twice is the same
        // staff (measured: two such lines in one score block render as one staff), and the voices
        // live in the group named by the music that follows.
        let s = byLabel.get(label.raw);
        if (!s) {
          s = { staff: staves.length, voices: [] };
          staves.push(s);
          byLabel.set(label.raw, s);
        }
        cur = s;
        for (const ref of groupRefs(toks, i + 4)) {
          for (const v of groups.get(ref) ?? []) {
            if (!s.voices.some((x) => x.name === v.name)) s.voices.push(v);
          }
        }
      } else if (!cur || cur.voices.length) {
        // An unlabelled `\context Staff` opens a fresh staff unless it merely renames the one we
        // are inside (musicxml2ly writes `\new Staff = "x" << \context Staff << … >> >>`).
        cur = { staff: staves.length, voices: [] };
        staves.push(cur);
      }
      continue;
    }
    if (what.raw !== 'Voice' || !cur) continue;
    const entry = voiceEntry(toks, i);
    if (!cur.voices.some((x) => x.name === entry.name)) cur.voices.push(entry);
  }
  return staves;
}

/** Tokens that matter for the context scan: the lexer keeps whitespace and comments between the
    keyword, the context type, `=` and the label. */
const meaningful = (src: string): Token[] =>
  tokenize(src).filter((t) => t.t !== TT.WS && t.t !== TT.COMMENT);

/** The label of a `\context|\new <type> = <label>` at `i`, or undefined when it carries none. */
function labelled(toks: Token[], i: number): Token | undefined {
  return toks[i + 2]?.raw === '=' ? toks[i + 3] : undefined;
}

/** Top-level `name = << … >>` containers and the voices they declare (one per MIDI channel). */
function topLevelGroups(toks: Token[]): Map<string, { voice: string; name: string }[]> {
  const out = new Map<string, { voice: string; name: string }[]>();
  for (let i = 0; i + 2 < toks.length; i++) {
    const head = toks[i]!;
    if (head.t !== TT.IDENT || toks[i + 1]?.raw !== '=' || toks[i + 2]?.raw !== '<<') continue;
    const voices: { voice: string; name: string }[] = [];
    let depth = 0;
    let j = i + 2;
    for (; j < toks.length; j++) {
      const x = toks[j]!;
      if (x.t === TT.SEQ) {
        if (x.raw === '<<') depth++;
        else if (--depth <= 0) break;
        continue;
      }
      if (
        x.t === TT.CMD &&
        (x.raw === '\\context' || x.raw === '\\new') &&
        toks[j + 1]?.raw === 'Voice'
      ) {
        voices.push(voiceEntry(toks, j));
      }
    }
    out.set(head.raw, voices);
    i = j;
  }
  return out;
}

/** The music variables handed to a `\context Staff=ident` body, stopping at the next context. */
function groupRefs(toks: Token[], from: number): string[] {
  const out: string[] = [];
  let depth = 0;
  for (let k = from; k < toks.length; k++) {
    const x = toks[k]!;
    if (x.t === TT.SEQ || x.t === TT.BRACE) {
      if (x.raw === '<<' || x.raw === '{') depth++;
      else if (depth === 0) break;
      else depth--;
      continue;
    }
    if (x.t !== TT.CMD) continue;
    if ((x.raw === '\\context' || x.raw === '\\new') && depth === 0) break;
    if (depth === 0) out.push(x.raw.slice(1));
  }
  return out;
}

/** A `\context Voice = <label> <reference>` declaration starting at `i`. */
function voiceEntry(toks: Token[], i: number): { voice: string; name: string } {
  const label = labelled(toks, i);
  const value = label ? (label.t === TT.STR ? label.raw.replace(/^"|"$/g, '') : label.raw) : '';
  return {
    voice: value,
    name: referencedVoice(toks, label ? i + 4 : i + 2, value, label?.t === TT.IDENT),
  };
}

/**
 * The music variable a `\context Voice` block actually plays. musicxml2ly writes
 * `{ \voiceOne \PartPOneVoiceOne }`, so the reference is a *command* token whose name starts with
 * the quoted context value; midi2ly writes `\context Voice = voiceA \trackBchannelB`, where the
 * unquoted label is only a context name and the following command is the region L1 indexes.
 * `\voiceOne`/`\voiceTwo`/`\oneVoice` are directives, not references. Falls back to the context
 * value when the block holds inline music (then L1's region name equals it anyway).
 */
function referencedVoice(toks: Token[], from: number, value: string, unquoted = false): string {
  const open = toks[from]?.t === TT.BRACE ? from : -1;
  const stop = open < 0 ? Math.min(toks.length, from + 24) : toks.length;
  let depth = 0;
  for (let k = open < 0 ? from : open + 1; k < stop; k++) {
    const x = toks[k]!;
    if (x.t === TT.BRACE) {
      if (x.raw === '{') depth++;
      else if (--depth <= 0) break;
      continue;
    }
    if (x.t !== TT.CMD) continue;
    const name = x.raw.slice(1);
    if (/^(voice|oneVoice|partial|repeatTie|noBeam|stem|tie|slur)/i.test(name)) continue;
    if (unquoted || name.startsWith(value)) return name;
  }
  return value;
}

export interface LocatorTable {
  staffs: StaffVoices[];
  voices: VoiceRegion[];
  /** Measure ranges per voice, keyed by voice name. */
  measures: Record<string, Measure[]>;
  /** L2 index of every rendered page, keyed by 1-based page number. */
  pages: { page: number; layout: LayoutIndex }[];
  /** Page + system + column -> 1-based global measure number (measured, see layout-join.ts). */
  grid: MeasureGrid;
}

export interface Located {
  /** Which rendered page the hit came from, so the client can re-draw the selection there. */
  page: number;
  staff: number;
  voice: string | null;
  /** Never null: a locator without a measure is reported as "no hit" instead. */
  measure: number;
  system: number;
  lineStart: number | null;
  lineEnd: number | null;
}

/**
 * Build the join table for one rendered version. Cheap: L1 ~2 ms, L2 ~6 ms per page (ADR-0002 §4.3).
 * Every page is indexed because the client clicks on whichever page it is looking at, and a
 * page-1-only index would silently mis-attribute every click after the first page.
 */
export function locatorTable(ly: string, svgPages: string[]): LocatorTable {
  const idx = buildIndex(ly);
  const pages = svgPages.map((svg, i) => ({ page: i + 1, layout: buildLayout(svg) }));
  return {
    staffs: parseStaves(ly),
    voices: idx.voices,
    measures: Object.fromEntries(idx.voices.map((v) => [v.name, v.measures])),
    pages,
    grid: measureGrid(pages),
  };
}

/** The voice whose measure list contains `measure`, preferring the staff it is engraved on. */
export function voicesForMeasure(t: LocatorTable, staff: number, measure: number): string[] {
  const staffVoices = t.staffs[staff]?.voices ?? [];
  const candidates = staffVoices.length
    ? staffVoices.map((v) => v.name).filter((n) => t.measures[n])
    : Object.keys(t.measures);
  return candidates.filter((n) =>
    t.measures[n]!.some((m) => measure >= m.measureStart && measure <= m.measureEnd),
  );
}

/**
 * Page-space point -> locator. L2 gives (system, staff, column); the grid turns that column into
 * the global measure number, and L1 confirms the measure exists for a voice on that staff and
 * yields the source line range the Agent may edit.
 */
export function locatePoint(t: LocatorTable, page: number, x: number, y: number): Located | null {
  const p = t.pages.find((q) => q.page === page) ?? t.pages[0];
  if (!p) return null;
  const hit = hitTest(p.layout, x, y);
  if (!hit) return null;
  const measure = globalMeasure(t.grid, p.page, hit.system, hit.measure);
  if (measure == null) return null;
  const names = voicesForMeasure(t, hit.staff, measure);
  const name = names[0] ?? null;
  const m = name
    ? t.measures[name]!.find((mm) => measure >= mm.measureStart && measure <= mm.measureEnd)
    : null;
  return {
    page: p.page,
    staff: hit.staff,
    voice: name,
    measure,
    system: hit.system,
    lineStart: m?.lineStart ?? null,
    lineEnd: m?.lineEnd ?? null,
  };
}

/** Locator as the client expresses it (a measure range of a staff), resolved to line ranges. */
export function resolveLocator(
  t: LocatorTable,
  l: {
    staff?: number;
    voice?: string;
    measureStart: number;
    measureEnd?: number;
  },
): {
  tag: string;
  lineStart: number;
  lineEnd: number;
  voice: string;
  measureStart: number;
  measureEnd: number;
}[] {
  const names = l.voice
    ? [l.voice]
    : l.staff != null
      ? (t.staffs[l.staff]?.voices.map((v) => v.name) ?? [])
      : Object.keys(t.measures);
  const out: ReturnType<typeof resolveLocator> = [];
  for (const name of names) {
    const ms = t.measures[name];
    if (!ms?.length) continue;
    const end = l.measureEnd ?? l.measureStart;
    const picked = ms.filter((m) => m.measureEnd >= l.measureStart && m.measureStart <= end);
    if (!picked.length) continue;
    const lineStart = picked[0]!.lineStart;
    const lineEnd = picked[picked.length - 1]!.lineEnd;
    out.push({
      tag: `${name} m${picked.length === 1 ? l.measureStart : `${l.measureStart}-${end}`}`,
      lineStart,
      lineEnd,
      voice: name,
      measureStart: picked[0]!.measureStart,
      measureEnd: picked[picked.length - 1]!.measureEnd,
    });
  }
  return out;
}

export { buildIndex };

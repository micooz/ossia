// L1 semantic index (ADR-0002 §3): `.ly` -> measure/staff/voice source ranges.
// Ported from `work/ly/index.mjs`.
import { tokenize, TT, posOf } from './lexer.ts';
import type { Token } from './lexer.ts';

export interface Measure {
  measureStart: number;
  measureEnd: number;
  measure: number;
  from: number;
  to: number;
  lineStart: number;
  lineEnd: number;
}

export interface VoiceRegion {
  name: string;
  /** Character span of the whole voice region, so pitch facts and edits can slice the source. */
  from: number;
  to: number;
  measures: Measure[];
  events: number;
  lineStart: number;
  lineEnd: number;
}

export interface LyIndex {
  toks: Token[];
  pos: (off: number) => { line: number; col: number };
  voices: VoiceRegion[];
  totalBars: number;
  noteEvents: number;
  maxMeasures: number;
}

export const nextSig = (toks: Token[], j: number): Token | undefined => {
  while (j < toks.length && (toks[j]!.t === TT.WS || toks[j]!.t === TT.COMMENT)) j++;
  return toks[j];
};

interface RegionStart {
  name: string;
  at: number;
}

// Voice regions: `Name = \relative … { … }`, `Name = \chordmode { … }`. musicxml2ly emits
// exactly one top-level assignment per part/voice, so a region runs from its variable name
// to the next assignment (or end of source).
function voiceRegions(toks: Token[], src: string): RegionStart[] {
  const starts: RegionStart[] = [];
  for (let i = 0; i + 2 < toks.length; i++) {
    if (toks[i]!.t !== TT.IDENT) continue;
    const eq = nextSig(toks, i + 1);
    if (!eq || eq.t !== TT.PUNCT || eq.raw !== '=') continue;
    const rhs = nextSig(toks, toks.indexOf(eq) + 1);
    if (!rhs || rhs.t !== TT.CMD) continue;
    if (!/^(?:\\relative|\\chordmode|\\transpose)/.test(rhs.raw)) continue;
    starts.push({ name: toks[i]!.raw, at: toks[i]!.s });
  }
  return starts.map((s, i) => ({ name: s.name, at: s.at }));
}

export function buildIndex(src: string): LyIndex {
  const toks = tokenize(src);
  const pos = posOf(src);
  const starts = voiceRegions(toks, src);
  const voices: VoiceRegion[] = [];
  for (let k = 0; k < starts.length; k++) {
    const r = starts[k]!;
    // A region runs to the next top-level assignment, or to the end of the source.
    const to = k + 1 < starts.length ? starts[k + 1]!.at : src.length;
    // `\`|` separates measures in every region kind, including \chordmode (whose contents are
    // text-mode), so bar splitting ignores mode; only event counting is music-only.
    const seg = toks.filter((t) => t.s >= r.at && t.s < to);
    // musicxml2ly terminates every measure with `% N`, the authoritative measure number, which
    // trails the closing `|`. Counting `|` alone is wrong when the source compresses empty
    // spans into a multimeasure rest (`s1*20` covers 20 measures in one bar), so each span
    // carries [measureStart, measureEnd] (§3.2).
    let mStart = seg.length ? seg[0]!.s : r.at,
      events = 0;
    const measures: Measure[] = [];
    for (const t of seg) {
      if ((t.t === TT.NOTE || t.t === TT.REST || t.t === TT.SKIP) && t.mode === 'music') events++;
      if (t.t === TT.COMMENT) {
        const mm = /^%\s*(\d+)\s*$/.exec(t.raw);
        if (mm && measures.length) measures[measures.length - 1]!.measureEnd = +mm[1]!;
        continue;
      }
      if (t.t !== TT.BAR) continue;
      const end = t.s + 1;
      measures.push({
        measureEnd: 0,
        measureStart: 0,
        measure: 0,
        from: mStart,
        to: end,
        lineStart: pos(mStart).line,
        lineEnd: pos(end).line,
      });
      mStart = end;
    }
    if (mStart < to)
      measures.push({
        measureEnd: 0,
        measureStart: 0,
        measure: 0,
        from: mStart,
        to,
        lineStart: pos(mStart).line,
        lineEnd: pos(to - 1).line,
      });
    let prevEnd = 0;
    for (const m of measures) {
      m.measureStart = prevEnd + 1;
      if (!m.measureEnd || m.measureEnd < m.measureStart) m.measureEnd = m.measureStart;
      m.measure = m.measureEnd;
      prevEnd = m.measureEnd;
    }
    voices.push({
      name: r.name,
      from: r.at,
      to,
      measures,
      events,
      lineStart: pos(r.at).line,
      lineEnd: pos(Math.max(r.at, to - 1)).line,
    });
  }
  return {
    toks,
    pos,
    voices,
    totalBars: toks.filter((t) => t.mode === 'music' && t.t === TT.BAR).length,
    noteEvents: toks.filter(
      (t) => (t.t === TT.NOTE || t.t === TT.REST || t.t === TT.SKIP) && t.mode === 'music',
    ).length,
    maxMeasures: Math.max(0, ...voices.map((v) => v.measures.length)),
  };
}

// Pitch state machine for `\relative` regions (ADR-OPEN-9 / ADR-0002 §6.3).
//
// Why: inside `\relative`, a lone octave marking redefines the reference pitch and silently
// transposes every later note of that voice (ADR-0002 §6.1: 341 notes moved for a one-note
// request). LilyPond reports nothing, so the Agent needs absolute pitch facts to project, and the
// semantic gate (G-e) needs them to judge intent vs. sounding result.
//
// These rules were measured against the vendored LilyPond 2.26.0: random `\relative` bodies
// compiled, and the produced MIDI diffed. They are the contract the resolver below implements.
//   R1 reference is a diatonic index di = 7*octaveMarks + step; written `c` is di 0 (MIDI 48).
//   R2 a note lands on the unique diatonic index within [-3, +3] steps of the reference
//      (a fourth up or a fourth down), *then* its own octave marks are added.
//   R3 the reference advances to each resolved note, including inside chords (each member is
//      relative to the previous one) and inside `\times`/`\repeat`/nested `{}` groups.
//   R4 after a chord the reference is the FIRST chord member, not the last (measured: `<g e c>4
//      a4` -> a lands a second above g; `<c e g>4 a4` -> a lands a fourth below g).
//   R5 accidentals change the sounding pitch only; they never move the diatonic reference.
//   R6 `r`/`R`/`s` (rest, multimeasure rest, skip) leave the reference untouched.
//   R7 `\key` and `\ottava` do not affect the MIDI the engine emits, so they are ignored here.
//   R8 inside `\absolute { … }` a note's octave comes from its own written marks.
//   R9 two chord members that sound the same pitch (`<ceses bes …>` are both Bb) produce ONE
//      MIDI note: the engine merges unisons inside a chord (measured on 2.26.0).
//   R10 a written pitch outside MIDI 0..127 is clamped by the engine (observed note number 127
//      and 252), so such events are flagged `outOfRange` and are not predictable from here.
// Out of the verified subset (`\transpose`, `\chordmode`, non-music mode notes) the resolver
// reports `unsupported` instead of guessing, so callers can fall back to MIDI-based gates.
import { tokenize, TT, posOf, nextSig } from './lexer.ts';
import type { Token } from './lexer.ts';

export const STEP_SEMITONE = [0, 2, 4, 5, 7, 9, 11] as const;
// Letter -> diatonic step. `h` is B natural in the German spelling musicxml2ly may emit.
const LETTER_STEP: Record<string, number> = { c: 0, d: 1, e: 2, f: 3, g: 4, a: 5, b: 6, h: 6 };
const ALTER: Record<string, number> = {
  is: 1,
  isis: 2,
  s: 1,
  ss: 2,
  x: 2,
  es: -1,
  eses: -2,
  f: -1,
  ff: -2,
};

const PITCH_RE =
  /^([a-h])(isis|eses|ses|is|es|ss|ff|s|f)?((?:,,|'')*(?:,|')*)(!?)((?:\d+(?:\.\d*)?\.{0,2}))?$/;

export interface PitchParse {
  step: number;
  alter: number;
  /** Octave displacement in octaves: `'` = +1, `,` = -1, `''` = +2, `,,` = -2. */
  marks: number;
  cancel: boolean;
  duration: string;
}

export function parsePitch(raw: string): PitchParse | null {
  const m = PITCH_RE.exec(raw);
  if (!m) return null;
  const letter = m[1]!,
    acc = m[2] ?? '',
    marksRaw = m[3] ?? '',
    cancel = m[4] === '!';
  let marks = 0;
  for (const k of marksRaw.match(/,,|''|,|'/g) ?? []) {
    marks += k === ',' ? -1 : k === ',,' ? -2 : k === "'" ? 1 : 2;
  }
  // `ses` is a sesquialteral flat LilyPond rejects in our spelling; treat it as unsupported.
  if (acc === 'ses') return null;
  return {
    step: LETTER_STEP[letter]!,
    alter: acc in ALTER ? ALTER[acc]! : 0,
    marks,
    cancel,
    duration: m[5] ?? '',
  };
}

/** Diatonic index of a written pitch, ignoring relative placement (R1). */
export const diOf = (step: number, marks: number) => 7 * marks + step;

/** MIDI number for a diatonic index plus an accidental (R1, R5). Written `c` = 48. */
export function midiOf(di: number, alter: number): number {
  const octave = Math.floor(di / 7);
  const step = di - 7 * octave;
  return 48 + 12 * octave + STEP_SEMITONE[step]! + alter;
}

/** R2: the unique diatonic step offset in [-3, +3] that reaches `step`. */
export function relativeDelta(refDi: number, step: number): number {
  for (let d = -3; d <= 3; d++) if ((((refDi + d) % 7) + 7) % 7 === step) return d;
  throw new Error(`relativeDelta: no candidate for refDi=${refDi} step=${step}`);
}

export interface PitchMember {
  raw: string;
  midi: number;
  di: number;
  alter: number;
}

export interface PitchEvent {
  kind: 'note' | 'chord' | 'rest' | 'skip';
  raw: string;
  /** Lowest sounding member of a chord; `-1` for rests and skips. */
  midi: number;
  /**
   * Sounding pitches of this event, de-duplicated (R9) and ascending — i.e. exactly what the
   * engine writes into the MIDI file. Gates compare against this rather than `members`.
   */
  pitches: number[];
  members?: PitchMember[];
  /** Reference pitch after this event (R3/R4), i.e. the di this event leaves behind. */
  refDi: number;
  /** A member's written pitch lies outside MIDI 0..127 (R10). */
  outOfRange?: boolean;
  /** Character offset of the first token of this event. */
  s: number;
  line: number;
}

export interface PitchResult {
  events: PitchEvent[];
  /** Diatonic reference at the end of the region, for chaining or debugging. */
  endRefDi: number | null;
  /** Non-null when the region contains something outside the verified subset. */
  unsupported: string | null;
  skipped: { reason: string; raw: string; line: number }[];
}

const inRange = (m: number) => m >= 0 && m <= 127;
const pitchSet = (xs: number[]) => [...new Set(xs)].sort((a, b) => a - b);

// Commands that move the reference in ways the verified rules do not cover.
const UNSUPPORTED_CMDS = new Set(['transpose', 'chordmode', 'repeat']);

/**
 * Resolve every note event in one voice region to an absolute MIDI pitch.
 * `from`/`to` are character offsets covering `Name = \relative … { … }`.
 */
export function resolveRegion(src: string, from: number, to: number): PitchResult {
  const toks = tokenize(src).filter((t) => t.s >= from && t.s < to);
  const pos = posOf(src);
  const line = (t: Token) => pos(t.s).line;
  const events: PitchEvent[] = [];
  const skipped: PitchResult['skipped'] = [];
  let unsupported: string | null = null;

  const relIdx = toks.findIndex((t) => t.t === TT.CMD && t.raw === '\\relative');
  if (relIdx < 0)
    return { events, endRefDi: null, unsupported: 'not a \\relative region', skipped };
  // `\relative` carries an argument exemption (§2.4), so the start pitch lexes as IDENT
  // (`c''`) rather than NOTE, and whitespace sits between. The start pitch is only the
  // initial reference: it is not itself a sounding event (verified: `\relative c'' { c4 }`
  // yields one note, C4).
  let k = relIdx + 1;
  while (k < toks.length && (toks[k]!.t === TT.WS || toks[k]!.t === TT.COMMENT)) k++;
  const startTok = toks[k];
  const start =
    startTok && (startTok.t === TT.NOTE || startTok.t === TT.IDENT)
      ? parsePitch(startTok.raw)
      : null;
  if (!startTok || !start)
    return { events, endRefDi: null, unsupported: 'no \\relative start pitch', skipped };

  let refDi = diOf(start.step, start.marks);
  let inChord = 0;
  let members: PitchMember[] = [];
  let chordS = 0;
  // `\absolute { … }`: inside, octaves come from the written marks; on close the reference is
  // restored to what it was before the group (measured: `\relative c'' { c4 \absolute { c4 c'4 }
  // c4 }` -> C5 C3 C4 C5, so the absolute group does not leak its reference outwards).
  // A stack is required because the Agent's `\absolute { … }` wrapper can itself sit inside a
  // `\relative { … }` or another `\absolute { … }`; closing the inner group must leave the
  // outer one active (found by that differential harness on seed 101).
  const absolute: { depth: number; refDi: number }[] = [];
  const isAbsolute = () => absolute.length > 0;
  let depth = 0;

  for (const t of toks) {
    if (t.t === TT.CMD) {
      const cmd = t.raw.slice(1);
      if (cmd === 'relative') continue;
      if (cmd === 'absolute') {
        absolute.push({ depth: depth + 1, refDi });
        continue;
      }
      if (UNSUPPORTED_CMDS.has(cmd))
        unsupported = unsupported ?? `\\${cmd} outside the verified subset`;
      continue;
    }
    if (t.t === TT.BRACE) {
      if (t.raw === '{') depth++;
      else {
        const top = absolute[absolute.length - 1];
        if (top && depth === top.depth) {
          refDi = absolute.pop()!.refDi;
        }
        depth--;
      }
      continue;
    }
    if (t.t === TT.CHORD_O) {
      inChord++;
      members = [];
      chordS = t.s;
      continue;
    }
    if (t.t === TT.CHORD_C) {
      if (inChord > 0) inChord--;
      if (inChord === 0 && members.length) {
        const lowest = members.reduce((a, b) => (b.midi < a.midi ? b : a), members[0]!);
        const ps = pitchSet(members.map((m) => m.midi));
        // R4: the reference after the chord is the first member's di.
        events.push({
          kind: 'chord',
          raw: `<${members.map((m) => m.raw).join(' ')}>`,
          midi: lowest.midi,
          pitches: ps,
          members: members.slice(),
          refDi: members[0]!.di,
          outOfRange: members.some((m) => !inRange(m.midi)),
          s: chordS,
          line: line(t),
        });
        refDi = members[0]!.di;
        members = [];
      }
      continue;
    }
    if (t.t === TT.REST || t.t === TT.SKIP) {
      // R6: rests and skips never move the reference.
      events.push({
        kind: t.t === TT.REST ? 'rest' : 'skip',
        raw: t.raw,
        midi: -1,
        pitches: [],
        refDi,
        s: t.s,
        line: line(t),
      });
      continue;
    }
    if (t.t !== TT.NOTE || t.mode !== 'music') continue;

    const p = parsePitch(t.raw);
    if (!p) {
      skipped.push({ reason: `unparsable pitch: ${t.raw}`, raw: t.raw, line: line(t) });
      continue;
    }
    const di = isAbsolute()
      ? diOf(p.step, p.marks) // R8
      : refDi + relativeDelta(refDi, p.step) + 7 * p.marks; // R2
    const midi = midiOf(di, p.alter);
    refDi = di; // R3
    if (inChord > 0) members.push({ raw: t.raw, midi, di, alter: p.alter });
    else
      events.push({
        kind: 'note',
        raw: t.raw,
        midi,
        pitches: [midi],
        refDi: di,
        outOfRange: !inRange(midi),
        s: t.s,
        line: line(t),
      });
  }
  return { events, endRefDi: refDi, unsupported, skipped };
}

export interface VoicePitch {
  name: string;
  from: number;
  to: number;
  res: PitchResult;
}

// Every `\relative` voice region in the document, in source order, with pitches resolved.
export function voicePitches(src: string): VoicePitch[] {
  const toks = tokenize(src);
  const starts: { name: string; at: number }[] = [];
  for (let i = 0; i + 2 < toks.length; i++) {
    if (toks[i]!.t !== TT.IDENT) continue;
    const eq = nextSig(toks, i + 1);
    if (!eq || eq.t !== TT.PUNCT || eq.raw !== '=') continue;
    const rhs = nextSig(toks, toks.indexOf(eq) + 1);
    if (!rhs || rhs.t !== TT.CMD || !rhs.raw.startsWith('\\relative')) continue;
    starts.push({ name: toks[i]!.raw, at: toks[i]!.s });
  }
  return starts.map((s, i) => {
    const to = i + 1 < starts.length ? starts[i + 1]!.at : src.length;
    return { name: s.name, from: s.at, to, res: resolveRegion(src, s.at, to) };
  });
}

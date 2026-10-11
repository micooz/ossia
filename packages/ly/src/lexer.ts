// L0 tokenizer for the musicxml2ly `.ly` subset (ADR-0002 §2, acceptance criteria §2.7).
//
// Hard invariant: serialize(tokenize(src)) === src for ANY input. Anything the grammar
// does not model degrades to RAW (one char at a time), never to an error.
//
// Every rule is sticky (`y`). Sticky + a literal `^` in the same pattern only ever
// matches at offset 0, so `^` must NOT appear in these rules (§2.6-1).

export const TT = {
  WS: 'ws',
  COMMENT: 'comment',
  SCHEME: 'scheme',
  CMD: 'cmd',
  STR: 'string',
  SEQ: 'seq',
  BRACE: 'brace',
  CHORD_C: 'chordC',
  CHORD_O: 'chordO',
  BAR: 'bar',
  BEAM: 'beam',
  TIE: 'tie',
  FING: 'fing',
  NUM: 'num',
  SLASH: 'slash',
  STAR: 'star',
  ARTIC: 'artic',
  NOTE: 'note',
  REST: 'rest',
  SKIP: 'skip',
  IDENT: 'ident',
  PUNCT: 'punct',
  RAW: 'raw',
} as const;

export type TokenKind = (typeof TT)[keyof typeof TT];
export type Mode = 'top' | 'music' | 'text';

export interface Token {
  t: TokenKind;
  raw: string;
  s: number;
  mode: Mode;
}

// Duration literals: 4 4. 4.. 2.5 — always fully grouped when interpolated (§2.6-2).
const DUR = String.raw`\d+(?:\.\d*)?\.{0,2}`;
// Octave marks: longest first so `,,` wins over `,`.
const OCTAVE = String.raw`(?:,,|'')*(?:,|')*`;
const ACC = String.raw`isis|eses|ses|is|es|ss|ff|s|f`;
// Empirical token shape from musicxml2ly output:
//   root [accidental] [octave marks] [!] [duration] [dots]
// `!` (accidental cancellation) sits *after* the octave marks: `bes,!16`, `d'!4`.
const PITCHISH = new RegExp(
  String.raw`^(?:[a-h](?:${ACC})?(?:,,|'')*(?:,|')*!?${DUR}?` +
    String.raw`|[rR](?:,,|'')*(?:,|')*${DUR}|s${DUR})$`,
);
// Chord members carry no duration (`<c e g>4`), so inside `<…>` a bare spelling is a note.
const BARE_PITCH = new RegExp(String.raw`^[a-h](?:${ACC})?${OCTAVE}!?$`);

export const WORD = /[a-zA-Z][a-zA-Z0-9_.'!,]*/y;

// A `{` after one of these opens a property / markup / chord-name list: contents are
// verbatim text, not a stream of note events.
export const TEXT_CMDS = new Set([
  'header',
  'paper',
  'layout',
  'context',
  'midi',
  'markup',
  'concat',
  'fontsize',
  'smaller',
  'larger',
  'normal-text',
  'rhythm',
  'override',
  'revert',
  'set',
  'char',
  'chordmode',
  'column-line',
  'center-column',
  'general-column',
  'bold',
  'italic',
  'medium',
  'huge',
  'raise',
  'translate',
  'scale',
  'with-color',
  'abs-fontsize',
  'path',
  'circle',
  'musicglyph',
  'bracket',
  'box',
  'note',
  'center-align',
  'left-align',
  'right-align',
  'override',
  'size',
  'sharp',
]);

// Commands whose following bare word is an argument, never a note event.
export const NON_MUSIC_ARG = new Set([
  'key',
  'time',
  'meter',
  'tie',
  'slur',
  'text',
  'markup',
  'override',
  'revert',
  'set',
  'change',
  'context',
  'numericTimeSignature',
  'relative',
  'clef',
  'ottava',
  'partial',
  'repeat',
  'unfold',
  'skip',
  'tempo',
  'barNumberCheck',
  'setStaffIdentNumbering',
  'transposition',
]);

const NUM_RE = new RegExp(
  String.raw`(?:[0-9]+(?:\.[0-9]*)?\s*/\s*[0-9]+|[0-9]+(?:\.[0-9]*)?)`,
  'y',
);

// Text-mode inheritance for a nested brace. Split out so TS's control-flow narrowing (which at
// this point has proved `mode` cannot be 'text' from the outer branch) does not fight the check.
const isText = (m: Mode): boolean => m === 'text';

// The argument exemption covers only the token immediately after the command (§2.4/§2.6-5).
const clearsArg = (t: TokenKind) => t !== TT.WS && t !== TT.COMMENT && t !== TT.CMD;

// Ordered: first match wins.
const rules: [TokenKind, RegExp][] = [
  [TT.WS, /[ \t\r\n\f]+/y],
  [TT.COMMENT, /%\{[\s\S]*?%\}/y],
  [TT.COMMENT, /%[^\n]*/y],
  [
    TT.SCHEME,
    /#(?:#[a-zA-Z][a-zA-Z0-9_-]*|-?(?:[0-9]+(?:\.[0-9]*)?|[a-z]+)|[a-zA-Z][a-zA-Z0-9_-]*|\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}|\((?:[^()]|\([^()]*\))*\)|"(?:[^"\\]|\\.)*")/y,
  ],
  [TT.STR, /"(?:[^"\\]|\\.)*"/y],
  [TT.CMD, /\\(?:[a-zA-Z-]+|[(){}[\]|%;!"/\\])/y],
  [TT.SEQ, /<<|>>/y],
  [TT.CHORD_C, new RegExp(String.raw`>${DUR}?`, 'y')],
  [TT.CHORD_O, /</y],
  [TT.BAR, /\|/y],
  [TT.BEAM, /[[\]]/y],
  [TT.TIE, /~/y],
  [TT.FING, /-(?:[0-9]+|\\[a-zA-Z-]+|->|\.)/y],
  [TT.NUM, NUM_RE],
  [TT.SLASH, /\//y],
  [TT.STAR, /\*/y],
  // `=` is assignment, not an articulation; L1 keys voice regions off it (§2.6-4).
  [TT.PUNCT, /=/y],
  [TT.ARTIC, /[!^_.:>-]/y],
  [TT.BRACE, /[{}]/y],
];

export function tokenize(src: string): Token[] {
  const toks: Token[] = [];
  const stack: Mode[] = [];
  let mode: Mode = 'top';
  let lastCmd: string | null = null;
  let argPending = false;
  let inChord = 0;
  let i = 0;
  const push = (t: TokenKind, raw: string) => {
    toks.push({ t, raw, s: i, mode });
    i += raw.length;
  };

  while (i < src.length) {
    let hit: [TokenKind, string] | null = null;
    for (const [t, re] of rules) {
      re.lastIndex = i;
      const m = re.exec(src);
      if (m && m[0].length > 0) {
        hit = [t, m[0]];
        break;
      }
    }

    if (hit) {
      const [t, raw] = hit;
      if (t === TT.CMD) {
        push(t, raw);
        lastCmd = raw.slice(1);
        argPending = NON_MUSIC_ARG.has(lastCmd);
        continue;
      }
      if (clearsArg(t)) argPending = false;
      const opens = (t === TT.BRACE && raw === '{') || (t === TT.SEQ && raw === '<<');
      const closes = (t === TT.BRACE && raw === '}') || (t === TT.SEQ && raw === '>>');
      if (opens) {
        // A nested `{` inside a text region stays text (`\markup { \bold { x } }`).
        const inner: Mode =
          t === TT.SEQ
            ? 'music'
            : TEXT_CMDS.has(lastCmd || '')
              ? 'text'
              : isText(mode)
                ? 'text'
                : 'music';
        push(t, raw);
        stack.push(mode);
        mode = inner;
        continue;
      }
      if (closes) {
        push(t, raw);
        mode = stack.length ? (stack.pop() as Mode) : 'top';
        continue;
      }
      if (t === TT.CHORD_O) inChord++;
      else if (t === TT.CHORD_C) inChord = Math.max(0, inChord - 1);
      push(t, raw);
      continue;
    }

    WORD.lastIndex = i;
    const w = WORD.exec(src);
    if (w) {
      const raw = w[0];
      const music = mode === 'music' && !argPending;
      const chordMember = music && inChord > 0 && BARE_PITCH.test(raw);
      if (music && (chordMember || PITCHISH.test(raw))) {
        const h = raw[0]!.toLowerCase();
        push(h === 'r' ? TT.REST : h === 's' ? TT.SKIP : TT.NOTE, raw);
      } else push(TT.IDENT, raw);
      argPending = false;
      continue;
    }

    push(TT.RAW, src[i]!);
  }
  return toks;
}

// I1: byte-exact round trip.
export const serialize = (toks: Token[]) => toks.map((x) => x.raw).join('');

// Character offset -> 1-based line/col, the convention LilyPond uses in its logs.
export function posOf(src: string) {
  const starts = [0];
  for (let k = 0; k < src.length; k++) if (src[k] === '\n') starts.push(k + 1);
  return (off: number) => {
    let lo = 0,
      hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid]! <= off) lo = mid;
      else hi = mid - 1;
    }
    return { line: lo + 1, col: off - starts[lo]! + 1 };
  };
}

/** The next token that is not whitespace/comment, starting at `j`. */
export const nextSig = (toks: Token[], j: number): Token | undefined => {
  while (j < toks.length && (toks[j]!.t === TT.WS || toks[j]!.t === TT.COMMENT)) j++;
  return toks[j];
};

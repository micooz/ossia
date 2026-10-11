// Service D — Agent: projection -> LLM -> sanitize/apply -> five gates -> patch record.
//
// Protocol is ADR-0002 §5 verbatim: the model sees the whole `.ly` with a 4-char line gutter and
// `[Voice m<n>]` TARGET tags, and answers with line-splice edits in strict JSON. It never sees
// or produces whole-file text, so the applier can *prove* nothing outside the authorized region
// changed (G-a/G-b). Nothing an LLM emitted is ever written without passing G-c/G-d/G-e.
import { tokenize, TT, buildIndex, resolveRegion, diffTracks, name as midiName } from '@ossia/ly';
import type { PitchEvent, SmfTrack, TrackChange } from '@ossia/ly';

import { renderMidi } from './render.ts';

export const GATEWAY = () => process.env.OSSIA_GATEWAY || 'http://127.0.0.1:3425';
export const DEFAULT_MODEL = () =>
  process.env.OSSIA_MODEL || 'commandcode-plan/deepseek/deepseek-v4.1-flash';

export interface Range {
  tag: string;
  lineStart: number;
  lineEnd: number;
}

/** §5.1: `NNNN | ` gutter plus TARGET tags; the only projection the model is given. */
export function project(src: string, ranges: Range[], facts: string[] = []): string {
  const lines = src.split('\n');
  const label = (l: number) =>
    ranges
      .filter((r) => l >= r.lineStart && l <= r.lineEnd)
      .map((r) => r.tag)
      .join(',');
  const body = lines
    .map((l, i) => `${String(i + 1).padStart(4)} | ${label(i + 1) ? `[${label(i + 1)}] ` : ''}${l}`)
    .join('\n');
  return facts.length
    ? `${body}\n\nPITCH FACTS (absolute pitches, computed by the app):\n${facts.join('\n')}`
    : body;
}

const SYSTEM = (rules: string[]) => `You edit LilyPond source (\`.ly\`) for a music notation app.
You receive the full source with a 4-char line-number gutter: "NNNN | content".
You also receive one or more TARGET ranges described by a tag, and PITCH FACTS listing the
sounding (absolute) pitch of the notes inside each TARGET range.
Make the smallest change that satisfies the instruction, touching ONLY lines inside the
TARGET ranges. Never change the gutter, and never change other lines.
Reply with STRICT JSON only, no prose, no code fences:
{"edits":[{"lineStart":N,"lineEnd":N,"replacement":"new text for those lines, \\n separated"}],"why":"one short Chinese sentence"}
lineStart/lineEnd are 1-based and inclusive, and must lie inside a TARGET range.
If the request is impossible or already satisfied, return {"edits":[],"why":"reason"}.
${rules.map((r, i) => `RULE ${i + 1}: ${r}`).join('\n')}`;

// Required for this class of edit (ADR-0002 §6.1: without it the model emits `bes,8` and drags
// the whole voice down).
export const RULES = {
  relativeOctave:
    'LilyPond pitfall: inside a \\relative region, writing a lone octave marking ' +
    '(e.g. `bes,8`) redefines the reference pitch and transposes EVERY later note of that ' +
    'voice. To move a single note by an octave without affecting the rest, replace that one ' +
    'note with `\\absolute { <note> }`, keeping its duration and fingering.',
  noGutter: 'Never echo the "NNNN | " gutter or a "[Voice m<n>]" tag into `replacement`.',
};

const DEFAULT_RULES = [RULES.relativeOctave, RULES.noGutter];

export interface AskOpts {
  model?: string;
  maxTokens?: number;
  reasoningEffort?: string;
  rules?: string[];
  timeoutMs?: number;
}

export interface AskResult {
  text: string;
  finish: string | undefined;
  ms: number;
  usage: { prompt: number; completion: number } | null;
}

export async function ask(
  src: string,
  ranges: Range[],
  instruction: string,
  opts: AskOpts & { facts?: string[]; history?: { role: string; content: string }[] } = {},
): Promise<AskResult> {
  const prompt = `TARGET ranges: ${ranges.map((r) => `${r.tag} = lines ${r.lineStart}-${r.lineEnd}`).join(', ')}

INSTRUCTION: ${instruction}

SOURCE:
${project(src, ranges, opts.facts ?? [])}`;
  const body: Record<string, unknown> = {
    model: opts.model ?? DEFAULT_MODEL(),
    messages: [
      { role: 'system', content: SYSTEM(opts.rules ?? DEFAULT_RULES) },
      ...(opts.history ?? []),
      { role: 'user', content: prompt },
    ],
    max_tokens: opts.maxTokens ?? 8000,
  };
  // §5.5: reasoning models spend the budget on thinking and return empty content, so effort is
  // capped explicitly and `finish_reason === "length"` is tracked separately from bad format.
  if (opts.reasoningEffort ?? 'low') body.reasoning_effort = opts.reasoningEffort ?? 'low';
  const post = async (b: Record<string, unknown>) => {
    const res = await fetch(`${GATEWAY()}/v1/chat/completions`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        authorization: `Bearer ${process.env.OSSIA_MODEL_API_KEY ?? ''}`,
      },
      body: JSON.stringify(b),
      signal: AbortSignal.timeout(opts.timeoutMs ?? 120000),
    });
    return { ok: res.ok, status: res.status, text: await res.text() };
  };
  const t0 = performance.now();
  let out = await post(body);
  // Some providers reject `temperature` outright, so retry without it rather than failing.
  if (!out.ok && /temperature/i.test(out.text))
    out = await post({ ...body, temperature: undefined });
  if (!out.ok) throw new Error(`gateway HTTP ${out.status}: ${out.text.slice(0, 300)}`);
  const j = JSON.parse(out.text) as {
    choices?: { message?: { content?: string }; finish_reason?: string }[];
    usage?: { prompt_tokens?: number; completion_tokens?: number };
  };
  const choice = j.choices?.[0];
  return {
    text: choice?.message?.content ?? '',
    finish: choice?.finish_reason,
    ms: performance.now() - t0,
    usage: j.usage
      ? { prompt: j.usage.prompt_tokens ?? 0, completion: j.usage.completion_tokens ?? 0 }
      : null,
  };
}

export interface Edit {
  lineStart: number;
  lineEnd: number;
  replacement: string;
}

/** §5.3: models emit reasoning and sometimes two self-corrected objects; take the LAST balanced
 *  object that has an `edits` array, after stripping fences. */
export function parseEdits(
  text: string,
): { edits: Edit[]; why?: string } | { error: string; raw: string } {
  const t = text
    .trim()
    .replace(/^```(?:json)?/i, '')
    .replace(/```$/i, '')
    .trim();
  const openings = [...t.matchAll(/\{/g)]
    .map((m) => m.index)
    .filter((i) => t.slice(i).startsWith('{"edits') || t.slice(i).startsWith('{ "edits'));
  const candidates = openings.length
    ? openings.reverse()
    : [...t.matchAll(/\{/g)].map((m) => m.index).reverse();
  for (const a of candidates) {
    const b = t.lastIndexOf('}');
    if (b < a) continue;
    let o: { edits?: unknown; why?: unknown };
    try {
      o = JSON.parse(t.slice(a, b + 1));
    } catch {
      continue;
    }
    if (!Array.isArray(o.edits)) continue;
    return {
      edits: (o.edits as Edit[]).map((e) => ({
        lineStart: Number(e.lineStart),
        lineEnd: Number(e.lineEnd),
        // Literal newlines inside a JSON string are a common model slip; normalise them.
        replacement: String(e.replacement ?? '').replace(/\r\n?/g, '\n'),
      })),
      why: o.why === undefined ? undefined : String(o.why),
    };
  }
  return { error: 'no parseable edits object', raw: text.slice(0, 300) };
}

/** Strip echoed gutter / TARGET tags before applying (ADR-0002 §5.4): a cosmetic leak that
 *  otherwise breaks `.ly` syntax — 3 of 24 evaluation calls failed before this cleaning.
 *
 * The control-character repair is not cosmetic. A dynamics edit asks the model for `\f`, and JSON
 * parses that escape into U+000C, so a candidate line can become `\time 4/4 <form feed>c4`: a raw
 * control byte in the user's source that LilyPond tolerates and nothing downstream would ever show.
 * `\f` is the only LilyPond command whose escape is a control character (\p, \b, \v, \r are not
 * commands), so it is restored explicitly; every other C0 control is dropped, and a tab becomes two
 * spaces because musicxml2ly output never contains one. */
const stripGutter = (l: string) => l.replace(/^\s*\d{1,6}\s*\|\s?/, '');
const stripTags = (l: string) => l.replace(/\[(?:[A-Za-z_][\w.]*\s)?m\d+(?:-\d+)?\]\s?/g, '');
const repairControls = (l: string) =>
  l
    .replace(/\x0c/g, '\\f')
    .replace(/\t/g, '  ')
    .replace(/[\x00-\x08\x0b\x0e-\x1f]/g, '');
export const sanitize = (replacement: string) =>
  replacement
    .split('\n')
    .map((l) => repairControls(stripTags(stripGutter(l))))
    .join('\n');

export type ApplyResult =
  | { ok: true; src: string; changedLines: number }
  | { ok: false; violations: string[] };

/** G-a: integer bounds inside the file and inside an authorized range. */
export function apply(src: string, edits: Edit[], ranges: Range[]): ApplyResult {
  const lines = src.split('\n');
  const inside = (l: number) => ranges.some((x) => l >= x.lineStart && l <= x.lineEnd);
  const violations: string[] = [];
  const sorted = [...edits].sort((a, b) => a.lineStart - b.lineStart);
  for (const e of sorted) {
    if (!Number.isInteger(e.lineStart) || !Number.isInteger(e.lineEnd)) {
      violations.push(`non-integer line: ${JSON.stringify(e)}`);
      continue;
    }
    if (e.lineStart < 1 || e.lineEnd > lines.length) {
      violations.push(`out of file: ${e.lineStart}-${e.lineEnd}`);
      continue;
    }
    if (!inside(e.lineStart) || !inside(e.lineEnd)) {
      violations.push(`outside TARGET: ${e.lineStart}-${e.lineEnd}`);
      continue;
    }
  }
  if (violations.length) return { ok: false, violations };
  const out: string[] = [];
  let cur = 0;
  for (const e of sorted) {
    while (cur < e.lineStart - 1) out.push(lines[cur++]!);
    out.push(...sanitize(e.replacement).split('\n'));
    cur = e.lineEnd;
  }
  while (cur < lines.length) out.push(lines[cur++]!);
  return {
    ok: true,
    src: out.join('\n'),
    changedLines: out.filter((l, i) => l !== lines[i]).length,
  };
}

/**
 * G-b: inside an authorized line, characters belonging to a *neighbouring* measure must survive.
 * musicxml2ly can fold two measures onto one line, so "the line changed" is not enough — the
 * fragment outside the selected measure span has to still be present in the replacement.
 */
export function outsideRegionKept(
  before: string,
  after: string,
  region: { from: number; to: number },
  authorized: { lineStart: number; lineEnd: number },
): string[] {
  const slice = (s: string) => {
    const keep: string[] = [];
    const lines = s.split('\n');
    for (let i = authorized.lineStart - 1; i < authorized.lineEnd && i < lines.length; i++) {
      const line = lines[i]!;
      const lineStartOff = lines.slice(0, i).join('\n').length + (i > 0 ? 1 : 0);
      const a = Math.max(region.from - lineStartOff, 0);
      const b = Math.min(region.to - lineStartOff, line.length);
      if (a > 0) keep.push(line.slice(0, a).trim());
      if (b < line.length) keep.push(line.slice(b).trim());
    }
    return keep.filter((x) => x.length > 1);
  };
  const beforeFrags = slice(before);
  const afterText = after;
  return beforeFrags.filter((f) => !afterText.includes(f));
}

const sigTokens = (text: string) =>
  tokenize(text)
    .filter((t) => t.t !== TT.WS && t.t !== TT.COMMENT)
    .map((t) => t.raw);

/** §6.2 second channel: trim the common prefix/suffix of significant tokens. Never regex. */
export function tokenDelta(before: string, after: string) {
  const a = sigTokens(before),
    b = sigTokens(after);
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  let j = 0;
  while (j < a.length - i && j < b.length - i && a[a.length - 1 - j] === b[b.length - 1 - j]) j++;
  return { removed: a.slice(i, a.length - j), added: b.slice(i, b.length - j) };
}

/**
 * Isolated one-measure snippet, used to learn how many note events the selection holds (G-d).
 *
 * L1's measure span starts where the *voice region* starts, and a musicxml2ly region starts at its
 * `Name = \\relative fis'' {` assignment. Splicing that text into a snippet yields
 * `\\relative as'' { Name = \\relative fis'' { … }`, which the engine rejects ("not a note name:
 * Name"), so the header is consumed here and its start pitch becomes the snippet's own reference.
 * Brace depth is repaired rather than trimmed, so the last measure of a voice (whose span carries
 * the region's closing `}`) still compiles.
 */
export function measureSnippet(measureText: string): string {
  const rel =
    /^\s*[^\n]*?=\s*\\relative\s+(\S+)\s*\{/.exec(measureText) ??
    /\\relative\s+(\S+)\s*\{/.exec(measureText);
  let body = rel ? measureText.slice(rel.index + rel[0].length) : measureText;
  const depth = [...body].reduce((d, c) => (c === '{' ? d + 1 : c === '}' ? d - 1 : d), 0);
  body = '{'.repeat(Math.max(0, -depth)) + body.trim() + '}'.repeat(Math.max(0, depth));
  // `\relative` only decides octave spelling, and G-d counts note events, so the fallback start
  // pitch is only used for slices that carry no region header at all.
  return `\\version "2.26.0"\n\\score {\n  \\new Staff { \\relative ${rel?.[1] ?? "as''"} { ${body} } }\n  \\layout {}\n  \\midi { \\tempo 4 = 60 }\n}\n`;
}

// Gates read the *sounding* result, so every MIDI probe goes through the MIDI-only pass
// (0.5 s on the corpus) rather than a full engrave.
async function midiTracks(ly: string): Promise<{ tracks: SmfTrack[] } | { error: string }> {
  return renderMidi(ly);
}

// G-d denominator: note-on count in the selected measure. `\layout {}` is dropped for speed and
// the snippet is compiled in isolation, so count/rhythm are preserved (ADR-0002 §8).
export async function measureNoteCount(ly: string, region: { from: number; to: number }) {
  const snippet = measureSnippet(ly.slice(region.from, region.to));
  const r = await midiTracks(snippet);
  if ('error' in r) return { error: r.error };
  const perTrack = r.tracks.map((t) => t.notes.length);
  return { count: perTrack.length ? Math.max(...perTrack) : 0, perTrack };
}

export const blastRadius = (diff: TrackChange[]): number =>
  diff.reduce(
    (a, d) => a + (d.kind === 'pitch' ? (d.nDiffs ?? 0) : (d.after ?? 0) - (d.before ?? 0)),
    0,
  );

/** §6 retry prompt: show the model what its own patch did musically. */
export function feedbackPrompt(diff: TrackChange[], allowed: number): string {
  const lines = diff
    .slice(0, 4)
    .map((d) =>
      d.kind === 'pitch'
        ? `track ${d.track}: ${d.nDiffs} notes changed, e.g. ${d.diffs?.map((x) => `${x.before}->${x.after}`).join(', ')}`
        : `track ${d.track}: note count ${d.before} -> ${d.after}`,
    );
  return (
    `Your previous patch compiled, but it changed ${blastRadius(diff)} sounding notes. ` +
    `The selected measure contains at most ${allowed} note events, so the change leaked ` +
    `outside the selection. Diff:\n${lines.join('\n')}\n` +
    'Fix the patch so that only the selected measure changes. Reply with STRICT JSON only.'
  );
}

/**
 * Pitch facts for the projection (ADR-OPEN-9). Absolute sounding pitches of the notes inside the
 * target ranges, because a spelled octave change only makes sense against the real pitch.
 * Falls back to nothing when the region is outside the verified subset, so the model still gets
 * the RULE text and the gates still decide.
 */
export function pitchFacts(
  ly: string,
  ranges: Range[],
  voices: { name: string; from: number; to: number }[],
): string[] {
  const resolved = voices
    .map((v) => ({ v, res: resolveRegion(ly, v.from, v.to) }))
    .filter((x) => !x.res.unsupported);
  const out: string[] = [];
  for (const r of ranges) {
    for (const { v, res } of resolved) {
      const evs = res.events.filter(
        (e) =>
          e.line >= r.lineStart && e.line <= r.lineEnd && (e.kind === 'note' || e.kind === 'chord'),
      ) as PitchEvent[];
      if (!evs.length) continue;
      const seq = evs.map((e) => e.pitches.map((p) => midiName(p)).join('+')).join(' ');
      out.push(`${r.tag} (${v.name}): ${seq}`);
    }
  }
  return out;
}

export interface Proposal {
  status: 'accepted-candidate' | 'rejected';
  why?: string;
  ly?: string;
  gate: GateReport;
  diff?: TrackChange[];
  tokens?: { removed: string[]; added: string[] };
  model: { name: string; ms: number; usage: AskResult['usage']; finish: string | undefined };
  attempts: number;
}

export interface GateReport {
  'G-a': { ok: boolean; detail?: string };
  'G-b': { ok: boolean; detail?: string };
  'G-c': { ok: boolean; detail?: string };
  'G-d': { ok: boolean; detail?: string };
  'G-e': { ok: boolean; detail?: string };
}

/**
 * Run the whole proposal pipeline for one instruction. Exactly one feedback retry is allowed
 * (ADR-0002 §6.3), and only for G-d/G-e style semantic leaks — format/bounds failures are
 * returned as-is so a bad request is not laundered by repeated sampling.
 */
export async function propose(input: {
  ly: string;
  ranges: Range[];
  voices: { name: string; from: number; to: number }[];
  region: { from: number; to: number; lineStart: number; lineEnd: number };
  instruction: string;
  opts?: AskOpts;
}): Promise<Proposal> {
  const gate: GateReport = {
    'G-a': { ok: false },
    'G-b': { ok: false },
    'G-c': { ok: false },
    'G-d': { ok: false },
    'G-e': { ok: false },
  };
  const facts = pitchFacts(input.ly, input.ranges, input.voices);
  const history: { role: string; content: string }[] = [];
  let attempts = 0;
  let lastModel: Proposal['model'] = {
    name: input.opts?.model ?? DEFAULT_MODEL(),
    ms: 0,
    usage: null,
    finish: undefined,
  };

  for (let round = 0; round < 2; round++) {
    const askRes = await ask(input.ly, input.ranges, input.instruction, {
      ...input.opts,
      facts,
      history: round ? history : undefined,
    });
    attempts++;
    lastModel = {
      name: input.opts?.model ?? DEFAULT_MODEL(),
      ms: askRes.ms,
      usage: askRes.usage,
      finish: askRes.finish,
    };
    if (askRes.finish === 'length') {
      gate['G-e'] = { ok: false, detail: 'finish_reason=length (budget exhausted by reasoning)' };
      return { status: 'rejected', gate, model: lastModel, attempts };
    }
    const parsed = parseEdits(askRes.text);
    if ('error' in parsed) {
      gate['G-a'] = { ok: false, detail: parsed.error };
      return { status: 'rejected', why: parsed.error, gate, model: lastModel, attempts };
    }
    if (!parsed.edits.length) {
      return {
        status: 'rejected',
        why: parsed.why ?? 'model returned no edits',
        gate,
        model: lastModel,
        attempts,
      };
    }
    const applied = apply(input.ly, parsed.edits, input.ranges);
    if (!applied.ok) {
      gate['G-a'] = { ok: false, detail: applied.violations.join('; ') };
      return { status: 'rejected', why: parsed.why, gate, model: lastModel, attempts };
    }
    gate['G-a'] = { ok: true };

    const lost = outsideRegionKept(input.ly, applied.src, input.region, {
      lineStart: input.region.lineStart,
      lineEnd: input.region.lineEnd,
    });
    gate['G-b'] = lost.length
      ? { ok: false, detail: `lost neighbouring fragments: ${lost.slice(0, 3).join(' | ')}` }
      : { ok: true };
    if (!gate['G-b'].ok)
      return { status: 'rejected', why: parsed.why, gate, model: lastModel, attempts };

    const before = await midiTracks(input.ly);
    if ('error' in before) {
      gate['G-c'] = { ok: false, detail: `baseline did not compile: ${before.error}` };
      return { status: 'rejected', why: parsed.why, gate, model: lastModel, attempts };
    }
    const after = await midiTracks(applied.src);
    if ('error' in after) {
      gate['G-c'] = { ok: false, detail: after.error };
      return { status: 'rejected', why: parsed.why, gate, model: lastModel, attempts };
    }
    gate['G-c'] = { ok: true };

    const diff = diffTracks(before.tracks, after.tracks);
    const allowed = await measureNoteCount(input.ly, {
      from: input.region.from,
      to: input.region.to,
    });
    if ('error' in allowed) {
      gate['G-d'] = { ok: false, detail: `measure probe failed: ${allowed.error}` };
    } else {
      const radius = blastRadius(diff);
      gate['G-d'] =
        radius <= allowed.count
          ? { ok: true, detail: `radius ${radius} <= ${allowed.count} note events` }
          : {
              ok: false,
              detail: `radius ${radius} > ${allowed.count} note events in the selection`,
            };
    }
    if (!gate['G-d'].ok) {
      if (round === 0 && !('error' in allowed)) {
        history.push({ role: 'assistant', content: askRes.text });
        history.push({ role: 'user', content: feedbackPrompt(diff, allowed.count) });
        continue;
      }
      return { status: 'rejected', why: parsed.why, gate, diff, model: lastModel, attempts };
    }

    // G-e: the two channels together decide "did the intended thing happen, and only there".
    const tokens = tokenDelta(input.ly, applied.src);
    const sounding = diff.reduce((a, d) => a + (d.kind === 'pitch' ? (d.nDiffs ?? 0) : 1), 0);
    gate['G-e'] = {
      ok: sounding > 0 || tokens.added.length > 0,
      detail:
        sounding > 0
          ? `${sounding} sounding change(s); tokens +[${tokens.added.join(' ')}] -[${tokens.removed.join(' ')}]`
          : tokens.added.length
            ? `non-sounding edit: +[${tokens.added.join(' ')}] -[${tokens.removed.join(' ')}]`
            : 'patch compiled identically: nothing changed',
    };
    if (!gate['G-e'].ok)
      return {
        status: 'rejected',
        why: parsed.why,
        gate,
        diff,
        tokens,
        model: lastModel,
        attempts,
      };
    return {
      status: 'accepted-candidate',
      why: parsed.why,
      ly: applied.src,
      gate,
      diff,
      tokens,
      model: lastModel,
      attempts,
    };
  }
  return { status: 'rejected', gate, model: lastModel, attempts };
}

/** Character span + line range of one measure of a voice: the region G-b protects and the
 *  measure the G-d probe counts. */
export function measureSpan(ly: string, voice: string, measure: number) {
  const i = buildIndex(ly);
  const v = i.voices.find((x) => x.name === voice);
  if (!v) return null;
  const m = v.measures.find((mm) => measure >= mm.measureStart && measure <= mm.measureEnd);
  if (!m) return null;
  return { from: m.from, to: m.to, lineStart: m.lineStart, lineEnd: m.lineEnd, voice: v.name };
}

// Chain 3 — natural-language edit, end to end, as one service call.
//
// locator -> L1 line ranges -> projection with pitch facts -> LLM -> sanitize/apply -> five gates
// -> candidate content (the head does not move until the player accepts) -> re-render ->
// before/after payload. The client only ever receives SVG + summary + hashes; `.ly` stays here.
import { resolveRegion } from '@ossia/ly';

import { propose, type Range } from './agent.ts';
import { locatePoint, resolveLocator, type LocatorTable } from './locate.ts';
import { RenderError } from './render.ts';
import { rendered, peekRendered, type ScoreEntry } from './score.ts';
import {
  getDocument,
  getText,
  putContent,
  getVersion,
  recordPatch,
  updatePatch,
  getPatch,
  commit,
} from './store.ts';

export interface Locator {
  voice?: string;
  staff?: number;
  measureStart: number;
  measureEnd?: number;
}

export class EditError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly detail?: unknown,
  ) {
    super(message);
  }
}

/** Authoritative line ranges for a set of locators, plus the measure character spans they cover. */
export function targets(table: LocatorTable, locators: Locator[]) {
  const ranges: Range[] = [];
  const regions: { voice: string; from: number; to: number; lineStart: number; lineEnd: number }[] =
    [];
  for (const l of locators) {
    for (const r of resolveLocator(table, l)) {
      ranges.push({ tag: r.tag, lineStart: r.lineStart, lineEnd: r.lineEnd });
      const ms = table.measures[r.voice]!.find((m) => m.measureStart === r.measureStart)!;
      const me = table.measures[r.voice]!.find((m) => m.measureEnd === r.measureEnd)!;
      regions.push({
        voice: r.voice,
        from: ms.from,
        to: me.to,
        lineStart: ms.lineStart,
        lineEnd: me.lineEnd,
      });
    }
  }
  if (!ranges.length)
    throw new EditError('no-such-locator', 'no locator matches a measure of this version', {
      locators,
    });
  // Merge duplicate ranges so a multi-voice selection does not project the same lines twice.
  const uniq = [
    ...new Map(ranges.map((r) => [`${r.lineStart}-${r.lineEnd}-${r.tag}`, r])).values(),
  ];
  return { ranges: uniq, regions };
}

export interface Preview {
  hash: string;
  pageCount: number;
  svgUrl: string;
  pdfUrl: string | null;
  midiUrl: string | null;
  renderMs: number;
}

export interface EditResult {
  patchId: string;
  status: 'candidate' | 'rejected';
  why: string | null;
  baseHash: string;
  candidateHash: string | null;
  gate: unknown;
  diff: unknown;
  tokens: unknown;
  model: unknown;
  attempts: number;
  targets: { tag: string; lineStart: number; lineEnd: number }[];
  before: Preview | null;
  after: Preview | null;
}

const preview = (docId: string, hash: string, e: ScoreEntry): Preview => ({
  hash,
  pageCount: e.record.pages.length,
  svgUrl: `/api/v1/documents/${docId}/versions/${hash}/svg/1`,
  pdfUrl: e.record.pdf ? `/api/v1/documents/${docId}/versions/${hash}/pdf` : null,
  midiUrl: e.record.midi ? `/api/v1/documents/${docId}/versions/${hash}/midi` : null,
  renderMs: Math.round(e.record.ms),
});

/**
 * Run one edit task. `accept: true` commits straight away (smoke checklist); by default the
 * result stays a candidate until the player accepts it, because D10 gates acceptance on the
 * rendered before/after rather than on a textual diff.
 */
export async function runEdit(input: {
  docId: string;
  baseHash: string;
  locators: Locator[];
  instruction: string;
  model?: string;
  accept?: boolean;
  onPhase?: (phase: string, step: number) => void;
}): Promise<EditResult> {
  const doc = getDocument(input.docId);
  if (!doc) throw new EditError('no-such-document', input.docId);
  if (!getVersion(input.docId, input.baseHash))
    throw new EditError('no-such-version', input.baseHash);
  const entry = await rendered(input.baseHash, doc.engine_tag);
  const ly = getText(input.baseHash);
  const { ranges, regions } = targets(entry.table, input.locators);

  input.onPhase?.('asking model', 1);
  const proposal = await propose({
    ly,
    ranges,
    // Voice spans come from L1, which already knows the region boundaries.
    voices: entry.table.voices.map((v) => ({ name: v.name, from: v.from, to: v.to })),
    region: regions[0]!,
    instruction: input.instruction,
    opts: input.model ? { model: input.model } : undefined,
  });

  const patchId = recordPatch({
    docId: input.docId,
    baseHash: input.baseHash,
    locator: { locators: input.locators, ranges, regions },
    instruction: input.instruction,
    status: proposal.status === 'accepted-candidate' ? 'candidate' : 'rejected',
    detail: {
      gate: proposal.gate,
      why: proposal.why ?? null,
      model: proposal.model,
      attempts: proposal.attempts,
      diff: proposal.diff ?? null,
      tokens: proposal.tokens ?? null,
    },
  });

  const targetView = ranges.map((r) => ({
    tag: r.tag,
    lineStart: r.lineStart,
    lineEnd: r.lineEnd,
  }));

  if (proposal.status !== 'accepted-candidate' || !proposal.ly) {
    return {
      patchId,
      status: 'rejected',
      why: proposal.why ?? null,
      baseHash: input.baseHash,
      candidateHash: null,
      gate: proposal.gate,
      diff: proposal.diff ?? null,
      tokens: proposal.tokens ?? null,
      model: proposal.model,
      attempts: proposal.attempts,
      targets: targetView,
      before: null,
      after: null,
    };
  }

  input.onPhase?.('rendering result', 2);
  const candidateHash = putContent(proposal.ly);
  const after = await renderOrThrow(candidateHash, doc.engine_tag);
  if (input.accept) {
    const c = commit({
      docId: input.docId,
      baseHash: input.baseHash,
      ly: proposal.ly,
      origin: 'agent',
      author: proposal.model.name,
      meta: {
        patchId,
        why: proposal.why ?? null,
        gate: proposal.gate,
        instruction: input.instruction,
      },
    });
    if (!c.ok) throw new EditError(`commit-${c.reason}`, 'the document moved while editing; retry');
    updatePatch(patchId, { status: 'accepted', resultHash: candidateHash });
  } else {
    updatePatch(patchId, { status: 'candidate', resultHash: candidateHash });
  }

  return {
    patchId,
    status: 'candidate',
    why: proposal.why ?? null,
    baseHash: input.baseHash,
    candidateHash,
    gate: proposal.gate,
    diff: proposal.diff ?? null,
    tokens: proposal.tokens ?? null,
    model: proposal.model,
    attempts: proposal.attempts,
    targets: targetView,
    before: preview(input.docId, input.baseHash, entry),
    after: preview(input.docId, candidateHash, after),
  };
}

async function renderOrThrow(hash: string, engineTag: string): Promise<ScoreEntry> {
  try {
    return await rendered(hash, engineTag);
  } catch (e) {
    if (e instanceof RenderError) throw new EditError('render-failed', e.message, { log: e.log });
    throw e;
  }
}

/** Accept a stored candidate: CAS-commit it. A moved head is refused, never auto-merged. */
export function acceptPatch(docId: string, patchId: string) {
  const p = getPatch(patchId);
  if (!p || p.doc_id !== docId) throw new EditError('no-such-patch', patchId);
  if (!p.result_hash)
    throw new EditError('no-candidate', 'this task produced no candidate content');
  if (p.status === 'accepted') return { status: p.status, headHash: p.result_hash };
  const doc = getDocument(docId);
  if (!doc) throw new EditError('no-such-document', docId);
  if (doc.head_hash !== p.base_hash)
    throw new EditError(
      'conflict',
      'the document changed since this task ran; the candidate is kept but not applied',
    );
  const c = commit({
    docId,
    baseHash: p.base_hash,
    ly: getText(p.result_hash),
    origin: 'agent',
    author: 'player',
    meta: { patchId, instruction: p.instruction },
  });
  if (!c.ok) throw new EditError(`commit-${c.reason}`, 'commit rejected');
  updatePatch(patchId, { status: 'accepted' });
  return { status: 'accepted' as const, headHash: c.version.content_hash, seq: c.version.seq };
}

/** Discard a candidate: the chain does not move; the task is marked so the UI can reset. */
export function discardPatch(docId: string, patchId: string) {
  const p = getPatch(patchId);
  if (!p || p.doc_id !== docId) throw new EditError('no-such-patch', patchId);
  updatePatch(patchId, { status: 'discarded' });
  return { status: 'discarded' as const, headHash: getDocument(docId)!.head_hash };
}

/** Page-space point -> locator (chain 2). Requires the version to be rendered. */
export function locate(docId: string, baseHash: string, page: number, x: number, y: number) {
  const doc = getDocument(docId);
  if (!doc) throw new EditError('no-such-document', docId);
  const e = peekRendered(baseHash);
  if (!e) throw new EditError('not-rendered', 'render this version before selecting');
  const hit = locatePoint(e.table, page, x, y);
  if (!hit) throw new EditError('no-hit', 'the point is not inside a measure column', { x, y });
  return hit;
}

/**
 * Sounding pitches inside a measure span, for the summary the player reads ("这一小节现在是
 * C4 Eb4 ..."). Falls back to null when the region is outside the verified subset.
 */
export function selectionPitches(ly: string, from: number, to: number) {
  const res = resolveRegion(ly, from, to);
  if (res.unsupported) return { unsupported: res.unsupported, notes: [] as string[] };
  return {
    unsupported: null,
    notes: res.events.filter((e) => e.kind === 'note' || e.kind === 'chord').map((e) => e.pitches),
  };
}

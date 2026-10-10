// Render orchestration + per-version cache (services B and C glued to A).
//
// A version's `.ly` is immutable (it is addressed by hash), so its SVG pages, PDF, MIDI and the
// L1/L2 join table can be cached by content hash and replayed. That is what makes the selection
// round-trip and the before/after comparison cheap after the first ~10 s render.
import fs from 'node:fs';
import zlib from 'node:zlib';

import { locatorTable, type LocatorTable } from './locate.ts';
import { renderSet } from './render.ts';
import { getText, type AssetRef } from './store.ts';

export interface PageRef {
  page: number;
  hash: string;
  file: string;
  gz: boolean;
}

export interface RenderRecord {
  contentHash: string;
  engineTag: string;
  pages: PageRef[];
  pdf: PageRef | null;
  midi: PageRef | null;
  ms: number;
  log: string[];
}

export interface ScoreEntry {
  record: RenderRecord;
  table: LocatorTable;
  svg: string[];
}

const cache = new Map<string, ScoreEntry>();
const MAX_ENTRIES = 12;

const toRef = (a: AssetRef, page: number): PageRef => ({
  page,
  hash: a.hash,
  file: a.file,
  gz: a.gz,
});

export function readAsset(ref: PageRef): Buffer {
  const buf = fs.readFileSync(ref.file);
  return ref.gz ? zlib.gunzipSync(buf) : buf;
}

/**
 * Render a stored version if needed and return its artifacts plus the locator table. SVG pages
 * come back as text so the client can inline them; the `.ly` itself is never handed out (G6).
 */
export async function rendered(
  contentHash: string,
  engineTag: string,
  opts: { force?: boolean } = {},
): Promise<ScoreEntry> {
  const hit = opts.force ? undefined : cache.get(contentHash);
  if (hit) return hit;
  const ly = getText(contentHash);
  const set = await renderSet(ly);
  const record: RenderRecord = {
    contentHash,
    engineTag,
    pages: set.pages.map((a, i) => toRef(a, i + 1)),
    pdf: set.pdf ? toRef(set.pdf, 1) : null,
    midi: set.midi ? toRef(set.midi, 1) : null,
    ms: set.ms,
    log: set.log,
  };
  const svg = record.pages.map((p) => readAsset(p).toString('utf8'));
  const table = locatorTable(ly, svg);
  const entry: ScoreEntry = { record, table, svg };
  cache.set(contentHash, entry);
  if (cache.size > MAX_ENTRIES) {
    for (const key of cache.keys()) {
      if (key !== contentHash) {
        cache.delete(key);
        break;
      }
    }
  }
  return entry;
}

/** Artifacts of an already-rendered version, without touching the engine. `null` means the
 *  caller must render (a restored version whose node was never viewed, for example). */
export function peekRendered(contentHash: string): ScoreEntry | null {
  return cache.get(contentHash) ?? null;
}

/**
 * One SVG page of an already-rendered version, as text (the client inlines it). The bytes were
 * sanitized when they were stored, so no server-side scratch path can reach a client (G6) and the
 * asset hash depends only on the score.
 */
export function peekPage(contentHash: string, page: number): string | null {
  const e = cache.get(contentHash);
  if (!e) return null;
  const ref = e.record.pages.find((p) => p.page === page);
  return ref ? readAsset(ref).toString('utf8') : null;
}

/** PDF / MIDI artifact of an already-rendered version, for the export endpoints. */
export function peekBinary(contentHash: string, kind: 'pdf' | 'midi'): Buffer | null {
  const e = cache.get(contentHash);
  if (!e) return null;
  const ref = kind === 'pdf' ? e.record.pdf : e.record.midi;
  return ref ? readAsset(ref) : null;
}

/**
 * Compact view of a render for the job registry: the UI polls `/api/v1/jobs/{id}` every 250 ms,
 * and the full entry carries every SVG page as text. The client only needs to know the render is
 * there, then it fetches the pages from their own URLs.
 */
export const renderSummary = (e: ScoreEntry) => ({
  contentHash: e.record.contentHash,
  engineTag: e.record.engineTag,
  renderMs: Math.round(e.record.ms),
  pageCount: e.record.pages.length,
  hasPdf: !!e.record.pdf,
  hasMidi: !!e.record.midi,
  log: e.record.log,
});

// Service A — document store: version chain in SQLite, content in a gzip CAS.
//
// ADR-0001 §6.4: single writer per document + WAL is enough; `parent_hash` makes the chain a
// tamper-evident prefix, and every write carries the base hash so a stale client is rejected
// (CAS) instead of silently overwriting. `.ly` bytes and the MusicXML baseline stay server-side
// (G6 / D9) and are only ever addressed by hash.
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import zlib from 'node:zlib';

/** Where the repo root is: `next dev`/`next start` run with cwd = packages/backend. */
const projectRoot = (): string =>
  process.env.OSSIA_REPO_ROOT || path.resolve(process.cwd(), '../..');

/**
 * Resolve `OSSIA_DATA_DIR` without assuming a working directory, so the same value works in dev,
 * in `next start`, and under any process manager.
 *   - unset       -> production uses `~/.ossia`, everything else uses `<repo root>/.ossia`
 *   - `~/x`, `~`  -> expanded against `homedir()` (a shell would do this; an env file is not a shell)
 *   - `x`/`./x`   -> relative to the repo root, so `.env` files can stay machine-independent
 *   - `/x`        -> verbatim
 * Pure (no mkdir, no globals read beyond its arguments) so it is unit-testable.
 */
export function resolveDataDir(
  env: NodeJS.ProcessEnv = process.env,
  root: string = projectRoot(),
  home: string = os.homedir(),
): string {
  const raw = (env.OSSIA_DATA_DIR ?? '').trim();
  if (!raw) {
    return env.NODE_ENV === 'production' ? path.join(home, '.ossia') : path.join(root, '.ossia');
  }
  if (raw === '~') return home;
  if (raw.startsWith('~/')) return path.join(home, raw.slice(2));
  if (path.isAbsolute(raw)) return path.normalize(raw);
  return path.resolve(root, raw);
}

/** The data directory, created on use: `cas/` and `assets/` are addressed by hash, `db.sqlite` is the chain. */
export function dataDir(): string {
  const dir = resolveDataDir();
  fs.mkdirSync(path.join(dir, 'cas'), { recursive: true });
  fs.mkdirSync(path.join(dir, 'assets'), { recursive: true });
  return dir;
}

const SCHEMA = `
CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  head_hash TEXT NOT NULL,
  engine_tag TEXT NOT NULL,
  baseline_hash TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS versions (
  doc_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  parent_hash TEXT,
  origin TEXT NOT NULL,
  author TEXT NOT NULL,
  meta TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (doc_id, seq)
);
CREATE TABLE IF NOT EXISTS assets (
  hash TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  page INTEGER,
  path TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS patches (
  id TEXT PRIMARY KEY,
  doc_id TEXT NOT NULL,
  base_hash TEXT NOT NULL,
  result_hash TEXT,
  locator TEXT NOT NULL,
  instruction TEXT NOT NULL,
  status TEXT NOT NULL,
  detail TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS versions_hash ON versions (content_hash);
`;

let handle: DatabaseSync | null = null;

export function db(): DatabaseSync {
  if (handle) return handle;
  handle = new DatabaseSync(path.join(dataDir(), 'db.sqlite'));
  handle.exec('PRAGMA journal_mode = WAL');
  handle.exec('PRAGMA busy_timeout = 5000');
  handle.exec(SCHEMA);
  return handle;
}

export const sha256 = (buf: Buffer | string): string =>
  crypto.createHash('sha256').update(buf).digest('hex');

/** Put `.ly`/baseline bytes into the gzip CAS. Idempotent; returns the content hash. */
export function putContent(bytes: Buffer | string): string {
  const buf = Buffer.isBuffer(bytes) ? bytes : Buffer.from(bytes, 'utf8');
  const hash = sha256(buf);
  const f = path.join(dataDir(), 'cas', `${hash}.gz`);
  if (!fs.existsSync(f)) fs.writeFileSync(f, zlib.gzipSync(buf, { level: 9 }));
  return hash;
}

export function getContent(hash: string): Buffer {
  const f = path.join(dataDir(), 'cas', `${hash}.gz`);
  if (!fs.existsSync(f)) throw new Error(`no such content: ${hash}`);
  return zlib.gunzipSync(fs.readFileSync(f));
}

export const getText = (hash: string): string => getContent(hash).toString('utf8');

export interface AssetRef {
  hash: string;
  file: string;
  gz: boolean;
}

/** Store a render artifact so it can be served by hash. SVGs are gzipped: ~25–28 KB/page. */
export function putAsset(
  kind: 'svg' | 'pdf' | 'midi',
  buf: Buffer,
  page: number | null = null,
): AssetRef {
  const hash = sha256(buf);
  const ext = kind === 'svg' ? 'svg' : kind === 'pdf' ? 'pdf' : 'midi';
  const rel = `${hash}${page != null ? `.p${page}` : ''}.${ext}`;
  const f = path.join(dataDir(), 'assets', rel);
  const gz = kind === 'svg';
  if (!fs.existsSync(f)) fs.writeFileSync(f, gz ? zlib.gzipSync(buf, { level: 9 }) : buf);
  db()
    .prepare('INSERT OR IGNORE INTO assets (hash, kind, page, path, created_at) VALUES (?,?,?,?,?)')
    .run(hash, kind, page, rel, Date.now());
  return { hash, file: f, gz };
}

/** Re-read a stored asset by its original bytes' hash (used to replay a cached render). */
export function findAsset(
  kind: 'svg' | 'pdf' | 'midi',
  hash: string,
  page: number | null,
): AssetRef | null {
  const row = db()
    .prepare('SELECT path FROM assets WHERE hash = ? AND kind = ? AND (page IS ? OR page = ?)')
    .get(hash, kind, page, page) as { path: string } | undefined;
  if (!row) return null;
  const file = path.join(dataDir(), 'assets', row.path);
  if (!fs.existsSync(file)) return null;
  return { hash, file, gz: kind === 'svg' };
}

export interface VersionRow {
  doc_id: string;
  seq: number;
  content_hash: string;
  parent_hash: string | null;
  origin: string;
  author: string;
  meta: string | null;
  created_at: number;
}

export interface DocumentRow {
  id: string;
  title: string;
  head_hash: string;
  engine_tag: string;
  baseline_hash: string | null;
  created_at: number;
  updated_at: number;
}

export function createDocument(input: {
  title: string;
  ly: string;
  baseline?: Buffer;
  engineTag: string;
  meta?: unknown;
}): { doc: DocumentRow; version: VersionRow } {
  const d = db();
  const contentHash = putContent(input.ly);
  const id = crypto.randomUUID();
  const now = Date.now();
  d.prepare(
    'INSERT INTO documents (id, title, head_hash, engine_tag, baseline_hash, created_at, updated_at) VALUES (?,?,?,?,?,?,?)',
  ).run(id, input.title, contentHash, input.engineTag, null, now, now);
  if (input.baseline) {
    const bh = putContent(input.baseline);
    d.prepare('UPDATE documents SET baseline_hash = ? WHERE id = ?').run(bh, id);
  }
  const version: VersionRow = {
    doc_id: id,
    seq: 1,
    content_hash: contentHash,
    parent_hash: null,
    origin: 'import',
    author: 'system',
    meta: input.meta === undefined ? null : JSON.stringify(input.meta),
    created_at: now,
  };
  insertVersion(version);
  return { doc: getDocument(id)!, version };
}

export function insertVersion(v: VersionRow): void {
  db()
    .prepare(`INSERT INTO versions (doc_id, seq, content_hash, parent_hash, origin, author, meta, created_at)
    VALUES (?,?,?,?,?,?,?,?)`)
    .run(v.doc_id, v.seq, v.content_hash, v.parent_hash, v.origin, v.author, v.meta, v.created_at);
}

export const getDocument = (id: string): DocumentRow | null =>
  (db().prepare('SELECT * FROM documents WHERE id = ?').get(id) as DocumentRow | undefined) ?? null;

export const listDocuments = (): DocumentRow[] =>
  db()
    .prepare('SELECT * FROM documents ORDER BY updated_at DESC')
    .all() as unknown as DocumentRow[];

export const latestSeq = (docId: string): number =>
  (
    db().prepare('SELECT MAX(seq) AS s FROM versions WHERE doc_id = ?').get(docId) as {
      s: number | null;
    }
  ).s ?? 0;

export const getVersion = (docId: string, hash: string): VersionRow | null =>
  (db().prepare('SELECT * FROM versions WHERE doc_id = ? AND content_hash = ?').get(docId, hash) as
    | VersionRow
    | undefined) ?? null;

export const listVersions = (docId: string): VersionRow[] =>
  db()
    .prepare('SELECT * FROM versions WHERE doc_id = ? ORDER BY seq DESC')
    .all(docId) as unknown as VersionRow[];

/**
 * CAS commit: advance the head only while it still equals `baseHash`. A stale base is rejected,
 * never merged (ADR-0001 §6.4). An edit that produces identical content reuses the head node.
 */
export function commit(input: {
  docId: string;
  baseHash: string;
  ly: string;
  origin: string;
  author: string;
  meta?: unknown;
}):
  | { ok: true; version: VersionRow }
  | { ok: false; reason: 'conflict' | 'no-such-base' | 'no-such-document' } {
  const d = db();
  const doc = getDocument(input.docId);
  if (!doc) return { ok: false, reason: 'no-such-document' };
  if (doc.head_hash !== input.baseHash) return { ok: false, reason: 'conflict' };
  if (!getVersion(input.docId, input.baseHash)) return { ok: false, reason: 'no-such-base' };
  const contentHash = putContent(input.ly);
  if (contentHash === input.baseHash) {
    const v = getVersion(input.docId, contentHash)!;
    d.prepare('UPDATE documents SET updated_at = ? WHERE id = ?').run(Date.now(), input.docId);
    return { ok: true, version: v };
  }
  const version: VersionRow = {
    doc_id: input.docId,
    seq: latestSeq(input.docId) + 1,
    content_hash: contentHash,
    parent_hash: input.baseHash,
    origin: input.origin,
    author: input.author,
    meta: input.meta === undefined ? null : JSON.stringify(input.meta),
    created_at: Date.now(),
  };
  insertVersion(version);
  d.prepare('UPDATE documents SET head_hash = ?, updated_at = ? WHERE id = ?').run(
    contentHash,
    version.created_at,
    input.docId,
  );
  return { ok: true, version };
}

/** Roll the head back to an existing version: a new chain node pointing at old content. */
export function restore(input: {
  docId: string;
  baseHash: string;
  targetHash: string;
}): { ok: true; version: VersionRow } | { ok: false; reason: string } {
  const d = db();
  const doc = getDocument(input.docId);
  if (!doc) return { ok: false, reason: 'no-such-document' };
  if (doc.head_hash !== input.baseHash) return { ok: false, reason: 'conflict' };
  const target = getVersion(input.docId, input.targetHash);
  if (!target) return { ok: false, reason: 'no-such-version' };
  if (target.content_hash === doc.head_hash) return { ok: true, version: target };
  const version: VersionRow = {
    doc_id: input.docId,
    seq: latestSeq(input.docId) + 1,
    content_hash: target.content_hash,
    parent_hash: input.baseHash,
    origin: 'restore',
    author: 'user',
    meta: JSON.stringify({ restoredFrom: target.content_hash.slice(0, 12), seq: target.seq }),
    created_at: Date.now(),
  };
  insertVersion(version);
  d.prepare('UPDATE documents SET head_hash = ?, updated_at = ? WHERE id = ?').run(
    version.content_hash,
    version.created_at,
    input.docId,
  );
  return { ok: true, version };
}

export interface PatchRow {
  id: string;
  doc_id: string;
  base_hash: string;
  result_hash: string | null;
  locator: string;
  instruction: string;
  status: string;
  detail: string | null;
  created_at: number;
}

/** Record an Agent task so the UI can poll it and the audit trail keeps the instruction + outcome. */
export function recordPatch(p: {
  docId: string;
  baseHash: string;
  resultHash?: string | null;
  locator: unknown;
  instruction: string;
  status: string;
  detail?: unknown;
}): string {
  const id = crypto.randomUUID();
  db()
    .prepare(
      'INSERT INTO patches (id, doc_id, base_hash, result_hash, locator, instruction, status, detail, created_at) VALUES (?,?,?,?,?,?,?,?,?)',
    )
    .run(
      id,
      p.docId,
      p.baseHash,
      p.resultHash ?? null,
      JSON.stringify(p.locator),
      p.instruction,
      p.status,
      p.detail === undefined ? null : JSON.stringify(p.detail),
      Date.now(),
    );
  return id;
}

export const listPatches = (docId: string): PatchRow[] =>
  db()
    .prepare('SELECT * FROM patches WHERE doc_id = ? ORDER BY created_at DESC')
    .all(docId) as unknown as PatchRow[];

export const getPatch = (id: string): PatchRow | null =>
  (db().prepare('SELECT * FROM patches WHERE id = ?').get(id) as PatchRow | undefined) ?? null;

export function updatePatch(
  id: string,
  patch: {
    status?: string;
    resultHash?: string | null;
    detail?: unknown;
  },
): void {
  const sets: string[] = [];
  const values: (string | number | null)[] = [];
  if (patch.status !== undefined) {
    sets.push('status = ?');
    values.push(patch.status);
  }
  if (patch.resultHash !== undefined) {
    sets.push('result_hash = ?');
    values.push(patch.resultHash);
  }
  if (patch.detail !== undefined) {
    sets.push('detail = ?');
    values.push(JSON.stringify(patch.detail));
  }
  if (!sets.length) return;
  db()
    .prepare(`UPDATE patches SET ${sets.join(', ')} WHERE id = ?`)
    .run(...values, id);
}

/** JSON-safe view of a patch row (parses the stored detail/locator back out for the client). */
export const patchView = (p: PatchRow) => ({
  id: p.id,
  docId: p.doc_id,
  baseHash: p.base_hash,
  resultHash: p.result_hash,
  instruction: p.instruction,
  status: p.status,
  createdAt: p.created_at,
  locator: JSON.parse(p.locator) as unknown,
  detail: p.detail ? (JSON.parse(p.detail) as unknown) : null,
});

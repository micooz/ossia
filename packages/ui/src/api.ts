// The whole client surface: typed calls over the v1 API. Nothing here knows about React, and
// nothing here can read a file — the .ly source never arrives (G6), only SVG, locators and hashes.
import type { Endpoint } from './endpoint.ts';

// No constructor parameter properties: Vite only erases types, it does not transform them.
export class ApiError extends Error {
  status: number;
  code: string;
  detail?: unknown;
  constructor(status: number, code: string, message: string, detail?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.detail = detail;
  }
}

export function makeApi(ep: Endpoint) {
  const headers = (extra?: Record<string, string>): Record<string, string> => ({
    ...(ep.token ? { 'x-ossia-token': ep.token } : {}),
    ...extra,
  });

  async function json<T>(path: string, init: RequestInit = {}): Promise<T> {
    const res = await fetch(ep.baseUrl + path, {
      ...init,
      headers: headers(init.headers as Record<string, string>),
    });
    const text = await res.text();
    let body: unknown = null;
    try {
      body = text ? JSON.parse(text) : null;
    } catch {
      /* a non-JSON answer is reported below */
    }
    if (!res.ok) {
      const err = (body as { error?: { code?: string; message?: string; detail?: unknown } } | null)
        ?.error;
      throw new ApiError(
        res.status,
        err?.code ?? `http-${res.status}`,
        err?.message ?? `${res.status} ${res.statusText || ''} on ${path}`.trim(),
        err?.detail,
      );
    }
    return body as T;
  }

  return {
    urls: { asset: (path: string) => ep.baseUrl + path },

    health: () => json<{ ok: boolean; service: string; pid: number }>('/health'),

    listDocuments: () => json<{ documents: DocumentSummary[] }>('/api/v1/documents'),

    /**
     * Import a MusicXML/MIDI file. A 422 `confirm-required` is not an error: it is the ADR §6.1
     * report, and `confirm` carries the digests the player accepted on a second attempt.
     */
    importDocument: (file: File, confirm?: string) => {
      const form = new FormData();
      form.append('file', file);
      if (confirm) form.append('confirm', confirm);
      return json<ImportResult>('/api/v1/documents', { method: 'POST', body: form });
    },

    document: (docId: string) => json<DocumentDetail>(`/api/v1/documents/${docId}`),

    /** Start a render of one version. `done` means it was already rendered (cache hit). */
    render: (docId: string, hash: string, force = false) =>
      json<{ jobId: string | null; done?: VersionPayload }>(
        `/api/v1/documents/${docId}/versions/${hash}${force ? '?force=1' : ''}`,
        { method: 'POST' },
      ),

    version: (docId: string, hash: string) =>
      json<VersionPayload | { pending: true }>(`/api/v1/documents/${docId}/versions/${hash}`),

    /**
     * Page-space point -> locator (chain 2). x/y are the SVG's own user units, not CSS pixels, and
     * `page` selects which page's geometry the server tests.
     */
    locate: (docId: string, hash: string, page: number, x: number, y: number) =>
      json<{ locator: Located }>(`/api/v1/documents/${docId}/locate`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ hash, page, x, y }),
      }),

    /** Natural-language edit (chain 3). Returns a job id; poll it for the gate report. */
    edit: (docId: string, body: EditRequest) =>
      json<{ jobId: string }>(`/api/v1/documents/${docId}/jobs`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      }),

    patch: (docId: string, patchId: string) =>
      json<PatchView>(`/api/v1/documents/${docId}/patches/${patchId}`),

    patchAction: (docId: string, patchId: string, action: 'accept' | 'discard') =>
      json<{ status: string; headHash?: string; seq?: number }>(
        `/api/v1/documents/${docId}/patches/${patchId}`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ action }),
        },
      ),

    restore: (docId: string, baseHash: string, targetHash: string) =>
      json<{ jobId: string; version: { seq: number; hash: string; origin: string } }>(
        `/api/v1/documents/${docId}/restore`,
        {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ baseHash, targetHash }),
        },
      ),

    job: (jobId: string) => json<JobView>(`/api/v1/jobs/${jobId}`),

    /** Poll one job until it settles. 250 ms is the settled MVP cadence (no SSE). */
    async waitJob<T>(jobId: string, onTick?: (j: JobView) => void): Promise<T> {
      for (;;) {
        const j = await this.job(jobId);
        onTick?.(j);
        if (j.state === 'done') return j.result as T;
        if (j.state === 'error')
          throw new ApiError(
            500,
            j.error?.code ?? 'job-failed',
            j.error?.message ?? 'job failed',
            j.error?.detail,
          );
        await new Promise((r) => setTimeout(r, 250));
      }
    },

    /** Download PDF/MIDI through the browser so the shell keeps its own save dialog simple. */
    exportUrl: (docId: string, format: 'pdf' | 'midi', hash: string) =>
      ep.baseUrl + `/api/v1/documents/${docId}/export/${format}?hash=${encodeURIComponent(hash)}`,

    /**
     * Fetch an asset with the token header attached. `<img src>`/`<a href>` cannot carry
     * `x-ossia-token`, so SVG pages and downloads go through fetch + object URL instead.
     */
    async fetchText(path: string): Promise<string> {
      const res = await fetch(ep.baseUrl + path, { headers: headers() });
      if (!res.ok) throw new ApiError(res.status, `http-${res.status}`, `${path} -> ${res.status}`);
      return res.text();
    },
    async download(path: string, filename: string): Promise<void> {
      const res = await fetch(ep.baseUrl + path, { headers: headers() });
      if (!res.ok) throw new ApiError(res.status, `http-${res.status}`, `${path} -> ${res.status}`);
      const blob = await res.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    },
  };
}

export type Api = ReturnType<typeof makeApi>;

// ---- wire types ---------------------------------------------------------------

export interface DocumentSummary {
  id: string;
  title: string;
  headHash: string;
  engineTag: string;
  createdAt: number;
  updatedAt: number;
}

export interface ImportStats {
  status: 'clean' | 'needs-confirmation' | 'refused';
  measures: number;
  voices: number;
  noteEvents: number;
  blocking: { warning: string; digest: string; reason: string }[];
  losses: { warning: string; digest: string }[];
  notes: string[];
  hint: string | null;
  requiredConfirm?: string[];
}

export interface ImportResult {
  document: DocumentSummary;
  version: { seq: number; hash: string; origin: string };
  stats: ImportStats;
  acknowledged: boolean;
  render: { jobId: string };
  bytes: number;
}

export interface VersionPayload {
  hash: string;
  engineTag: string;
  renderMs: number;
  log: string[];
  pageCount: number;
  pages: { page: number; svgUrl: string }[];
  pdfUrl: string | null;
  midiUrl: string | null;
  layout: {
    pageCount: number;
    staffSpace: number | null;
    pages: PageLayout[];
  };
  staves: { staff: number; voices: string[] }[];
  measures: Record<string, { count: number; maxMeasure: number }>;
}

export interface DocumentDetail {
  document: {
    id: string;
    title: string;
    headHash: string;
    engineTag: string;
    baseline: boolean;
    createdAt: number;
    updatedAt: number;
  };
  versions: {
    seq: number;
    hash: string;
    parentHash: string | null;
    origin: string;
    author: string;
    createdAt: number;
    isHead: boolean;
    meta: unknown;
    preview: { pageCount: number; ms: number; svgUrl: string } | null;
  }[];
}

export interface PageLayout {
  page: number;
  staffSpace: number;
  systems: {
    system: number;
    top: number;
    bot: number;
    barlines: number[];
    measures: number;
    staffs: { top: number; bot: number; x0: number; x1: number }[];
  }[];
  /** Global measure numbers, in reading order across pages (see server/layout-join.ts). */
  columns: {
    system: number;
    staff: number;
    measure: number;
    column: number;
    x0: number;
    x1: number;
  }[];
}

export interface Located {
  page: number;
  staff: number;
  voice: string | null;
  measure: number;
  system: number;
  lineStart: number | null;
  lineEnd: number | null;
}

export interface Locator {
  staff?: number;
  voice?: string;
  measureStart: number;
  measureEnd?: number;
}

export interface EditRequest {
  baseHash: string;
  locators: Locator[];
  instruction: string;
  model?: string;
  accept?: boolean;
}

export interface GateReport {
  'G-a': { ok: boolean; detail?: string };
  'G-b': { ok: boolean; detail?: string };
  'G-c': { ok: boolean; detail?: string };
  'G-d': { ok: boolean; detail?: string };
  'G-e': { ok: boolean; detail?: string };
}

export interface Preview {
  hash: string;
  pageCount: number;
  svgUrl: string;
  pdfUrl: string | null;
  midiUrl: string | null;
  renderMs: number;
}

export interface EditResultView {
  patchId: string;
  status: 'candidate' | 'rejected';
  why: string | null;
  baseHash: string;
  candidateHash: string | null;
  gate: GateReport;
  diff: unknown;
  tokens: { removed: string[]; added: string[] } | null;
  model: {
    name: string;
    ms: number;
    usage: { prompt: number; completion: number } | null;
    finish: string | undefined;
  };
  attempts: number;
  targets: { tag: string; lineStart: number; lineEnd: number }[];
  before: Preview | null;
  after: Preview | null;
}

export interface PatchView {
  id: string;
  docId: string;
  baseHash: string;
  resultHash: string | null;
  instruction: string;
  status: string;
  createdAt: number;
  locator: unknown;
  detail: unknown;
}

export interface JobView {
  jobId: string;
  kind: string;
  state: 'queued' | 'running' | 'done' | 'error';
  phase: string;
  progress: number;
  result: unknown;
  error: { code: string; message: string; detail?: unknown } | null;
  createdAt: number;
  updatedAt: number;
}

export interface RenderSummary {
  contentHash: string;
  engineTag: string;
  renderMs: number;
  pageCount: number;
  hasPdf: boolean;
  hasMidi: boolean;
  log: string[];
}

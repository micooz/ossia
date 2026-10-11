// HTTP plumbing shared by the API surface: loopback guard, token handshake, CORS for the
// SPA origins, and a job registry the UI polls (250 ms; the MVP has no SSE).
import { createHash, randomUUID } from 'node:crypto';

export const TOKEN_HEADER = 'x-ossia-token';

// Origins and the header set live in origins.ts so the preflight middleware can share them.
import { corsFor } from './origins.ts';

/** CORS response headers for this request's Origin, per the shared allowlist. */
export const corsHeaders = (req: Request): Record<string, string> =>
  corsFor(req.headers.get('origin'));

export function ok<T>(data: T, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(data), {
    ...init,
    status: init.status ?? 200,
    headers: { 'content-type': 'application/json; charset=utf-8', ...(init.headers as object) },
  });
}

export function fail(status: number, code: string, message: string, extra?: unknown): Response {
  // `extra` is the route's structured detail (warnings, gate report). It is nested rather than
  // spread, so a detail object can never shadow `code` or `message`.
  return new Response(
    JSON.stringify({
      error: extra === undefined ? { code, message } : { code, message, detail: extra },
    }),
    {
      status,
      headers: { 'content-type': 'application/json; charset=utf-8' },
    },
  );
}

/**
 * Run `fn` with auth + CORS + error translation applied. Route handlers stay plain
 * `export const GET = (req: Request) => withApi(req, …)` so Next's generated route types are
 * satisfied without a generic wrapper fighting over `ctx.params`.
 */
export async function withApi(
  req: Request,
  fn: (req: Request) => Promise<Response> | Response,
): Promise<Response> {
  if (req.method === 'OPTIONS')
    return new Response(null, { status: 204, headers: corsHeaders(req) });
  const denied = authorize(req);
  if (denied) return json_(denied, corsHeaders(req));
  try {
    return json_(await fn(req), corsHeaders(req));
  } catch (e) {
    const err = e as Error & { code?: string; status?: number; detail?: unknown };
    return json_(
      fail(
        err.status ?? 500,
        err.code ?? 'internal',
        err.message || String(err),
        err.detail ? { detail: err.detail } : {},
      ),
      corsHeaders(req),
    );
  }
}

function json_(res: Response, cors: Record<string, string>): Response {
  const headers = new Headers(res.headers);
  for (const [k, v] of Object.entries(cors)) headers.set(k, v);
  return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
}

/**
 * Deployment auth is a shared secret in `x-ossia-token`, whatever host the server runs on. When
 * `OSSIA_TOKEN` is unset (plain `pnpm dev`), requests are still restricted to loopback hosts so a
 * browser page on another machine cannot drive the engine. A real deployment must set the token (or
 * replace this with session auth): with it unset and a public Host, every API route is open.
 */
export function authorize(req: Request): Response | null {
  const expected = process.env.OSSIA_TOKEN;
  if (expected) {
    const got = req.headers.get(TOKEN_HEADER);
    if (!timingSafeEqual(got ?? '', expected)) {
      return fail(401, 'unauthorized', 'missing or wrong x-ossia-token');
    }
    return null;
  }
  const host = (req.headers.get('host') ?? '').replace(/:\d+$/, '');
  const loop = host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '';
  if (!loop) return fail(403, 'loopback-only', `refusing non-loopback host: ${host}`);
  return null;
}

function timingSafeEqual(a: string, b: string): boolean {
  const ha = createHash('sha256').update(a).digest();
  const hb = createHash('sha256').update(b).digest();
  if (ha.length !== hb.length) return false;
  let diff = 0;
  for (let i = 0; i < ha.length; i++) diff |= ha[i]! ^ hb[i]!;
  return diff === 0;
}

// ---- jobs --------------------------------------------------------------------

export type JobState = 'queued' | 'running' | 'done' | 'error';

export interface Job<T = unknown> {
  id: string;
  kind: string;
  state: JobState;
  /** Coarse phase name + step count so the UI can show progress while polling. */
  phase: string;
  step: number;
  steps: number;
  result?: T;
  error?: { code: string; message: string; detail?: unknown };
  createdAt: number;
  updatedAt: number;
}

const jobs = new Map<string, Job>();
const JOB_TTL_MS = 30 * 60 * 1000;

export function newJob(kind: string, steps = 3): Job {
  sweep();
  const job: Job = {
    id: randomUUID(),
    kind,
    state: 'queued',
    phase: 'queued',
    step: 0,
    steps,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  };
  jobs.set(job.id, job);
  return job;
}

export const getJob = (id: string): Job | undefined => jobs.get(id);

export function advance(id: string, phase: string, step = undefined as number | undefined): void {
  const j = jobs.get(id);
  if (!j) return;
  j.phase = phase;
  if (step != null) j.step = step;
  j.state = j.state === 'queued' ? 'running' : j.state;
  j.updatedAt = Date.now();
}

export function finish<T>(id: string, result: T): void {
  const j = jobs.get(id);
  if (!j) return;
  j.state = 'done';
  j.phase = 'done';
  j.step = j.steps;
  j.result = result;
  j.updatedAt = Date.now();
}

export function jobFail(id: string, code: string, message: string, detail?: unknown): void {
  const j = jobs.get(id);
  if (!j) return;
  j.state = 'error';
  j.phase = 'error';
  j.error = { code, message, detail };
  j.updatedAt = Date.now();
}

/**
 * Start a job on the next tick and return its id immediately: renders and Agent tasks take
 * seconds, so the caller polls instead of holding a request open. `summarize` reduces the work
 * result before it is stored, because the UI polls every 250 ms and a render result holds the
 * whole SVG body (~600 KB per page) — the client is meant to fetch that once, from its own URL.
 */
export function startJob<R>(
  kind: string,
  steps: number,
  work: (job: Job) => Promise<R>,
  summarize: (r: R) => unknown = (r) => r,
): { jobId: string } {
  const job = newJob(kind, steps);
  queueMicrotask(() => {
    advance(job.id, 'running', 1);
    work(job)
      .then((r) => finish(job.id, summarize(r)))
      .catch((e) => {
        const err = e as Error & { code?: string; detail?: unknown };
        // Carry as much of the failure as the log allows: engine errors live in `RenderError.log`,
        // and a bare "compile failed" is useless for a user-reported regression.
        const detail =
          err.detail ??
          (err as { log?: string[] }).log ??
          String(err.stack ?? err)
            .split('\n')
            .slice(0, 4);
        console.error(`[job ${job.id} ${kind}] failed: ${err.message || String(err)}`, detail);
        jobFail(job.id, err.code ?? 'job-failed', err.message || String(err), detail);
      });
  });
  return { jobId: job.id };
}

function sweep(): void {
  const now = Date.now();
  for (const [id, j] of jobs) {
    if (j.state !== 'done' && j.state !== 'error') continue;
    if (now - j.updatedAt > JOB_TTL_MS) jobs.delete(id);
  }
}

export const jobView = (j: Job) => ({
  jobId: j.id,
  kind: j.kind,
  state: j.state,
  phase: j.phase,
  progress: j.steps ? Math.min(1, j.step / j.steps) : 0,
  result: j.result ?? null,
  error: j.error ?? null,
  createdAt: j.createdAt,
  updatedAt: j.updatedAt,
});

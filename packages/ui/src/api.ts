import type { Session } from './endpoint.ts';
import type {
  DocumentState,
  EditArgs,
  ExportArgs,
  ImportArgs,
  Job,
  LocateArgs,
  LocateOutcome,
  Locator,
  RenderGeometry,
  RestoreArgs,
  RuntimeEvent,
  Selection,
  StatusSnapshot,
  Version,
  ViewSnapshot,
} from './generated/protocol.ts';
import { jobStateKey, translate } from './i18n.tsx';

export class ApiError extends Error {
  constructor(publicStatus: number, code: string, message: string, detail?: unknown) {
    super(message);
    this.status = publicStatus;
    this.code = code;
    this.detail = detail;
  }
  status: number;
  code: string;
  detail?: unknown;
}
export function describe(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object' && 'message' in error) return String(error.message);
  return JSON.stringify(error) ?? String(error);
}
export function makeApi(session: Session, signal?: AbortSignal) {
  async function request(path: string, init: RequestInit = {}) {
    const headers = new Headers(init.headers);
    headers.set('Authorization', `Bearer ${session.token}`);
    const response = await fetch(`/api/v1${path}`, {
      ...init,
      headers,
      signal: init.signal ?? signal,
    });
    if (!response.ok) {
      const body = await response.json().catch(() => null);
      const error = body?.error;
      throw new ApiError(
        response.status,
        error?.code ?? 'http-error',
        error?.message ?? translate(session.language, 'requestFailed', { status: response.status }),
        error?.detail,
      );
    }
    return response;
  }
  async function json<T>(path: string, init?: RequestInit): Promise<T> {
    return (await request(path, init)).json();
  }
  const post = <T>(path: string, body: unknown = {}) =>
    json<T>(path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  const put = <T>(path: string, body: unknown) =>
    json<T>(path, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  return {
    session,
    context: () => json<ViewSnapshot>(`/views/${session.viewId}`),
    status: () => json<StatusSnapshot>('/status'),
    documents: () => json<DocumentState[]>('/documents'),
    history: async (id: string) =>
      (await json<Version[]>(`/documents/${id}/versions`)).sort((a, b) => b.seq - a.seq),
    selectDocument: (documentId: string | null) =>
      put<ViewSnapshot>(`/views/${session.viewId}/document`, { documentId }),
    selection: (body: Selection) => put<ViewSnapshot>(`/views/${session.viewId}/selection`, body),
    locate: (body: LocateArgs) => post<LocateOutcome>(`/views/${session.viewId}/locate`, body),
    geometry: (key: string) => json<RenderGeometry>(`/renders/${key}/geometry`),
    page: async (key: string, page: number) =>
      (await request(`/renders/${key}/pages/${page}`)).blob(),
    renderVersion: (id: string, version: string) =>
      post<Job>(`/documents/${id}/versions/${version}/render`),
    edit: (args: EditArgs) => post<null>(`/documents/${args.documentId}/edits`, args),
    restore: (args: RestoreArgs) => post<Job>(`/documents/${args.documentId}/restore`, args),
    export: (args: ExportArgs) => post<Job>(`/documents/${args.documentId}/export`, args),
    reveal: (id: string) => post<{ opened: boolean }>(`/jobs/${id}/reveal`),
    job: (id: string) => json<Job>(`/jobs/${id}`),
    cancel: (id: string) => post<Job>(`/jobs/${id}/cancel`),
    import: (file: File, metadata: Omit<ImportArgs, 'path'>) => {
      const form = new FormData();
      form.append('metadata', JSON.stringify(metadata));
      form.append('file', file);
      return json<Job>('/documents', { method: 'POST', body: form });
    },
    async waitJob<T>(initial: Job): Promise<T> {
      let job = initial;
      while (job.state === 'queued' || job.state === 'running') {
        await delay(200, signal);
        job = await json<Job>(`/jobs/${job.jobId}`);
      }
      if (job.state !== 'done')
        throw new ApiError(
          422,
          job.error?.code ?? job.state,
          job.error?.message ??
            translate(session.language, 'jobState', {
              state: translate(session.language, jobStateKey(job.state)),
            }),
          job.error?.detail,
        );
      return job.result as T;
    },
    async events(snapshot: ViewSnapshot, onEvent: (event: RuntimeEvent) => void) {
      const query = new URLSearchParams({
        instanceId: snapshot.instanceId,
        after: String(snapshot.seq),
      });
      const response = await request(`/events?${query}`);
      if (!response.body) throw new Error(translate(session.language, 'eventsUnsupported'));
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      try {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          let boundary: RegExpExecArray | null;
          while ((boundary = /\r?\n\r?\n/.exec(buffer))) {
            const frame = buffer.slice(0, boundary.index);
            buffer = buffer.slice(boundary.index + boundary[0].length);
            let kind = '';
            const data: string[] = [];
            for (const line of frame.split(/\r?\n/)) {
              if (line.startsWith('event:')) kind = line.slice(6).trim();
              if (line.startsWith('data:')) data.push(line.slice(5).replace(/^ /, ''));
            }
            if (kind === 'session-expired')
              throw new ApiError(
                401,
                'unauthorized',
                translate(session.language, 'sessionExpired'),
              );
            if (kind === 'resync-required') return;
            if (kind === 'runtime' && data.length) onEvent(JSON.parse(data.join('\n')));
          }
          if (buffer.length > 1024 * 1024)
            throw new Error(translate(session.language, 'eventTooLarge'));
        }
      } finally {
        await reader.cancel().catch(() => {});
        reader.releaseLock();
      }
    },
  };
}
export type Api = ReturnType<typeof makeApi>;
export const locatorKey = (l: Locator) =>
  `${l.staffIndex}:${l.measureStart}:${l.measureEnd}:${l.voice ?? ''}`;
export function delay(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(signal.reason);
      return;
    }
    const abort = () => {
      clearTimeout(timer);
      reject(signal?.reason);
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', abort);
      resolve();
    }, ms);
    signal?.addEventListener('abort', abort, { once: true });
  });
}

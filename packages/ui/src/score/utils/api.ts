import type {
  AgentStatus,
  DocumentPage,
  Drawing,
  DocumentSummary,
  EditArgs,
  ImportArgs,
  Job,
  JobOutcome,
  LocateArgs,
  LocateOutcome,
  Locator,
  SelectionTarget,
  OutputArgs,
  RenderGeometry,
  ScorePlayback,
  ScoreSettings,
  RestoreArgs,
  RuntimeEvent,
  Selection,
  Version,
  VersionPage,
  DocumentContext,
  UiLanguage,
} from '../../generated/protocol.ts';
import { jobStateKey, translate } from '../../i18n.tsx';
import { createUuid } from './uuid.ts';

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
export function makeApi(language: () => UiLanguage, signal?: AbortSignal) {
  const clientId = createUuid();
  async function request(path: string, init: RequestInit = {}) {
    const response = await fetch(`/api/v1${path}`, {
      ...init,
      signal: init.signal ?? signal,
    });
    if (!response.ok) {
      const body = await response.json().catch(() => null);
      const error = body?.error;
      throw new ApiError(
        response.status,
        error?.code ?? 'http-error',
        error?.message ?? translate(language(), 'requestFailed', { status: response.status }),
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
  return {
    clientId,
    context: (documentId: string) => json<DocumentContext>(`/documents/${documentId}/context`),
    drawing: (documentId: string) => json<Drawing>(`/documents/${documentId}/drawing`),
    scoreSettings: (documentId: string) => json<ScoreSettings>(`/documents/${documentId}/settings`),
    saveScoreSettings: (documentId: string, settings: ScoreSettings) =>
      json<ScoreSettings>(`/documents/${documentId}/settings`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
      }),
    saveDrawing: (documentId: string, drawing: Drawing) =>
      json<Drawing>(`/documents/${documentId}/drawing`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(drawing),
      }),
    setLanguage: (language: UiLanguage) =>
      json<UiLanguage>('/ui-language', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ language }),
      }),
    status: () => json<AgentStatus>('/status'),
    documents: async () => {
      const items: DocumentSummary[] = [];
      let offset = 0;
      for (;;) {
        const page = await json<DocumentPage>(`/documents?limit=200&offset=${offset}`);
        items.push(...page.items);
        if (page.nextOffset === null) return items;
        offset = page.nextOffset;
      }
    },
    history: async (id: string) => {
      const items: Version[] = [];
      let offset = 0;
      for (;;) {
        const page = await json<VersionPage>(
          `/documents/${id}/versions?limit=200&offset=${offset}`,
        );
        items.push(...page.items);
        if (page.nextOffset === null) return items.sort((a, b) => b.seq - a.seq);
        offset = page.nextOffset;
      }
    },
    selection: (documentId: string, body: Selection) =>
      json<DocumentContext>(`/documents/${documentId}/selection`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          'X-Ossia-Client-Id': clientId,
        },
        body: JSON.stringify(body),
      }),
    locate: (documentId: string, body: LocateArgs) =>
      post<LocateOutcome>(`/documents/${documentId}/locate`, body),
    geometry: (key: string) => json<RenderGeometry>(`/renders/${key}/geometry`),
    playback: (key: string) => json<ScorePlayback>(`/renders/${key}/playback`),
    page: async (key: string, page: number, pageSignal?: AbortSignal) =>
      (
        await request(`/renders/${key}/pages/${page}`, {
          signal: pageSignal && signal ? AbortSignal.any([pageSignal, signal]) : pageSignal,
        })
      ).blob(),
    renderVersion: (id: string, version: string) =>
      post<Job>(`/documents/${id}/versions/${version}/render`),
    edit: (args: EditArgs) => post<null>(`/documents/${args.documentId}/edits`, args),
    restore: (documentId: string, args: RestoreArgs) =>
      post<Job>(`/documents/${documentId}/restore`, args),
    output: (args: OutputArgs) => post<Job>('/outputs', args),
    outputFile: async (jobId: string) => (await request(`/jobs/${jobId}/output`)).blob(),
    outputBatchFile: async (jobId: string, index: number) =>
      (await request(`/jobs/${jobId}/outputs/${index}`)).blob(),
    outputDownloadUrl: (jobId: string, fileName: string) =>
      `/api/v1/jobs/${encodeURIComponent(jobId)}/output?${new URLSearchParams({ filename: fileName })}`,
    reveal: (id: string) => post<{ opened: boolean }>(`/jobs/${id}/reveal`),
    job: (id: string) => json<JobOutcome>(`/jobs/${id}`),
    cancel: (id: string) => post<JobOutcome>(`/jobs/${id}/cancel`),
    import: (file: File, metadata: Omit<ImportArgs, 'path'>) => {
      const form = new FormData();
      form.append('metadata', JSON.stringify(metadata));
      form.append('file', file);
      return json<Job>('/documents', { method: 'POST', body: form });
    },
    async waitJob<T>(initial: Job, onUpdate?: (outcome: JobOutcome) => void): Promise<T> {
      let job = initial;
      while (job.state === 'queued' || job.state === 'running') {
        await delay(200, signal);
        const outcome = await json<JobOutcome>(`/jobs/${job.jobId}`);
        job = outcome.job;
        onUpdate?.(outcome);
      }
      if (job.state !== 'done')
        throw new ApiError(
          422,
          job.error?.code ?? job.state,
          job.error?.message ??
            translate(language(), 'jobState', {
              state: translate(language(), jobStateKey(job.state)),
            }),
          job.error?.detail,
        );
      return job.result as T;
    },
    async events(
      snapshot: { instanceId: string; seq: number },
      onEvent: (event: RuntimeEvent) => void,
    ) {
      const query = new URLSearchParams({
        instanceId: snapshot.instanceId,
        after: String(snapshot.seq),
      });
      const response = await request(`/events?${query}`);
      if (!response.body) throw new Error(translate(language(), 'eventsUnsupported'));
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
            if (kind === 'resync-required') return;
            if (kind === 'runtime' && data.length) onEvent(JSON.parse(data.join('\n')));
          }
          if (buffer.length > 1024 * 1024) throw new Error(translate(language(), 'eventTooLarge'));
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
export const selectionKey = (target: SelectionTarget) =>
  target.kind === 'measure'
    ? `measure:${locatorKey(target.locator)}`
    : `element:${target.elementId}`;
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

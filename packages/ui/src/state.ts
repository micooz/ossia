// The app's state container: one reducer, no framework beyond React. It exists so the *chain*
// (import -> render -> select -> edit -> accept/restore) is a readable sequence of transitions and
// so a re-render of one version never races another.
import type {
  Api,
  DocumentDetail,
  EditResultView,
  ImportResult,
  Located,
  Locator,
  VersionPayload,
} from './api.ts';

export type Phase =
  | { kind: 'booting' }
  | { kind: 'no-documents' }
  | { kind: 'importing' }
  | { kind: 'needs-confirmation'; report: ImportResult | null; error: unknown }
  | { kind: 'viewing'; doc: DocumentDetail; version: VersionPayload }
  | { kind: 'rendering'; doc: DocumentDetail; hash: string; progress: number }
  | { kind: 'editing'; doc: DocumentDetail; version: VersionPayload; jobPhase: string }
  | { kind: 'comparing'; doc: DocumentDetail; result: EditResultView }
  | { kind: 'error'; message: string; detail?: unknown };

export interface Selection {
  locators: Locator[];
  /** Locators resolved on the server, kept for the overlay geometry. */
  hits: Located[];
}

export interface AppState {
  phase: Phase;
  selection: Selection;
  lastEdit: EditResultView | null;
  busy: boolean;
}

export const initialState: AppState = {
  phase: { kind: 'booting' },
  selection: { locators: [], hits: [] },
  lastEdit: null,
  busy: false,
};

export type Action =
  | { type: 'phase'; phase: Phase }
  | { type: 'busy'; busy: boolean }
  | { type: 'select'; selection: Selection }
  | { type: 'clear-selection' }
  | { type: 'edit-result'; result: EditResultView; doc: DocumentDetail };

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case 'phase':
      return { ...state, phase: action.phase };
    case 'busy':
      return { ...state, busy: action.busy };
    case 'select':
      return { ...state, selection: action.selection };
    case 'clear-selection':
      return { ...state, selection: { locators: [], hits: [] } };
    case 'edit-result':
      return {
        ...state,
        lastEdit: action.result,
        phase: { kind: 'comparing', doc: action.doc, result: action.result },
        selection: { locators: [], hits: [] },
        busy: false,
      };
  }
}

/**
 * Ask the server for the measure a clicked column holds, and merge it into the selection. Clicking
 * the same measure again removes it, which is how a player narrows a mistaken multi-selection.
 */
export function toggleMeasure(selection: Selection, hit: Located, staff: number): Selection {
  const key = `${staff}:${hit.measure}`;
  const already = selection.locators.some((l) => `${l.staff}:${l.measureStart}` === key);
  if (already) {
    return {
      locators: selection.locators.filter((l) => `${l.staff}:${l.measureStart}` !== key),
      hits: selection.hits.filter((h) => `${h.staff}:${h.measure}` !== key),
    };
  }
  return {
    locators: [...selection.locators, { staff, measureStart: hit.measure }],
    hits: [...selection.hits, hit],
  };
}

/**
 * Ensure one version is rendered, following the job to completion. `rendered` short-circuits when
 * the server already has the pages, which is the common case after the first view of a version.
 */
export async function ensureRendered(
  api: Api,
  docId: string,
  hash: string,
  engineTag: string,
  onProgress: (p: number) => void,
): Promise<VersionPayload> {
  const started = await api.render(docId, hash);
  if (started.done) return started.done;
  if (started.jobId) {
    await api.waitJob(started.jobId, (j) => onProgress(j.progress));
    const got = await api.version(docId, hash);
    if ('pending' in got) throw new Error('render finished but the version is still pending');
    return got;
  }
  // No job and no payload: ask for a forced render rather than spinning.
  const forced = await api.render(docId, hash, true);
  if (forced.done) return forced.done;
  if (!forced.jobId) throw new Error('the server refused to render this version');
  await api.waitJob(forced.jobId, (j) => onProgress(j.progress));
  const got = await api.version(docId, hash);
  if ('pending' in got) throw new Error(`render of ${hash} (${engineTag}) produced nothing`);
  return got;
}

/**
 * Fold a freshly rendered payload back into the chain snapshot.
 *
 * The detail we hold was read *before* the render, so the node we just opened still carries
 * `preview: null`: the history row would say `未渲染` while the score bar two columns over reports
 * the render time of that very version. The server only reports a preview for pages it rendered in
 * this process, so the client has to remember what it just made.
 */
export function withPreview(
  detail: DocumentDetail,
  hash: string,
  payload: VersionPayload,
): DocumentDetail {
  return {
    ...detail,
    versions: detail.versions.map((v) =>
      v.hash !== hash || v.preview
        ? v
        : {
            ...v,
            preview: {
              pageCount: payload.pageCount,
              ms: payload.renderMs,
              svgUrl: payload.pages[0]?.svgUrl ?? '',
            },
          },
    ),
  };
}

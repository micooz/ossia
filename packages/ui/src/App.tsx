// The chain, wired: documents -> version -> rendered pages -> click to select -> NL edit -> five
// gates -> before/after -> accept or restore -> history. Each step talks to the server through the
// API client; the `.ly` never appears here, because it never leaves the server (G6).
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';

import type { Api, DocumentDetail, EditResultView, ImportResult } from './api.ts';
import { ApiError } from './api.ts';
import { CompareView } from './components/CompareView.tsx';
import { EditPanel } from './components/EditPanel.tsx';
import { ExportBar } from './components/ExportBar.tsx';
import { HistoryPanel } from './components/HistoryPanel.tsx';
import { ImportPanel } from './components/ImportPanel.tsx';
import { ScoreView } from './components/ScoreView.tsx';
import { ensureRendered, initialState, reducer, toggleMeasure, withPreview } from './state.ts';

interface Props {
  api: Api;
  source: string;
}

export function App({ api, source }: Props) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const [documents, setDocuments] = useState<DocumentDetail[]>([]);
  const [toast, setToast] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
    },
    [],
  );

  const doc =
    state.phase.kind === 'viewing' ||
    state.phase.kind === 'comparing' ||
    state.phase.kind === 'rendering' ||
    state.phase.kind === 'editing'
      ? state.phase.doc
      : null;
  const version =
    state.phase.kind === 'viewing'
      ? state.phase.version
      : state.phase.kind === 'editing'
        ? state.phase.version
        : null;

  const say = useCallback((m: string) => {
    if (!mounted.current) return;
    setToast(m);
    setTimeout(() => mounted.current && setToast(null), 4000);
  }, []);

  const loadDocuments = useCallback(async () => {
    const { documents: list } = await api.listDocuments();
    if (!mounted.current) return null;
    setDocuments((prev) => {
      const known = new Map(prev.map((d) => [d.document.id, d]));
      return list.map(
        (s) =>
          known.get(s.id) ?? {
            document: { ...s, baseline: false, createdAt: s.createdAt, updatedAt: s.updatedAt },
            versions: [],
          },
      );
    });
    return list;
  }, [api]);

  useEffect(() => {
    let live = true;
    api
      .listDocuments()
      .then((r) => {
        if (!live) return;
        setDocuments(
          r.documents.map((s) => ({
            document: { ...s, baseline: false, createdAt: s.createdAt, updatedAt: s.updatedAt },
            versions: [],
          })),
        );
        const first = r.documents[0];
        if (!first) {
          dispatch({ type: 'phase', phase: { kind: 'no-documents' } });
          return;
        }
        void open(first.id, first.headHash);
      })
      .catch((e) => {
        if (live)
          dispatch({
            type: 'phase',
            phase: { kind: 'error', message: String((e as Error).message) },
          });
      });
    return () => {
      live = false;
    };
    // `open` is stable and the boot must happen once per endpoint.
  }, [api]);

  /** Load one document's chain and make sure one version is rendered. */
  const open = useCallback(
    async (docId: string, hash: string) => {
      dispatch({ type: 'busy', busy: true });
      try {
        const detail = await api.document(docId);
        if (!mounted.current) return;
        setDocuments((prev) => [detail, ...prev.filter((d) => d.document.id !== docId)]);
        const head = detail.document.headHash;
        const found = detail.versions.find((v) => v.hash === hash);
        if (!found) {
          say('该版本不在此文档的链上');
          return;
        }
        const meta = detail.versions.find((v) => v.hash === head) ?? found;
        dispatch({
          type: 'phase',
          phase: { kind: 'rendering', doc: detail, hash: found.hash, progress: 0 },
        });
        const payload = await ensureRendered(
          api,
          docId,
          found.hash,
          meta ? detail.document.engineTag : '',
          (p) =>
            mounted.current &&
            dispatch({
              type: 'phase',
              phase: { kind: 'rendering', doc: detail, hash: found.hash, progress: p },
            }),
        );
        if (!mounted.current) return;
        const viewed = withPreview(detail, found.hash, payload);
        setDocuments((prev) => [viewed, ...prev.filter((d) => d.document.id !== docId)]);
        dispatch({ type: 'phase', phase: { kind: 'viewing', doc: viewed, version: payload } });
        dispatch({ type: 'clear-selection' });
      } catch (e) {
        if (mounted.current)
          dispatch({
            type: 'phase',
            phase: { kind: 'error', message: String((e as Error).message) },
          });
      } finally {
        if (mounted.current) dispatch({ type: 'busy', busy: false });
      }
    },
    [api, say],
  );

  const onSelect = useCallback(
    async (page: number, x: number, y: number) => {
      if (!doc || !version) return;
      try {
        const { locator } = await api.locate(doc.document.id, version.hash, page, x, y);
        const next = toggleMeasure(state.selection, locator, locator.staff);
        if (!mounted.current) return;
        dispatch({ type: 'select', selection: next });
      } catch (e) {
        // A miss is normal (clicked between systems); it must not look like a failure.
        if (e instanceof ApiError && e.code === 'no-hit') {
          say('请点击谱表上的小节');
          return;
        }
        say(String((e as Error).message));
      }
    },
    [api, doc, version, state.selection, say],
  );

  const onEditJob = useCallback(
    async (jobId: string) => {
      if (!doc || !version) return;
      dispatch({ type: 'phase', phase: { kind: 'editing', doc, version, jobPhase: '排队中' } });
      try {
        const result = await api.waitJob<EditResultView>(
          jobId,
          (j) =>
            mounted.current &&
            dispatch({
              type: 'phase',
              phase: { kind: 'editing', doc, version, jobPhase: j.phase },
            }),
        );
        if (!mounted.current) return;
        dispatch({ type: 'edit-result', result, doc });
      } catch (e) {
        if (mounted.current)
          dispatch({
            type: 'phase',
            phase: { kind: 'error', message: String((e as Error).message) },
          });
      }
    },
    [api, doc, version],
  );

  // The decision moves the head on the server, so the app must view *that* hash: `doc` here is the
  // pre-decision snapshot, and reopening its headHash would leave the score showing the old version
  // while the history panel already marked the new node as current.
  const afterDecision = useCallback(
    async (headHash: string) => {
      if (!doc) return;
      await open(doc.document.id, headHash);
      void loadDocuments();
    },
    [doc, open, loadDocuments],
  );

  const restore = useCallback(
    async (targetHash: string) => {
      if (!doc) return;
      dispatch({ type: 'busy', busy: true });
      try {
        const r = await api.restore(doc.document.id, doc.document.headHash, targetHash);
        await api.waitJob(r.jobId);
        await open(doc.document.id, r.version.hash);
        say(`已回退到版本 #${r.version.seq}（作为新版本重新入库）`);
        void loadDocuments();
      } catch (e) {
        say(String((e as Error).message));
        dispatch({ type: 'busy', busy: false });
      }
    },
    [api, doc, open, say, loadDocuments],
  );

  const selectedKeys = useMemo(
    () => new Set(state.selection.locators.map((l) => `${l.staff}:${l.measureStart}`)),
    [state.selection],
  );

  const imported = useCallback(
    (r: ImportResult) => {
      say(`已导入 ${r.document.title}：${r.stats.measures} 小节 / ${r.stats.voices} 声部`);
      void open(r.document.id, r.version.hash);
      void loadDocuments();
    },
    [open, say, loadDocuments],
  );

  if (state.phase.kind === 'error') {
    return (
      <main className="app">
        <div className="panel">
          <h3>无法继续</h3>
          <p className="error">{state.phase.message}</p>
          <button
            type="button"
            onClick={() =>
              void api
                .health()
                .then(() => dispatch({ type: 'phase', phase: { kind: 'no-documents' } }))
            }
          >
            重试连接
          </button>
        </div>
      </main>
    );
  }

  return (
    <main className="app">
      <header className="bar">
        <h1>Ossia</h1>
        <select
          aria-label="文档"
          value={doc?.document.id ?? ''}
          onChange={(ev) => {
            const d = documents.find((x) => x.document.id === ev.target.value);
            if (d) void open(d.document.id, d.document.headHash);
          }}
        >
          <option value="">{doc ? doc.document.title : '（无文档）'}</option>
          {documents.map((d) => (
            <option key={d.document.id} value={d.document.id}>
              {d.document.title}
            </option>
          ))}
        </select>
        {version && (
          <ExportBar
            api={api}
            docId={doc!.document.id}
            title={doc!.document.title}
            version={version}
          />
        )}
        <span className="hint endpoint">{source}</span>
      </header>

      <div className="cols">
        <aside className="left">
          <ImportPanel
            api={api}
            onImported={imported}
            onBusy={(b) => dispatch({ type: 'busy', busy: b })}
          />
          {doc && (
            <HistoryPanel
              doc={doc}
              currentHash={version?.hash ?? doc.document.headHash}
              busy={state.busy}
              onOpen={(h) => void open(doc.document.id, h)}
              onRestore={(h) => void restore(h)}
            />
          )}
        </aside>

        <div className="middle">
          {state.phase.kind === 'comparing' && (
            <CompareView
              api={api}
              docId={state.phase.doc.document.id}
              result={state.phase.result}
              onDecided={(h) => void afterDecision(h)}
              onBusy={(b) => dispatch({ type: 'busy', busy: b })}
            />
          )}
          {state.phase.kind === 'rendering' && (
            <div className="progress" role="status">
              <label>渲染中 {Math.round(state.phase.progress * 100)}%</label>
              <progress value={state.phase.progress} max={1} />
            </div>
          )}
          {state.phase.kind === 'editing' && (
            <div className="progress" role="status">
              <label>AI 处理中：{state.phase.jobPhase}</label>
              <div className="spinner" />
            </div>
          )}
          {version && (
            <ScoreView
              api={api}
              version={version}
              selected={selectedKeys}
              busy={state.busy}
              onSelect={(p, x, y) => void onSelect(p, x, y)}
            />
          )}
          {!version && state.phase.kind !== 'comparing' && (
            <p className="hint">导入一个 MusicXML / MIDI 文件开始。</p>
          )}
          {state.selection.hits.length > 0 && version && (
            <p className="hint selection-line">
              选中：
              {state.selection.hits
                .map((h) => `谱表${h.staff + 1} 第 ${h.measure} 小节（${h.voice ?? '?'}）`)
                .join('、')}
            </p>
          )}
        </div>

        <aside className="right">
          {doc && version && state.phase.kind !== 'comparing' ? (
            <EditPanel
              api={api}
              docId={doc.document.id}
              version={version}
              locators={state.selection.locators}
              busy={state.busy}
              onResult={(j) => void onEditJob(j)}
              onBusy={(b) => dispatch({ type: 'busy', busy: b })}
            />
          ) : (
            <p className="hint">选择或导入一个文档后可用。</p>
          )}
        </aside>
      </div>

      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
    </main>
  );
}

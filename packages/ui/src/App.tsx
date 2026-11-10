import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';

import { ApiError, delay, describe, locatorKey, makeApi } from './api.ts';
import { CompareView, VersionScore } from './components/CompareView.tsx';
import { ExportBar } from './components/ExportBar.tsx';
import { HistoryPanel } from './components/HistoryPanel.tsx';
import { ImportPanel } from './components/ImportPanel.tsx';
import { ScoreView } from './components/ScoreView.tsx';
import { forgetSession } from './endpoint.ts';
import type { Session } from './endpoint.ts';
import type {
  DocumentState,
  Locator,
  RenderManifest,
  Version,
  ViewSnapshot,
} from './generated/protocol.ts';
import { initialState, reducer } from './state.ts';

export function App({ session }: { session: Session }) {
  const lifetime = useMemo(() => new AbortController(), [session]);
  const api = useMemo(() => makeApi(session, lifetime.signal), [session, lifetime]);
  const [state, dispatch] = useReducer(reducer, initialState);
  const viewRef = useRef<ViewSnapshot | null>(null);
  const serial = useRef(0);
  const [working, setWorking] = useState(false);
  const selecting = useRef(false);
  const [notice, setNotice] = useState('');
  const [historyVersion, setHistoryVersion] = useState<string | null>(null);
  const [comparing, setComparing] = useState(false);
  const [confirm, setConfirm] = useState<{
    target: Version | 'discard';
    document: DocumentState;
  } | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const onError = useCallback(
    (error: unknown) => {
      if (lifetime.signal.aborted) return;
      if (error instanceof ApiError && error.status === 401) {
        forgetSession(session);
        dispatch({ type: 'connection', connection: 'expired' });
        dispatch({ type: 'error', message: '视图已失效，请让 Agent 重新打开谱面。' });
        lifetime.abort();
      } else dispatch({ type: 'error', message: describe(error) });
    },
    [lifetime, session],
  );
  const applyView = useCallback((view: ViewSnapshot) => {
    viewRef.current = view;
    dispatch({ type: 'view', view });
  }, []);
  const refresh = useCallback(async () => {
    const generation = ++serial.current;
    const view = await api.context();
    const [documents, status, versions] = await Promise.all([
      api.documents(),
      api.status(),
      view.document ? api.history(view.document.documentId) : Promise.resolve([]),
    ]);
    if (generation !== serial.current || lifetime.signal.aborted) return null;
    const previous = viewRef.current;
    if (
      previous?.document?.documentId === view.document?.documentId &&
      previous?.document?.sourceHash !== view.document?.sourceHash
    ) {
      setNotice('源码已变化，旧选区已失效，请在最新谱面上重新选择。');
    }
    viewRef.current = view;
    dispatch({ type: 'sync', view, documents, status, versions });
    return view;
  }, [api, lifetime]);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let refreshing = false;
    let again = false;
    const requestRefresh = () => {
      again = true;
      if (timer || refreshing) return;
      timer = setTimeout(() => {
        timer = undefined;
        refreshing = true;
        void (async () => {
          try {
            do {
              again = false;
              await refresh();
            } while (again && !lifetime.signal.aborted);
          } catch (e) {
            onError(e);
          } finally {
            refreshing = false;
          }
        })();
      }, 40);
    };
    void (async () => {
      while (!lifetime.signal.aborted) {
        try {
          const snapshot = await refresh();
          if (!snapshot) {
            await delay(100, lifetime.signal);
            continue;
          }
          await api.events(snapshot, requestRefresh);
          if (!lifetime.signal.aborted)
            dispatch({ type: 'connection', connection: 'reconnecting' });
        } catch (e) {
          onError(e);
          if (!lifetime.signal.aborted)
            dispatch({ type: 'connection', connection: 'reconnecting' });
        }
        await delay(800, lifetime.signal).catch(() => {});
      }
    })();
    return () => {
      if (timer) clearTimeout(timer);
      lifetime.abort();
    };
  }, [api, lifetime, refresh, onError]);
  const doc = state.view?.document;
  const documentId = doc?.documentId;
  useEffect(() => {
    setHistoryVersion(null);
    setComparing(false);
    setConfirm(null);
    setNotice('');
  }, [documentId]);
  useEffect(() => {
    if (confirm) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirm]);
  async function operate(action: () => Promise<unknown>) {
    setWorking(true);
    dispatch({ type: 'error', message: null });
    try {
      await action();
      await refresh();
    } catch (e) {
      onError(e);
      await refresh().catch(onError);
    } finally {
      setWorking(false);
    }
  }
  async function openDocument(id: string) {
    ++serial.current;
    setNotice('');
    await operate(async () => {
      applyView(await api.selectDocument(id));
      setHistoryVersion(null);
      setComparing(false);
    });
  }
  async function choose(locator?: Locator, point?: { page: number; x: number; y: number }) {
    const view = viewRef.current;
    const document = view?.document;
    if (!view || !document || selecting.current || state.connection !== 'connected') return;
    const hash = document.sourceHash;
    if (view.render?.manifest.sourceHash !== hash) return;
    selecting.current = true;
    try {
      let chosen = locator;
      if (point) {
        const result = await api.locate({ sourceHash: hash, ...point });
        if (result.capability !== 'measure' || !result.region) {
          if (result.reason && result.reason !== 'no-hit')
            setNotice(`此处无法选择小节：${result.reason}`);
          return;
        }
        chosen = result.region.locator;
      }
      const current = viewRef.current;
      if (
        current?.document?.documentId !== document.documentId ||
        current.document.sourceHash !== hash ||
        current.render?.renderKey !== view.render?.renderKey
      )
        return;
      const selected = current.selection?.locators ?? [];
      const key = chosen && locatorKey(chosen);
      const locators = !chosen
        ? []
        : selected.some((l) => locatorKey(l) === key)
          ? selected.filter((l) => locatorKey(l) !== key)
          : [...selected, chosen];
      const result = await api.selection({
        viewId: session.viewId,
        documentId: document.documentId,
        sourceHash: hash,
        locators,
      });
      if (
        viewRef.current?.document?.documentId === document.documentId &&
        viewRef.current.document.sourceHash === hash
      ) {
        ++serial.current;
        applyView(result);
      }
    } catch (e) {
      onError(e);
      await refresh().catch(onError);
    } finally {
      selecting.current = false;
    }
  }
  function requestConfirmation(target: Version | 'discard') {
    const document = viewRef.current?.document;
    if (document) setConfirm({ target, document });
  }
  async function destructive() {
    const pending = confirm;
    setConfirm(null);
    if (!pending) return;
    const action = pending.target;
    const current = pending.document;
    await operate(async () => {
      if (action === 'discard') {
        await api.edit({
          action: 'discard',
          documentId: current.documentId,
          viewId: session.viewId,
          baseVersionId: current.headVersionId,
          expectedSourceHash: current.sourceHash,
          editId: null,
          adoptWorkspace: false,
          author: 'user',
        });
      } else {
        const job = await api.restore({
          documentId: current.documentId,
          versionId: action.versionId,
          baseVersionId: current.headVersionId,
          expectedSourceHash: current.sourceHash,
          author: 'user',
          summary: `恢复至版本 ${action.seq}`,
        });
        await api.waitJob(job);
        setHistoryVersion(null);
        setNotice(`已追加恢复版本（来源：版本 ${action.seq}）。`);
      }
    });
  }
  if (state.connection === 'expired')
    return (
      <main className="start panel">
        <h1>视图已失效</h1>
        <p role="alert">{state.error}</p>
        <p>服务重启或凭据过期后，需要新的打开链接。工作区文件会保留。</p>
      </main>
    );
  const render = state.view?.render;
  const stale = !!doc && !!render && doc.sourceHash !== render.manifest.sourceHash;
  const jobs =
    state.status?.jobs.filter(
      (j) => j.documentId === documentId && ['queued', 'running'].includes(j.state),
    ) ?? [];
  const selected = state.view?.selection?.locators ?? [];
  const connected = state.connection === 'connected';
  return (
    <main className="app">
      <header className="bar">
        <h1>Ossia</h1>
        <label className="document-switch">
          乐谱{' '}
          <select
            aria-label="当前乐谱"
            value={documentId ?? ''}
            disabled={!connected || working}
            onChange={(e) => void openDocument(e.target.value)}
          >
            <option value="" disabled>
              选择乐谱
            </option>
            {state.documents.map((d) => (
              <option key={d.documentId} value={d.documentId}>
                {d.title}
              </option>
            ))}
          </select>
        </label>
        <span className="connection" role="status">
          {connected
            ? '本地服务已连接'
            : state.connection === 'connecting'
              ? '正在连接…'
              : '连接中断，正在重连…'}
        </span>
      </header>
      {state.error && (
        <div className="message error" role="alert">
          {state.error}
          <button type="button" onClick={() => dispatch({ type: 'error', message: null })}>
            关闭提示
          </button>
        </div>
      )}
      {notice && (
        <p className="message" role="status">
          {notice}
        </p>
      )}
      <div className="workspace">
        <aside className="library-column">
          <ImportPanel api={api} onImported={(id) => void openDocument(id)} onError={onError} />
          {doc && (
            <HistoryPanel
              versions={state.versions}
              head={doc.headVersionId}
              selected={historyVersion}
              busy={working || !connected}
              onSelect={(id) => {
                setHistoryVersion(id);
                setComparing(false);
              }}
              onRestore={requestConfirmation}
            />
          )}
        </aside>
        <div className="score-column">
          {!doc ? (
            <section className="panel empty">
              <h2>{connected ? '选择或导入一份乐谱' : '正在连接本地乐谱库'}</h2>
              <p>在这里查看谱面、选择小节；修改任务由你的 Agent 完成。</p>
            </section>
          ) : (
            <>
              <section className="panel document-status">
                <h2>{doc.title}</h2>
                <div className="row">
                  <span className={doc.dirty ? 'warn' : 'hint'}>
                    {doc.dirty ? '工作区有未提交修改' : '工作区与已提交版本一致'}
                  </span>
                  {state.view?.editActivity && (
                    <span>
                      编辑者：{state.view.editActivity.author} ·{' '}
                      {state.view.editActivity.ownedByView ? '当前视图' : '其他视图'}
                    </span>
                  )}
                  {doc.editState === 'active' && <span>编辑任务占用中</span>}
                  {['interrupted', 'recovery-conflict'].includes(doc.editState) && (
                    <span className="warn">待恢复：请让 Agent 检查工作区并显式恢复编辑。</span>
                  )}
                </div>
                {jobs.length > 0 && (
                  <p role="status">
                    正在处理：{jobs.map((j) => `${j.kind} · ${j.phase}`).join('；')}
                    <span className="hash"> · {doc.sourceHash.slice(0, 12)}</span>
                  </p>
                )}
                <p className="hint hash">
                  源码 {doc.sourceHash.slice(0, 12)}
                  {render ? ` · 谱面 ${render.manifest.sourceHash.slice(0, 12)}` : ''}
                </p>
                <div className="row">
                  <button
                    type="button"
                    aria-pressed={!historyVersion && !comparing}
                    onClick={() => {
                      setHistoryVersion(null);
                      setComparing(false);
                    }}
                  >
                    工作区谱面
                  </button>
                  <button
                    type="button"
                    disabled={state.versions.length < 2}
                    aria-pressed={comparing}
                    onClick={() => {
                      setComparing(!comparing);
                      setHistoryVersion(null);
                    }}
                  >
                    版本对比
                  </button>
                  {doc.dirty && (
                    <button
                      type="button"
                      disabled={!connected || working}
                      onClick={() => requestConfirmation('discard')}
                    >
                      丢弃工作区修改
                    </button>
                  )}
                </div>
              </section>
              {comparing ? (
                <CompareView
                  key={doc.documentId}
                  api={api}
                  documentId={doc.documentId}
                  versions={state.versions}
                />
              ) : historyVersion ? (
                <>
                  <p className="message">
                    正在浏览版本 {state.versions.find((v) => v.versionId === historyVersion)?.seq}
                    ，此谱面仅供查看。
                  </p>
                  <VersionScore api={api} documentId={doc.documentId} versionId={historyVersion} />
                </>
              ) : (
                <>
                  {stale && (
                    <p className="message warn" role="status">
                      上次成功结果：当前源码尚无成功谱面，旧谱面仅供查看，暂不可选择。
                    </p>
                  )}
                  {render ? (
                    <ScoreView
                      api={api}
                      render={render}
                      selected={selected}
                      selectable={connected && !stale && !working}
                      onPoint={(page, x, y) => void choose(undefined, { page, x, y })}
                      onToggle={(l) => void choose(l)}
                    />
                  ) : (
                    <section className="panel empty">
                      <h2>尚无成功谱面</h2>
                      <p>等待排版完成，或让 Agent 检查下方诊断。</p>
                    </section>
                  )}
                </>
              )}
            </>
          )}
        </div>
        {doc && (
          <aside className="details-column">
            <section className="panel selection">
              <h2>当前选区</h2>
              <p className="hint">选择会关联到当前视图，由 Agent 读取。</p>
              {!selected.length ? (
                <p>尚未选择小节</p>
              ) : (
                <ul>
                  {selected.map((l) => (
                    <li key={locatorKey(l)}>
                      谱表 {l.staffIndex + 1} · 第 {l.measureStart}
                      {l.measureEnd !== l.measureStart ? `–${l.measureEnd}` : ''} 小节
                    </li>
                  ))}
                </ul>
              )}
              <button
                type="button"
                disabled={!selected.length || !connected || stale}
                onClick={() => void choose()}
              >
                清空选区
              </button>
            </section>
            <section className="panel diagnostics">
              <h2>编译诊断</h2>
              {doc.lastCheck?.error && (
                <p className="error" role="alert">
                  {doc.lastCheck.error.message}
                </p>
              )}
              {(() => {
                const result = doc.lastCheck?.result as RenderManifest | null;
                const diagnostics =
                  result?.diagnostics ?? (stale ? [] : (render?.manifest.diagnostics ?? []));
                const issues = diagnostics.filter((d) => d.severity !== 'info');
                const logs = diagnostics.filter((d) => d.severity === 'info');
                return (
                  <>
                    {issues.length ? (
                      <ul>
                        {issues.map((d, i) => (
                          <li key={i}>
                            {d.position && `第 ${d.position.line} 行，第 ${d.position.column} 列：`}
                            {d.message}
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="hint">
                        {doc.lastCheck?.state === 'error'
                          ? '请让 Agent 检查源码和任务详情。'
                          : '暂无编译警告或错误'}
                      </p>
                    )}
                    {logs.length > 0 && (
                      <details>
                        <summary>查看编译日志</summary>
                        <ul>
                          {logs.map((d, i) => (
                            <li key={i}>{d.message}</li>
                          ))}
                        </ul>
                      </details>
                    )}
                  </>
                );
              })()}
            </section>
            {!!state.versions.length && (
              <ExportBar
                key={doc.documentId + (doc.headVersionId ?? '')}
                api={api}
                documentId={doc.documentId}
                versions={state.versions}
                onError={onError}
              />
            )}
          </aside>
        )}
      </div>
      <dialog ref={dialog} onCancel={() => setConfirm(null)} aria-labelledby="confirm-title">
        <h2 id="confirm-title">
          {confirm?.target === 'discard'
            ? '丢弃未提交修改？'
            : `恢复至版本 ${confirm?.target.seq ?? ''}？`}
        </h2>
        <p>
          {confirm?.target === 'discard'
            ? '公共源码将恢复为当前已提交版本。'
            : '将追加一个恢复版本。工作区存在未提交修改或编辑占用时，核心会拒绝操作。'}
        </p>
        <div className="row">
          <button type="button" autoFocus onClick={() => setConfirm(null)}>
            取消
          </button>
          <button type="button" onClick={() => void destructive()}>
            确认{confirm?.target === 'discard' ? '丢弃' : '恢复'}
          </button>
        </div>
      </dialog>
    </main>
  );
}

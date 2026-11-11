import { GitCompare, Music2, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';

import { describe } from '../errors.ts';
import type {
  DocumentState,
  Locator,
  UiLanguage,
  Version,
  ViewSnapshot,
} from '../generated/protocol.ts';
import {
  I18nProvider,
  jobKindKey,
  jobStateKey,
  matchSystemLanguage,
  phaseKey,
  translate,
  useI18n,
} from '../i18n.tsx';
import { ApiError, delay, locatorKey, makeApi } from './api.ts';
import { CompareView, VersionScore } from './components/CompareView.tsx';
import { DocumentDetails } from './components/DocumentDetails.tsx';
import { HistoryPanel } from './components/HistoryPanel.tsx';
import { ImportPanel } from './components/ImportPanel.tsx';
import { ScoreView } from './components/ScoreView.tsx';
import { forgetSession } from './endpoint.ts';
import type { Session } from './endpoint.ts';
import { initialState, reducer } from './state.ts';

export function ScorePage({ session }: { session: Session }) {
  const [language, setLanguage] = useState<UiLanguage>(session.language);
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  return (
    <I18nProvider language={language}>
      <ScoreApp session={session} language={language} onLanguage={setLanguage} />
    </I18nProvider>
  );
}

function ScoreApp({
  session,
  language,
  onLanguage,
}: {
  session: Session;
  language: UiLanguage;
  onLanguage: (language: UiLanguage) => void;
}) {
  const { t } = useI18n();
  const liveSession = useMemo(() => ({ ...session }), [session]);
  const languageRef = useRef(language);
  useEffect(() => {
    liveSession.language = language;
    languageRef.current = language;
  }, [language, liveSession]);
  const lifetime = useMemo(() => new AbortController(), [session]);
  const api = useMemo(() => makeApi(liveSession, lifetime.signal), [liveSession, lifetime]);
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
        dispatch({ type: 'error', message: translate(languageRef.current, 'sessionExpired') });
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
      setNotice(translate(languageRef.current, 'sourceChanged'));
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
          await api.events(snapshot, (event) => {
            if (event.type === 'ui-language-changed') {
              const next = (event.payload as { language?: UiLanguage | null }).language;
              onLanguage(next ?? matchSystemLanguage(navigator.languages));
              return;
            }
            requestRefresh();
          });
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
  }, [api, lifetime, refresh, onError, onLanguage]);
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
            setNotice(t('cannotSelect', { reason: result.reason }));
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
          summary: t('restoreSummary', { version: action.seq }),
        });
        await api.waitJob(job);
        setHistoryVersion(null);
        setNotice(t('restoredVersion', { version: action.seq }));
      }
    });
  }
  if (state.connection === 'expired')
    return (
      <main className="start panel">
        <h1>{t('viewExpiredTitle')}</h1>
        <p role="alert">{state.error}</p>
        <p>{t('viewExpiredBody')}</p>
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
        <h1>
          <Music2 aria-hidden="true" size={20} /> {t('appName')}
        </h1>
        <label className="document-switch">
          {t('score')}{' '}
          <select
            aria-label={t('currentScore')}
            value={documentId ?? ''}
            disabled={!connected || working}
            onChange={(e) => void openDocument(e.target.value)}
          >
            <option value="" disabled>
              {t('chooseScore')}
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
            ? t('connected')
            : state.connection === 'connecting'
              ? t('connecting')
              : t('reconnecting')}
        </span>
      </header>
      {state.error && (
        <div className="message error" role="alert">
          {state.error}
          <button type="button" onClick={() => dispatch({ type: 'error', message: null })}>
            {t('dismissNotice')}
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
              <h2>{connected ? t('chooseOrImport') : t('connectingLibrary')}</h2>
              <p>{t('emptyWorkspace')}</p>
            </section>
          ) : (
            <>
              <section className="panel document-status">
                <h2>{doc.title}</h2>
                <div className="row">
                  <span className={doc.dirty ? 'warn' : 'hint'}>
                    {doc.dirty ? t('dirtyWorkspace') : t('cleanWorkspace')}
                  </span>
                  {state.view?.editActivity && (
                    <span>
                      {t('editor', { author: state.view.editActivity.author })} ·{' '}
                      {state.view.editActivity.ownedByView ? t('currentView') : t('otherView')}
                    </span>
                  )}
                  {doc.editState === 'active' && <span>{t('editOccupied')}</span>}
                  {['interrupted', 'recovery-conflict'].includes(doc.editState) && (
                    <span className="warn">{t('recoveryNeeded')}</span>
                  )}
                </div>
                {jobs.length > 0 && (
                  <p role="status">
                    {t('processing', {
                      jobs: jobs
                        .map(
                          (j) =>
                            `${t(jobKindKey(j.kind))} · ${t(phaseKey(j.phase))} (${t(
                              jobStateKey(j.state),
                            )})`,
                        )
                        .join(language === 'zh-CN' ? '；' : '; '),
                    })}
                    <span className="hash"> · {doc.sourceHash.slice(0, 12)}</span>
                  </p>
                )}
                <p className="hint hash">
                  {t('source')} {doc.sourceHash.slice(0, 12)}
                  {render ? ` · ${t('render')} ${render.manifest.sourceHash.slice(0, 12)}` : ''}
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
                    <Music2 aria-hidden="true" size={15} /> {t('workspaceScore')}
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
                    <GitCompare aria-hidden="true" size={15} /> {t('compareVersions')}
                  </button>
                  {doc.dirty && (
                    <button
                      type="button"
                      disabled={!connected || working}
                      onClick={() => requestConfirmation('discard')}
                    >
                      <Trash2 aria-hidden="true" size={15} /> {t('discardChanges')}
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
                    {t('browsingVersion', {
                      version:
                        state.versions.find((v) => v.versionId === historyVersion)?.seq ?? '',
                    })}
                  </p>
                  <VersionScore api={api} documentId={doc.documentId} versionId={historyVersion} />
                </>
              ) : (
                <>
                  {stale && (
                    <p className="message warn" role="status">
                      {t('staleRender')}
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
                      <h2>{t('noRenderTitle')}</h2>
                      <p>{t('noRenderBody')}</p>
                    </section>
                  )}
                </>
              )}
            </>
          )}
        </div>
        {doc && (
          <DocumentDetails
            api={api}
            doc={doc}
            render={render}
            versions={state.versions}
            selected={selected}
            stale={stale}
            connected={connected}
            onClear={() => void choose()}
            onError={onError}
          />
        )}
      </div>
      <dialog ref={dialog} onCancel={() => setConfirm(null)} aria-labelledby="confirm-title">
        <h2 id="confirm-title">
          {confirm?.target === 'discard'
            ? t('discardTitle')
            : t('restoreTitle', { version: confirm?.target.seq ?? '' })}
        </h2>
        <p>{confirm?.target === 'discard' ? t('discardBody') : t('restoreBody')}</p>
        <div className="row">
          <button type="button" autoFocus onClick={() => setConfirm(null)}>
            {t('cancel')}
          </button>
          <button type="button" onClick={() => void destructive()}>
            {t('confirm')}
            {confirm?.target === 'discard' ? t('confirmDiscard') : t('confirmRestore')}
          </button>
        </div>
      </dialog>
    </main>
  );
}

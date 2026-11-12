import { Circle, Music2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';

import { ToastRegion, useToasts } from '../components/Toast.tsx';
import { describe } from '../errors.ts';
import type {
  DocumentContext,
  DocumentState,
  Locator,
  UiLanguage,
  Version,
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
import { ScoreNotFound } from './components/ScoreNotFound.tsx';
import { ScoreToolbar } from './components/ScoreToolbar.tsx';
import type { ScoreLayout } from './components/ScoreToolbar.tsx';
import { ScoreView } from './components/ScoreView.tsx';
import { documentIdFromLocation } from './endpoint.ts';
import { initialState, reducer } from './state.ts';

export function ScorePage({ fallbackLanguage }: { fallbackLanguage: UiLanguage }) {
  const [language, setLanguage] = useState<UiLanguage>(fallbackLanguage);
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  return (
    <I18nProvider language={language}>
      <ScoreApp language={language} onLanguage={setLanguage} />
    </I18nProvider>
  );
}

type RestoreConfirmation = { version: Version; document: DocumentState };

function ScoreApp({
  language,
  onLanguage,
}: {
  language: UiLanguage;
  onLanguage: (language: UiLanguage) => void;
}) {
  const { t } = useI18n();
  const toast = useToasts();
  const languageRef = useRef(language);
  useEffect(() => {
    languageRef.current = language;
  }, [language]);
  const lifetime = useMemo(() => new AbortController(), []);
  const api = useMemo(() => makeApi(() => languageRef.current, lifetime.signal), [lifetime]);
  const [state, dispatch] = useReducer(reducer, initialState);
  const contextRef = useRef<DocumentContext | null>(null);
  const documentIdRef = useRef(documentIdFromLocation());
  const serial = useRef(0);
  const [working, setWorking] = useState(false);
  const selecting = useRef(false);
  const [notice, setNotice] = useState('');
  const [notFound, setNotFound] = useState(!documentIdRef.current);
  const [historyVersion, setHistoryVersion] = useState<string | null>(null);
  const [compareVersion, setCompareVersion] = useState<string | null>(null);
  const [layout, setLayout] = useState<ScoreLayout>('vertical');
  const [zoom, setZoom] = useState(100);
  const [leftZoom, setLeftZoom] = useState(100);
  const [rightZoom, setRightZoom] = useState(100);
  const [confirm, setConfirm] = useState<RestoreConfirmation | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const onError = useCallback(
    (error: unknown) => {
      if (lifetime.signal.aborted) return;
      toast.show(describe(error), 'error');
    },
    [lifetime, toast.show],
  );
  const applyContext = useCallback((context: DocumentContext) => {
    contextRef.current = context;
    dispatch({ type: 'context', context });
  }, []);
  const refresh = useCallback(async () => {
    const generation = ++serial.current;
    const documentId = documentIdRef.current;
    const status = await api.status();
    let context: DocumentContext | null = null;
    let contextError: unknown = null;
    let missing = !documentId;
    if (documentId) {
      try {
        context = await api.context(documentId);
      } catch (error) {
        if (error instanceof ApiError && (error.status === 404 || error.code === 'not-found')) {
          missing = true;
        } else {
          contextError = error;
        }
      }
    }
    const versions = context ? await api.history(context.document.documentId) : [];
    if (generation !== serial.current || lifetime.signal.aborted) return null;
    if (status.language) onLanguage(status.language);
    const previous = contextRef.current;
    if (
      previous?.document.documentId === context?.document.documentId &&
      previous?.document.sourceHash !== context?.document.sourceHash
    ) {
      setNotice(translate(languageRef.current, 'sourceChanged'));
    }
    contextRef.current = context;
    setNotFound(missing);
    dispatch({ type: 'sync', context, versions, status });
    if (contextError) {
      toast.show(describe(contextError), 'error');
    }
    return { context, status };
  }, [api, lifetime, onLanguage, toast.show]);
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
          } catch (error) {
            onError(error);
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
          const cursor = {
            instanceId: snapshot.context?.instanceId ?? snapshot.status.instanceId,
            seq: snapshot.context?.seq ?? snapshot.status.seq,
          };
          await api.events(cursor, (event) => {
            if (event.type === 'ui-language-changed') {
              const next = (event.payload as { language?: UiLanguage | null }).language;
              onLanguage(next ?? matchSystemLanguage(navigator.languages));
              return;
            }
            requestRefresh();
          });
          if (!lifetime.signal.aborted)
            dispatch({ type: 'connection', connection: 'reconnecting' });
        } catch (error) {
          onError(error);
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
  const doc = state.context?.document;
  const documentId = doc?.documentId;
  useEffect(() => {
    setHistoryVersion(null);
    setCompareVersion(null);
    setConfirm(null);
    setNotice('');
  }, [documentId]);
  useEffect(() => {
    document.title = notFound ? t('scoreNotFoundDocumentTitle') : (doc?.title ?? t('appName'));
  }, [doc?.title, language, notFound, t]);
  useEffect(() => {
    if (confirm) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirm]);
  useEffect(() => {
    if (!notice) return;
    toast.show(notice, 'info');
    setNotice('');
  }, [notice, toast.show]);
  async function operate(action: () => Promise<unknown>) {
    setWorking(true);
    try {
      await action();
      await refresh();
    } catch (error) {
      onError(error);
      await refresh().catch(onError);
    } finally {
      setWorking(false);
    }
  }
  async function choose(locator?: Locator, point?: { page: number; x: number; y: number }) {
    const context = contextRef.current;
    const document = context?.document;
    if (!context || !document || selecting.current || state.connection !== 'connected') return;
    const hash = document.sourceHash;
    if (context.render?.manifest.sourceHash !== hash) return;
    selecting.current = true;
    try {
      let chosen = locator;
      if (point) {
        const result = await api.locate(document.documentId, { sourceHash: hash, ...point });
        if (result.capability !== 'measure' || !result.region) {
          if (result.reason && result.reason !== 'no-hit')
            setNotice(t('cannotSelect', { reason: result.reason }));
          return;
        }
        chosen = result.region.locator;
      }
      const current = contextRef.current;
      if (
        current?.document.documentId !== document.documentId ||
        current.document.sourceHash !== hash ||
        current.render?.renderKey !== context.render?.renderKey
      )
        return;
      const selected = current.selection?.locators ?? [];
      const key = chosen && locatorKey(chosen);
      const locators = !chosen
        ? []
        : selected.some((locator) => locatorKey(locator) === key)
          ? selected.filter((locator) => locatorKey(locator) !== key)
          : [...selected, chosen];
      const result = await api.selection(document.documentId, {
        documentId: document.documentId,
        sourceHash: hash,
        locators,
      });
      if (
        contextRef.current?.document.documentId === document.documentId &&
        contextRef.current.document.sourceHash === hash
      ) {
        ++serial.current;
        applyContext(result);
      }
    } catch (error) {
      onError(error);
      await refresh().catch(onError);
    } finally {
      selecting.current = false;
    }
  }
  function requestRestore(version: Version) {
    const document = contextRef.current?.document;
    if (document) setConfirm({ version, document });
  }
  async function restore() {
    const pending = confirm;
    setConfirm(null);
    if (!pending) return;
    const current = pending.document;
    const version = pending.version;
    await operate(async () => {
      const job = await api.restore(current.documentId, {
        versionId: version.versionId,
        baseVersionId: current.headVersionId,
        expectedSourceHash: current.sourceHash,
        author: 'user',
        summary: t('restoreSummary', { version: version.seq }),
        waitMs: null,
      });
      await api.waitJob(job);
      setHistoryVersion(null);
      setCompareVersion(null);
      setNotice(t('restoredVersion', { version: version.seq }));
    });
  }
  const render = state.context?.render;
  const stale = !!doc && !!render && doc.sourceHash !== render.manifest.sourceHash;
  const jobs =
    state.status?.jobs.filter(
      (job) => job.documentId === documentId && ['queued', 'running'].includes(job.state),
    ) ?? [];
  const selected = state.context?.selection?.locators ?? [];
  const connected = state.connection === 'connected';
  const exportVersionId = historyVersion ?? doc?.headVersionId ?? null;
  const documentHint = doc
    ? [
        doc.dirty ? t('dirtyWorkspace') : t('cleanWorkspace'),
        state.context?.editActivity
          ? t('editor', { author: state.context.editActivity.author })
          : null,
        doc.editState === 'active' ? t('editOccupied') : null,
        ['interrupted', 'recovery-conflict'].includes(doc.editState) ? t('recoveryNeeded') : null,
        stale ? t('staleRender') : null,
        jobs.length
          ? t('processing', {
              jobs: jobs
                .map(
                  (job) =>
                    `${t(jobKindKey(job.kind))} · ${t(phaseKey(job.phase))} (${t(
                      jobStateKey(job.state),
                    )})`,
                )
                .join(language === 'zh-CN' ? '；' : '; '),
            })
          : null,
      ]
        .filter(Boolean)
        .join(language === 'zh-CN' ? '；' : ' · ')
    : '';
  return (
    <main className="app score-app">
      <header className="bar score-topbar">
        <h1 className="topbar-brand">
          <Music2 aria-hidden="true" size={20} /> {t('appName')}
        </h1>
        <span className="topbar-title" title={notFound ? undefined : doc?.title}>
          {notFound ? t('scoreNotFoundTitle') : (doc?.title ?? '')}
        </span>
        <div className="topbar-right">
          <span
            className={`connection connection-${state.connection}`}
            role="status"
            data-state={state.connection}
          >
            <Circle className="connection-light" aria-hidden="true" size={9} fill="currentColor" />
            <span className="connection-label">
              {connected
                ? t('connected')
                : state.connection === 'connecting'
                  ? t('connecting')
                  : t('reconnecting')}
            </span>
          </span>
          {doc && !notFound && (
            <ScoreToolbar
              api={api}
              versions={state.versions}
              viewingVersionId={historyVersion}
              onViewVersion={(versionId) => {
                setHistoryVersion(versionId);
                setCompareVersion(null);
                setNotice('');
              }}
              compareVersionId={compareVersion}
              onCompare={setCompareVersion}
              documentTitle={doc.title}
              currentRender={render ?? null}
              documentHint={documentHint}
              exportVersionId={exportVersionId}
              exportDisabled={!!compareVersion}
              onRestore={compareVersion ? undefined : requestRestore}
            />
          )}
        </div>
      </header>
      {notFound ? (
        <ScoreNotFound />
      ) : doc ? (
        <div className="score-workspace">
          {compareVersion ? (
            <CompareView
              api={api}
              documentId={doc.documentId}
              versions={state.versions}
              leftVersionId={historyVersion}
              rightVersionId={compareVersion}
              currentRender={render ?? null}
              layout={layout}
              onLayoutChange={setLayout}
              leftZoom={leftZoom}
              onLeftZoom={setLeftZoom}
              rightZoom={rightZoom}
              onRightZoom={setRightZoom}
              selected={selected}
              selectable={connected && !stale && !working && !historyVersion}
              onPoint={(page, x, y) => void choose(undefined, { page, x, y })}
            />
          ) : historyVersion ? (
            <VersionScore
              api={api}
              documentId={doc.documentId}
              versionId={historyVersion}
              layout={layout}
              onLayoutChange={setLayout}
              zoom={zoom}
              onZoomChange={setZoom}
            />
          ) : render ? (
            <ScoreView
              api={api}
              render={render}
              layout={layout}
              onLayoutChange={setLayout}
              zoom={zoom}
              onZoomChange={setZoom}
              selected={selected}
              selectable={connected && !stale && !working}
              onPoint={(page, x, y) => void choose(undefined, { page, x, y })}
            />
          ) : (
            <section className="empty">
              <h2>{t('noRenderTitle')}</h2>
              <p>{t('noRenderBody')}</p>
            </section>
          )}
        </div>
      ) : (
        <div className="score-workspace">
          <p className="score-loading">{connected ? t('loadingScore') : t('connectingLibrary')}</p>
        </div>
      )}
      <dialog ref={dialog} onCancel={() => setConfirm(null)} aria-labelledby="confirm-title">
        <h2 id="confirm-title">{t('restoreTitle', { version: confirm?.version.seq ?? '' })}</h2>
        <p>{t('restoreBody')}</p>
        <div className="row">
          <button type="button" autoFocus onClick={() => setConfirm(null)}>
            {t('cancel')}
          </button>
          <button type="button" onClick={() => void restore()}>
            {t('confirm')}
            {t('confirmRestore')}
          </button>
        </div>
      </dialog>
      <ToastRegion toasts={toast.toasts} onDismiss={toast.dismiss} />
    </main>
  );
}

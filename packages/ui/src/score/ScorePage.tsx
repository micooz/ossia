import { Circle, Music2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react';

import { ToastRegion, useToasts } from '../components/Toast.tsx';
import { Button } from '../components/ui/button.tsx';
import { describe } from '../errors.ts';
import type { DocumentContext, DocumentState, UiLanguage, Version } from '../generated/protocol.ts';
import {
  I18nProvider,
  jobKindKey,
  jobStateKey,
  matchSystemLanguage,
  phaseKey,
  translate,
  useI18n,
} from '../i18n.tsx';
import { ApiError, delay, makeApi } from './api.ts';
import { CompareView, VersionScore } from './components/CompareView.tsx';
import { ScoreNotFound } from './components/ScoreNotFound.tsx';
import { ScoreTitleMenu } from './components/ScoreTitleMenu.tsx';
import { ScoreToolbar } from './components/ScoreToolbar.tsx';
import type { ScoreLayout } from './components/ScoreToolbar.tsx';
import { ScoreView } from './components/ScoreView.tsx';
import { documentIdFromLocation, pushDocumentIdToLocation } from './endpoint.ts';
import { initialState, reducer } from './state.ts';
import { useSelection } from './useSelection.ts';

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
    if (context.document.documentId !== documentIdRef.current) return;
    const previous = contextRef.current;
    if (
      previous?.instanceId === context.instanceId &&
      previous.document.documentId === context.document.documentId &&
      previous.seq > context.seq
    )
      return;
    contextRef.current = context;
    dispatch({ type: 'context', context });
  }, []);
  const resetDocumentView = useCallback(() => {
    setHistoryVersion(null);
    setCompareVersion(null);
    setConfirm(null);
    setNotice('');
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
      previous &&
      context &&
      previous.instanceId === context.instanceId &&
      previous.document.documentId === context.document.documentId &&
      previous.seq > context.seq
    ) {
      context = previous;
    }
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
  const selection = useSelection(api, state.context, contextRef, applyContext, refresh, onError);
  const refreshSelection = useCallback(
    async (documentId: string) => {
      const context = await api.context(documentId);
      if (!lifetime.signal.aborted) applyContext(context);
    },
    [api, applyContext, lifetime],
  );
  const selectDocument = useCallback(
    (next: string) => {
      if (next === documentIdRef.current) return;
      resetDocumentView();
      documentIdRef.current = next;
      pushDocumentIdToLocation(next);
      void refresh().catch(onError);
    },
    [onError, refresh, resetDocumentView],
  );
  useEffect(() => {
    const onPopState = () => {
      const next = documentIdFromLocation();
      if (next === documentIdRef.current) return;
      resetDocumentView();
      documentIdRef.current = next;
      void refresh().catch(onError);
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, [onError, refresh, resetDocumentView]);
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
            if (event.documentId && event.documentId !== documentIdRef.current) return;
            if (event.type === 'selection-changed' && event.documentId) {
              void refreshSelection(event.documentId).catch(onError);
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
  }, [api, lifetime, refresh, refreshSelection, onError, onLanguage]);
  const doc = state.context?.document;
  const documentId = doc?.documentId;
  useEffect(() => {
    resetDocumentView();
  }, [documentId, resetDocumentView]);
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
  async function choosePoint(page: number, x: number, y: number) {
    const context = contextRef.current;
    if (!context || state.connection !== 'connected') return;
    const document = context.document;
    if (context.render?.manifest.sourceHash !== document.sourceHash) return;
    try {
      const result = await api.locate(document.documentId, {
        sourceHash: document.sourceHash,
        page,
        x,
        y,
      });
      const current = contextRef.current;
      if (
        current?.document.documentId !== document.documentId ||
        current.document.sourceHash !== document.sourceHash ||
        current.render?.renderKey !== context.render.renderKey
      )
        return;
      if (result.capability === 'measure' && result.region) selection.choose(result.region.locator);
      else if (result.reason && result.reason !== 'no-hit') {
        setNotice(t('cannotSelect', { reason: result.reason }));
      }
    } catch (error) {
      onError(error);
      await refresh().catch(onError);
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
    if (!current.headVersionId) {
      onError(new Error('Restore requires a current document version'));
      return;
    }
    const baseVersionId = current.headVersionId;
    await operate(async () => {
      const job = await api.restore(current.documentId, {
        versionId: version.versionId,
        baseVersionId,
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
  const selected = selection.selected;
  const connected = state.connection === 'connected';
  const exportVersionId = historyVersion ?? doc?.headVersionId ?? null;
  const viewingVersion =
    state.versions.find((version) => version.versionId === historyVersion) ?? null;
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
          <Music2 aria-hidden="true" size={20} />
          <span className="topbar-brand-label">{t('appName')}</span>
        </h1>
        {doc && !notFound ? (
          <ScoreTitleMenu
            api={api}
            documentId={doc.documentId}
            title={doc.title}
            onSelect={selectDocument}
          />
        ) : (
          <span
            className="topbar-title topbar-title-static"
            title={notFound ? undefined : doc?.title}
          >
            {notFound ? t('scoreNotFoundTitle') : (doc?.title ?? '')}
          </span>
        )}
        <div className="topbar-right">
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
            />
          )}
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
        </div>
      </header>
      {doc && !notFound && (compareVersion || viewingVersion) && (
        <div className="score-context-region">
          <div className="score-context-bar">
            {compareVersion ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="score-context-action"
                onClick={() => setCompareVersion(null)}
              >
                {t('exitCompare')}
              </Button>
            ) : viewingVersion ? (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="score-context-action"
                  onClick={() => {
                    setHistoryVersion(null);
                    setNotice('');
                  }}
                >
                  {t('backToCurrentVersion')}
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="score-context-action"
                  title={t('restoreVersion', { version: viewingVersion.seq })}
                  onClick={() => requestRestore(viewingVersion)}
                >
                  {t('restoreThisVersion')}
                </Button>
              </>
            ) : null}
          </div>
        </div>
      )}
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
              onSelect={selection.choose}
              onPoint={(page, x, y) => void choosePoint(page, x, y)}
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
              onSelect={selection.choose}
              onPoint={(page, x, y) => void choosePoint(page, x, y)}
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
      <dialog
        ref={dialog}
        className="confirm-dialog"
        onCancel={() => setConfirm(null)}
        aria-labelledby="confirm-title"
        aria-describedby="confirm-body"
      >
        <div className="confirm-dialog-header">
          <h2 id="confirm-title">{t('restoreTitle', { version: confirm?.version.seq ?? '' })}</h2>
        </div>
        <div className="confirm-dialog-body">
          <p id="confirm-body">{t('restoreBody')}</p>
        </div>
        <div className="confirm-dialog-footer">
          <Button type="button" variant="outline" autoFocus onClick={() => setConfirm(null)}>
            {t('cancel')}
          </Button>
          <Button type="button" className="confirm-dialog-submit" onClick={() => void restore()}>
            {t('confirm')}
            {t('confirmRestore')}
          </Button>
        </div>
      </dialog>
      <ToastRegion toasts={toast.toasts} onDismiss={toast.dismiss} />
    </main>
  );
}

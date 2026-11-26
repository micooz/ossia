import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { ToastRegion, useToasts } from '../components/Toast.tsx';
import { describe } from '../errors.ts';
import type {
  DocumentState,
  DrawingColor,
  GeometryElement,
  Job,
  PlaybackPart,
  UiLanguage,
  Version,
} from '../generated/protocol.ts';
import { I18nProvider, useI18n } from '../i18n.tsx';
import { ScoreFeatureToolbar } from './components/ScoreFeatureToolbar.tsx';
import { RestoreDialog, ScorePageHeader, ScoreWorkspace } from './components/ScorePageSections.tsx';
import { useDrawing } from './hooks/useDrawing.ts';
import type { DrawingMode } from './hooks/useDrawingInput.ts';
import { useModalGestureLock } from './hooks/useModalGestureLock.ts';
import { useScoreFullscreen } from './hooks/useScoreFullscreen.ts';
import { useScorePlayback } from './hooks/useScorePlayback.ts';
import { useScoreSettings } from './hooks/useScoreSettings.ts';
import { useScoreShortcuts } from './hooks/useScoreShortcuts.ts';
import { useScoreSync } from './hooks/useScoreSync.ts';
import { useScoreViewMemory } from './hooks/useScoreViewMemory.ts';
import { useSelection } from './hooks/useSelection.ts';
import { useVersionRender } from './hooks/useVersionRender.ts';
import { isEditableKeyboardTarget } from './utils/annotationInput.ts';
import { makeApi } from './utils/api.ts';
import { DRAWING_PALETTE, ERASER_SIZES, PEN_SIZES } from './utils/drawing.ts';
import {
  documentIdFromLocation,
  pushDocumentIdToLocation,
  scoreDebugFlagsFromLocation,
} from './utils/endpoint.ts';
import { loadScorePlayback } from './utils/renderCache.ts';
import { focusScorePage } from './utils/scoreFocus.ts';
import { formatDocumentHint } from './utils/scorePageHint.ts';
import { readRememberedVersion, rememberVersion } from './utils/scoreVersionMemory.ts';

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

function isPendingRenderJob(job: Job) {
  return ['check', 'import'].includes(job.kind) && ['queued', 'running'].includes(job.state);
}

function savedDrawingSize(key: string, sizes: number[], fallback: number) {
  try {
    const value = Number(localStorage.getItem(key));
    return sizes.includes(value) ? value : fallback;
  } catch {
    return fallback;
  }
}

function ScoreApp({
  language,
  onLanguage,
}: {
  language: UiLanguage;
  onLanguage: (language: UiLanguage) => void;
}) {
  const { t } = useI18n();
  const toast = useToasts();
  useModalGestureLock();
  const languageRef = useRef(language);
  useEffect(() => {
    languageRef.current = language;
  }, [language]);
  useEffect(() => {
    const returnFocusToScore = (event: Event) => {
      if (
        !(event.target instanceof HTMLDialogElement) ||
        !event.target.closest('.score-app') ||
        document.querySelector('dialog[open]')
      )
        return;
      focusScorePage();
    };
    document.addEventListener('close', returnFocusToScore, true);
    return () => document.removeEventListener('close', returnFocusToScore, true);
  }, []);
  const lifetime = useMemo(() => new AbortController(), []);
  const api = useMemo(() => makeApi(() => languageRef.current, lifetime.signal), [lifetime]);
  const fullscreen = useScoreFullscreen();
  const fullscreenButton = useRef<HTMLButtonElement>(null);
  const documentIdRef = useRef(documentIdFromLocation());
  const debugFlags = useMemo(scoreDebugFlagsFromLocation, []);
  const showHitBoxes = debugFlags.has('hitboxes');
  const [working, setWorking] = useState(false);
  const [notice, setNotice] = useState('');
  const [historyVersion, setHistoryVersion] = useState<string | null>(() =>
    readRememberedVersion(documentIdRef.current),
  );
  const [compareVersion, setCompareVersion] = useState<string | null>(null);
  const [annotationMode, setAnnotationMode] = useState(false);
  const [drawingMode, setDrawingMode] = useState<DrawingMode>(null);
  const [drawingModifier, setDrawingModifier] = useState(false);
  const [penSize, setPenSize] = useState(() => savedDrawingSize('ossia:pen-size', PEN_SIZES, 4));
  const [eraserSize, setEraserSize] = useState(() =>
    savedDrawingSize('ossia:eraser-size', ERASER_SIZES, 20),
  );
  const [penColor, setPenColor] = useState<DrawingColor>(() => {
    try {
      const value = localStorage.getItem('ossia:pen-color');
      return DRAWING_PALETTE.find((color) => color === value) ?? 'black';
    } catch {
      return 'black';
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem('ossia:pen-size', String(penSize));
      localStorage.setItem('ossia:eraser-size', String(eraserSize));
      localStorage.setItem('ossia:pen-color', penColor);
    } catch {
      // Keep the selected settings for this session when storage is unavailable.
    }
  }, [penSize, eraserSize, penColor]);
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
  const { state, contextRef, applyContext, refresh, notFound } = useScoreSync(
    api,
    lifetime,
    documentIdRef,
    onLanguage,
    onError,
  );
  const drawing = useDrawing(api, state.context?.document.documentId);
  const drawingDirty = useRef(drawing.dirty);
  drawingDirty.current = drawing.dirty;
  const resetDocumentView = useCallback(() => {
    setHistoryVersion(readRememberedVersion(documentIdRef.current));
    setCompareVersion(null);
    setConfirm(null);
    setNotice('');
    setAnnotationMode(false);
  }, []);
  const selectVersion = useCallback((versionId: string | null) => {
    setHistoryVersion(versionId);
    rememberVersion(documentIdRef.current, versionId);
  }, []);
  const selection = useSelection(api, state.context, contextRef, applyContext, refresh, onError);
  const selectDocument = useCallback(
    (next: string) => {
      if (next === documentIdRef.current) return;
      if (drawingDirty.current) {
        setNotice(t('drawingUnsaved'));
        return;
      }
      documentIdRef.current = next;
      resetDocumentView();
      pushDocumentIdToLocation(next);
      void refresh().catch(onError);
    },
    [onError, refresh, resetDocumentView, t],
  );
  useEffect(() => {
    const onPopState = () => {
      const next = documentIdFromLocation();
      if (next === documentIdRef.current) return;
      if (drawingDirty.current) {
        if (documentIdRef.current) pushDocumentIdToLocation(documentIdRef.current);
        setNotice(t('drawingUnsaved'));
        return;
      }
      documentIdRef.current = next;
      resetDocumentView();
      void refresh().catch(onError);
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, [onError, refresh, resetDocumentView, t]);
  const doc = state.context?.document;
  const documentId = doc?.documentId;
  const scoreSettings = useScoreSettings(api, documentId, onError);
  useEffect(() => {
    if (!documentId) return;
    setHistoryVersion(readRememberedVersion(documentId));
    setCompareVersion(null);
    setConfirm(null);
    setNotice('');
    setAnnotationMode(false);
  }, [documentId]);
  useEffect(() => {
    if (
      !historyVersion ||
      !state.versionsReady ||
      !documentId ||
      state.context?.document.documentId !== documentId ||
      state.versions.some((version) => version.versionId === historyVersion)
    )
      return;
    selectVersion(null);
  }, [
    documentId,
    historyVersion,
    selectVersion,
    state.context?.document.documentId,
    state.versions,
    state.versionsReady,
  ]);
  useEffect(() => {
    document.title = notFound ? t('scoreNotFoundDocumentTitle') : (doc?.title ?? t('appName'));
  }, [doc?.title, language, notFound, t]);
  useEffect(() => {
    if (fullscreen.active && (!doc || notFound)) fullscreen.exit();
  }, [doc, fullscreen.active, fullscreen.exit, notFound]);
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
      if (result.region) selection.choose(result.region.target);
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
        discardCurrentEdit: true,
        author: 'user',
        summary: t('restoreSummary', { version: version.seq }),
        waitMs: null,
      });
      await api.waitJob(job);
      selectVersion(null);
      setCompareVersion(null);
      setNotice(t('restoredVersion', { version: version.seq }));
    });
  }
  const render = state.context?.render;
  const stale = !!doc && !!render && doc.sourceHash !== render.manifest.sourceHash;
  const latestVersion = state.versions.reduce<Version | null>(
    (latest, version) => (!latest || version.seq > latest.seq ? version : latest),
    null,
  );
  const draftAvailable =
    !!doc && (doc.dirty || (state.versionsReady && doc.sourceHash !== latestVersion?.sourceHash));
  const viewKey = `${documentId ?? 'none'}:${historyVersion ?? 'draft'}`;
  const viewMemory = useScoreViewMemory(documentId ?? null, historyVersion);
  const { layout, zoom, initialPan, setLayout, setZoom, savePan } = viewMemory;
  const versionRender = useVersionRender(api, documentId ?? null, historyVersion);
  const historyRender = historyVersion ? versionRender.render : null;
  const activeRender = historyVersion ? historyRender : (render ?? null);
  const jobs =
    state.status?.jobs.filter(
      (job) => job.documentId === documentId && ['queued', 'running'].includes(job.state),
    ) ?? [];
  const renderPending =
    !render &&
    (jobs.some(isPendingRenderJob) || (!!doc?.lastCheck && isPendingRenderJob(doc.lastCheck)));
  const connected = state.connection === 'connected';
  const agentEditing =
    connected && !historyVersion && !compareVersion && doc?.editState === 'active';
  const drawingAvailable =
    connected && !!activeRender && !agentEditing && !compareVersion && !working && drawing.ready;
  useEffect(() => {
    const update = (event: KeyboardEvent) => setDrawingModifier(event.metaKey || event.ctrlKey);
    const clear = () => setDrawingModifier(false);
    window.addEventListener('keydown', update);
    window.addEventListener('keyup', update);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', update);
      window.removeEventListener('keyup', update);
      window.removeEventListener('blur', clear);
    };
  }, []);
  useEffect(() => {
    if (!drawingAvailable) setDrawingMode(null);
  }, [drawingAvailable]);
  useEffect(() => {
    if (annotationMode) setDrawingMode(null);
  }, [annotationMode]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        !drawingAvailable ||
        event.defaultPrevented ||
        event.isComposing ||
        event.repeat ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.shiftKey ||
        document.querySelector('dialog[open]') ||
        isEditableKeyboardTarget(event.target)
      )
        return;
      const key = event.key.toLowerCase();
      if (key !== 'd' && key !== 'r') return;
      event.preventDefault();
      setAnnotationMode(false);
      const mode = key === 'd' ? 'pen' : 'eraser';
      setDrawingMode((current) => (current === mode ? null : mode));
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [drawingAvailable]);
  const annotationAvailable =
    connected &&
    !agentEditing &&
    !stale &&
    !working &&
    !!render &&
    render.manifest.sourceHash === doc?.sourceHash &&
    !historyVersion &&
    !compareVersion;
  const playbackEnabled =
    connected &&
    !agentEditing &&
    !working &&
    !compareVersion &&
    (historyVersion ? !!historyRender : annotationAvailable);
  const annotationActive = annotationMode && annotationAvailable;
  const activeDrawingMode = drawingAvailable ? drawingMode : null;
  const selectedMeasure = historyVersion
    ? undefined
    : selection.selected.findLast((target) => target.kind === 'measure')?.locator.measureStart;
  const playback = useScorePlayback(
    api,
    activeRender,
    playbackEnabled && !!scoreSettings.settings,
    onError,
    scoreSettings.settings,
    selectedMeasure,
  );
  const [loadedParts, setLoadedParts] = useState<{ key: string; parts: PlaybackPart[] } | null>(
    null,
  );
  const settingsRenderKey = activeRender?.renderKey;
  const settingsParts = settingsRenderKey
    ? loadedParts?.key === settingsRenderKey
      ? loadedParts.parts
      : null
    : [];
  useEffect(() => {
    let live = true;
    setLoadedParts(null);
    if (settingsRenderKey) {
      void loadScorePlayback(api, settingsRenderKey)
        .then((timeline) => {
          if (live) setLoadedParts({ key: settingsRenderKey, parts: timeline.parts });
        })
        .catch(() => {
          if (live) setLoadedParts({ key: settingsRenderKey, parts: [] });
        });
    }
    return () => {
      live = false;
    };
  }, [api, settingsRenderKey]);
  const activateNote = useCallback(
    (element: GeometryElement, simultaneous: boolean) => {
      const target = element.region.target;
      if (!target || target.kind !== 'element') return;
      const group = playback.timeline?.groups.find((candidate) =>
        candidate.elementIds.includes(target.elementId),
      );
      if (group)
        void playback.audition(group, {
          simultaneous,
          highlightElementId: target.elementId,
        });
    },
    [playback.timeline, playback.audition],
  );
  useScoreShortcuts({
    annotationAvailable,
    annotationMode: annotationActive,
    drawingMode: activeDrawingMode,
    playbackEnabled,
    playbackPhase: playback.phase,
    setAnnotationMode,
    setDrawingMode,
    togglePlayback: playback.toggle,
    clearPlaybackHighlight: playback.clearAuditionHighlight,
    seekMeasure: playback.seekMeasure,
    auditionAdjacent: playback.auditionAdjacent,
  });
  const exportVersionId = historyVersion ?? doc?.headVersionId ?? null;
  const viewingVersion =
    state.versions.find((version) => version.versionId === historyVersion) ?? null;
  const documentHint = doc
    ? formatDocumentHint({
        doc,
        editActivity: state.context?.editActivity,
        stale,
        jobs,
        language,
        t,
      })
    : '';
  const toggleFullscreen = useCallback(() => {
    fullscreen.toggle();
    if (fullscreen.active) {
      requestAnimationFrame(() => fullscreenButton.current?.focus());
    }
  }, [fullscreen.active, fullscreen.toggle]);
  const showFeatureToolbar =
    agentEditing ||
    annotationAvailable ||
    (viewingVersion && connected && !working) ||
    fullscreen.active;
  const featureToolbar = showFeatureToolbar ? (
    <ScoreFeatureToolbar
      editing={agentEditing}
      annotationEnabled={annotationActive}
      annotationAvailable={annotationAvailable}
      onToggleAnnotation={() => setAnnotationMode(!annotationActive)}
      playbackPhase={playback.phase}
      playbackAvailable={playback.available}
      playbackUnavailableReason={playback.unavailableReason}
      onTogglePlayback={() => void playback.toggle()}
      onRestoreVersion={viewingVersion ? () => requestRestore(viewingVersion) : undefined}
      fullscreen={fullscreen.active}
      onToggleFullscreen={toggleFullscreen}
      fullscreenButtonRef={fullscreenButton}
      floating={fullscreen.active}
      drawingMode={activeDrawingMode}
      sizingMode={activeDrawingMode === 'pen' && drawingModifier ? 'eraser' : activeDrawingMode}
      onDrawingMode={(mode) => {
        setAnnotationMode(false);
        setDrawingMode(mode);
      }}
      drawingAvailable={drawingAvailable}
      penSize={penSize}
      eraserSize={eraserSize}
      penColor={penColor}
      onPenColor={setPenColor}
      onPenSize={setPenSize}
      onEraserSize={setEraserSize}
    />
  ) : null;
  return (
    <main className={`app score-app${fullscreen.active ? ' is-fullscreen' : ''}`} tabIndex={-1}>
      <ScorePageHeader
        api={api}
        doc={doc}
        notFound={notFound}
        versions={state.versions}
        draftAvailable={draftAvailable}
        historyVersion={historyVersion}
        compareVersion={compareVersion}
        render={render ?? null}
        documentHint={documentHint}
        exportVersionId={exportVersionId}
        onSelectDocument={selectDocument}
        onViewVersion={selectVersion}
        onCompare={setCompareVersion}
        onClearNotice={() => setNotice('')}
        featureToolbar={fullscreen.active ? null : featureToolbar}
        drawing={drawing}
        webBaseUrl={state.status?.runtime.webBaseUrl ?? null}
        scoreSettings={scoreSettings}
        playbackParts={settingsParts}
        onError={onError}
      />
      <ScoreWorkspace
        api={api}
        doc={doc}
        notFound={notFound}
        connected={connected}
        stale={stale}
        working={working}
        versions={state.versions}
        historyVersion={historyVersion}
        compareVersion={compareVersion}
        onCompare={setCompareVersion}
        render={render ?? null}
        renderPending={renderPending}
        agentEditing={agentEditing}
        layout={layout}
        onLayout={setLayout}
        zoom={zoom}
        onZoom={setZoom}
        leftZoom={leftZoom}
        onLeftZoom={setLeftZoom}
        rightZoom={rightZoom}
        onRightZoom={setRightZoom}
        selection={selection}
        playback={playback}
        versionRender={versionRender}
        annotationMode={annotationActive}
        onAnnotationMode={setAnnotationMode}
        viewKey={viewKey}
        initialPan={initialPan}
        onPan={savePan}
        showHitBoxes={showHitBoxes}
        debugAgentRing={debugFlags.has('ring')}
        onActivateNote={activateNote}
        onPoint={(page, x, y) => void choosePoint(page, x, y)}
        featureToolbar={fullscreen.active ? featureToolbar : null}
        drawing={drawing}
        drawingMode={activeDrawingMode}
        penSize={penSize}
        eraserSize={eraserSize}
        penColor={penColor}
      />
      <RestoreDialog
        dialogRef={dialog}
        onCancel={() => setConfirm(null)}
        onConfirm={() => void restore()}
      />
      <ToastRegion toasts={toast.toasts} onDismiss={toast.dismiss} />
    </main>
  );
}

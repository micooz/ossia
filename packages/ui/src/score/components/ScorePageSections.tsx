import * as Popover from '@radix-ui/react-popover';
import { FileMusic, MoreHorizontal } from 'lucide-react';
import { useLayoutEffect, useRef, useState } from 'react';
import type { ReactNode, RefObject } from 'react';

import { Button } from '../../components/ui/button.tsx';
import type {
  DocumentState,
  DrawingColor,
  GeometryElement,
  RenderReference,
  PlaybackPart,
  Version,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { DrawingController } from '../hooks/useDrawing.ts';
import type { DrawingMode } from '../hooks/useDrawingInput.ts';
import type { useScorePlayback } from '../hooks/useScorePlayback.ts';
import type { useScoreSettings } from '../hooks/useScoreSettings.ts';
import type { ScoreViewPan } from '../hooks/useScoreViewMemory.ts';
import type { useSelection } from '../hooks/useSelection.ts';
import type { useVersionRender } from '../hooks/useVersionRender.ts';
import type { Api } from '../utils/api.ts';
import type { ScoreLayout } from '../utils/scoreLayout.ts';
import { CompareView } from './CompareView.tsx';
import { ScoreAnnotationBadge } from './ScoreAnnotationBadge.tsx';
import { ScoreHelpDialog } from './ScoreHelpDialog.tsx';
import { ScoreLoading } from './ScoreLoading.tsx';
import { ScoreNotFound } from './ScoreNotFound.tsx';
import { ScoreQrCode } from './ScoreQrCode.tsx';
import { ScoreSettingsDialog } from './ScoreSettingsDialog.tsx';
import { ScoreTitleMenu } from './ScoreTitleMenu.tsx';
import { ScoreToolbar } from './ScoreToolbar.tsx';
import { ScoreVersionMenu } from './ScoreVersionMenu.tsx';
import { ScoreView } from './ScoreView.tsx';

type PlaybackController = ReturnType<typeof useScorePlayback>;
type SelectionController = ReturnType<typeof useSelection>;
type VersionRenderController = ReturnType<typeof useVersionRender>;

interface ScorePageHeaderProps {
  api: Api;
  doc: DocumentState | undefined;
  notFound: boolean;
  versions: Version[];
  draftAvailable: boolean;
  historyVersion: string | null;
  compareVersion: string | null;
  render: RenderReference | null;
  documentHint: string;
  exportVersionId: string | null;
  onSelectDocument: (documentId: string) => void;
  onViewVersion: (versionId: string | null) => void;
  onCompare: (versionId: string | null) => void;
  onClearNotice: () => void;
  featureToolbar: ReactNode;
  webBaseUrl: string | null;
  drawing: DrawingController;
  scoreSettings: ReturnType<typeof useScoreSettings>;
  playbackParts: PlaybackPart[] | null;
  onError: (error: unknown) => void;
}

export function ScorePageHeader({
  api,
  doc,
  notFound,
  versions,
  draftAvailable,
  historyVersion,
  compareVersion,
  render,
  documentHint,
  exportVersionId,
  onSelectDocument,
  onViewVersion,
  onCompare,
  onClearNotice,
  featureToolbar,
  webBaseUrl,
  drawing,
  scoreSettings,
  playbackParts,
  onError,
}: ScorePageHeaderProps) {
  const { t } = useI18n();
  const rightRef = useRef<HTMLDivElement>(null);
  const [rightWidth, setRightWidth] = useState(0);
  useLayoutEffect(() => {
    const element = rightRef.current;
    if (!element) return;
    const update = () => {
      if (!document.querySelector('dialog[open]')) setRightWidth(element.clientWidth);
    };
    const observer = new ResizeObserver(update);
    observer.observe(element);
    document.addEventListener('close', update, true);
    update();
    return () => {
      observer.disconnect();
      document.removeEventListener('close', update, true);
    };
  }, []);
  const compactVersion = rightWidth < 72;
  const versionLabelWidth = 104;
  const actionWidth = 36;
  const visibleActions =
    rightWidth >= versionLabelWidth + actionWidth * 4
      ? 4
      : Math.max(
          0,
          Math.min(3, Math.floor((rightWidth - versionLabelWidth - actionWidth) / actionWidth)),
        );
  const versionControl = doc && !notFound && (
    <ScoreVersionMenu
      versions={versions}
      draftAvailable={draftAvailable}
      editing={doc.editState === 'active'}
      viewingVersionId={historyVersion}
      onViewVersion={(versionId) => {
        onViewVersion(versionId);
        onCompare(null);
        onClearNotice();
      }}
      documentHint={documentHint}
    />
  );
  const actions =
    doc && !notFound
      ? [
          <ScoreToolbar
            key="export"
            api={api}
            versions={versions}
            viewingVersionId={historyVersion}
            compareVersionId={compareVersion}
            onCompare={onCompare}
            documentTitle={doc.title}
            currentRender={render}
            documentHint={documentHint}
            exportVersionId={exportVersionId}
            exportDisabled={!!compareVersion}
            drawing={drawing}
            documentId={doc.documentId}
          />,
          <ScoreQrCode key="qr" webBaseUrl={webBaseUrl} documentId={doc.documentId} />,
          <ScoreSettingsDialog
            key="settings"
            api={api}
            settings={scoreSettings}
            parts={playbackParts}
            onError={onError}
          />,
          <ScoreHelpDialog key="help" />,
        ]
      : [<ScoreHelpDialog key="help" />];
  const hasOverflow = compactVersion || visibleActions < actions.length;
  const versionWidth = rightWidth - visibleActions * actionWidth - (hasOverflow ? actionWidth : 0);
  return (
    <header className="bar score-topbar">
      <div className="score-document-controls">
        {doc && !notFound ? (
          <ScoreTitleMenu
            api={api}
            documentId={doc.documentId}
            title={doc.title}
            onSelect={onSelectDocument}
          />
        ) : (
          <span
            className="topbar-title topbar-title-static"
            title={notFound ? undefined : doc?.title}
          >
            {notFound ? t('scoreNotFoundTitle') : (doc?.title ?? '')}
          </span>
        )}
      </div>
      <div className="score-feature-slot">{featureToolbar}</div>
      <div
        className="topbar-right"
        ref={rightRef}
        data-compact-version-label={versionWidth < versionLabelWidth ? 'true' : undefined}
      >
        {!compactVersion && versionControl}
        {actions.slice(0, visibleActions)}
        {hasOverflow && (
          <Popover.Root>
            <Popover.Trigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="topbar-action"
                aria-label={t('moreActions')}
              >
                <MoreHorizontal aria-hidden="true" size={19} />
              </Button>
            </Popover.Trigger>
            <Popover.Portal>
              <Popover.Content
                className="score-header-actions-popover"
                align="end"
                sideOffset={8}
                onInteractOutside={(event) => {
                  if (
                    event.target instanceof Element &&
                    event.target.closest('dialog, .score-qr-panel')
                  )
                    event.preventDefault();
                }}
              >
                {compactVersion && versionControl}
                {actions.slice(visibleActions)}
              </Popover.Content>
            </Popover.Portal>
          </Popover.Root>
        )}
      </div>
    </header>
  );
}

interface ScoreWorkspaceProps {
  api: Api;
  doc: DocumentState | undefined;
  notFound: boolean;
  connected: boolean;
  stale: boolean;
  working: boolean;
  versions: Version[];
  historyVersion: string | null;
  compareVersion: string | null;
  onCompare: (versionId: string | null) => void;
  render: RenderReference | null;
  renderPending: boolean;
  agentEditing: boolean;
  layout: ScoreLayout;
  onLayout: (layout: ScoreLayout) => void;
  zoom: number;
  onZoom: (zoom: number) => void;
  leftZoom: number;
  onLeftZoom: (zoom: number) => void;
  rightZoom: number;
  onRightZoom: (zoom: number) => void;
  selection: SelectionController;
  playback: PlaybackController;
  versionRender: VersionRenderController;
  annotationMode: boolean;
  onAnnotationMode: (mode: boolean) => void;
  viewKey: string;
  initialPan: ScoreViewPan;
  onPan: (x: number, y: number) => void;
  showHitBoxes: boolean;
  debugAgentRing: boolean;
  onActivateNote: (element: GeometryElement, simultaneous: boolean) => void;
  onPoint: (page: number, x: number, y: number) => void;
  featureToolbar: ReactNode;
  drawing: DrawingController;
  drawingMode: DrawingMode;
  penSize: number;
  eraserSize: number;
  penColor: DrawingColor;
}

export function ScoreWorkspace({
  api,
  doc,
  notFound,
  connected,
  stale,
  working,
  versions,
  historyVersion,
  compareVersion,
  onCompare,
  render,
  renderPending,
  agentEditing,
  layout,
  onLayout,
  zoom,
  onZoom,
  leftZoom,
  onLeftZoom,
  rightZoom,
  onRightZoom,
  selection,
  playback,
  versionRender,
  annotationMode,
  onAnnotationMode,
  viewKey,
  initialPan,
  onPan,
  showHitBoxes,
  debugAgentRing,
  onActivateNote,
  onPoint,
  featureToolbar,
  drawing,
  drawingMode,
  penSize,
  eraserSize,
  penColor,
}: ScoreWorkspaceProps) {
  const { t } = useI18n();
  const previous = useRef<{ documentId: string; render: RenderReference } | null>(null);
  useLayoutEffect(() => {
    if (!doc || notFound) previous.current = null;
    else if (render) previous.current = { documentId: doc.documentId, render };
  }, [doc?.documentId, notFound, render]);
  const draftRender =
    playback.render ??
    (previous.current?.documentId === doc?.documentId ? previous.current?.render : null);
  if (notFound) return <ScoreNotFound />;
  if (!doc) {
    return (
      <div className="score-workspace">
        <ScoreLoading label={connected ? t('loadingScore') : t('connectingLibrary')} />
      </div>
    );
  }
  return (
    <>
      {compareVersion && (
        <div className="score-context-region">
          <div className="score-context-bar">
            <Button
              type="button"
              variant="outline"
              size="sm"
              className="score-context-action"
              onClick={() => onCompare(null)}
            >
              {t('exitCompare')}
            </Button>
          </div>
        </div>
      )}
      <div className="score-workspace">
        {featureToolbar}
        {drawing.error && (
          <button type="button" className="score-drawing-error" onClick={drawing.retry}>
            {t('drawingSaveFailed')}
          </button>
        )}
        {render && !historyVersion && !compareVersion && (
          <ScoreAnnotationBadge
            annotationCount={selection.annotations.length}
            onClear={selection.clear}
          />
        )}
        {compareVersion ? (
          <CompareView
            api={api}
            drawing={drawing}
            documentId={doc.documentId}
            versions={versions}
            leftVersionId={historyVersion}
            rightVersionId={compareVersion}
            currentRender={render}
            layout={layout}
            onLayoutChange={onLayout}
            leftZoom={leftZoom}
            onLeftZoom={onLeftZoom}
            rightZoom={rightZoom}
            onRightZoom={onRightZoom}
            selected={selection.selected}
            selectable={connected && !stale && !working && !historyVersion}
            onSelect={selection.choose}
            onPoint={onPoint}
          />
        ) : historyVersion ? (
          versionRender.error ? (
            <p className="error" role="alert">
              {versionRender.error}
            </p>
          ) : playback.render ? (
            <ScoreView
              key={viewKey}
              api={api}
              render={playback.render}
              layout={layout}
              onLayoutChange={onLayout}
              zoom={zoom}
              onZoomChange={onZoom}
              activeElementIds={playback.activeElementIds}
              playheadElementIds={playback.playheadElementIds}
              playhead={playback.playhead}
              playbackSystems={playback.timeline?.systems}
              onAuditionNote={onActivateNote}
              onCueMeasure={playback.cueMeasure}
              viewKey={viewKey}
              initialPan={initialPan}
              onPanChange={onPan}
              drawing={drawing}
              drawingMode={drawingMode}
              penSize={penSize}
              eraserSize={eraserSize}
              penColor={penColor}
            />
          ) : (
            <ScoreLoading label={t('loadingHistoryScore')} />
          )
        ) : draftRender ? (
          <ScoreView
            key={viewKey}
            api={api}
            render={draftRender}
            layout={layout}
            onLayoutChange={onLayout}
            zoom={zoom}
            onZoomChange={onZoom}
            selected={selection.selected}
            annotations={selection.annotations}
            agentEditing={agentEditing || debugAgentRing}
            annotate={annotationMode && !!render && !stale}
            onAnnotationClose={() => onAnnotationMode(false)}
            onAnnotationsChange={selection.setAnnotations}
            showHitBoxes={showHitBoxes}
            activeElementIds={playback.activeElementIds}
            playheadElementIds={playback.playheadElementIds}
            playhead={playback.playhead}
            playbackSystems={playback.timeline?.systems}
            onAuditionNote={render && !stale ? onActivateNote : undefined}
            onCueMeasure={playback.cueMeasure}
            viewKey={viewKey}
            initialPan={initialPan}
            onPanChange={onPan}
            drawing={drawing}
            drawingMode={drawingMode}
            penSize={penSize}
            eraserSize={eraserSize}
            penColor={penColor}
          />
        ) : renderPending ? (
          <ScoreLoading label={t('loadingLatestDraft')} />
        ) : (
          <section className="score-empty">
            <div className="score-empty-icon" aria-hidden="true">
              <FileMusic size={28} strokeWidth={1.5} />
            </div>
            <h2>{t('noRenderTitle')}</h2>
            <p>{t('noRenderBody')}</p>
          </section>
        )}
      </div>
    </>
  );
}

interface RestoreDialogProps {
  dialogRef: RefObject<HTMLDialogElement | null>;
  onCancel: () => void;
  onConfirm: () => void;
}

export function RestoreDialog({ dialogRef, onCancel, onConfirm }: RestoreDialogProps) {
  const { t } = useI18n();
  return (
    <dialog
      ref={dialogRef}
      className="confirm-dialog"
      onCancel={onCancel}
      aria-labelledby="confirm-title"
      aria-describedby="confirm-body"
    >
      <div className="confirm-dialog-header">
        <h2 id="confirm-title">{t('restoreTitle')}</h2>
      </div>
      <div className="confirm-dialog-body">
        <p id="confirm-body">{t('restoreBody')}</p>
      </div>
      <div className="confirm-dialog-footer">
        <Button type="button" variant="outline" autoFocus onClick={onCancel}>
          {t('cancel')}
        </Button>
        <Button type="button" className="confirm-dialog-submit" onClick={onConfirm}>
          {t('confirm')}
          {t('confirmRestore')}
        </Button>
      </div>
    </dialog>
  );
}

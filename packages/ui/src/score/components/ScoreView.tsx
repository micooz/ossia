import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';
import { createPortal } from 'react-dom';

import type {
  GeometryElement,
  DrawingColor,
  GeometryPage,
  PlaybackSystemSpan,
  RenderReference,
  SelectionAnnotation,
  SelectionTarget,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { useAnnotationMarquee } from '../hooks/useAnnotationMarquee.ts';
import type { DrawingController } from '../hooks/useDrawing.ts';
import { useDrawingInput } from '../hooks/useDrawingInput.ts';
import type { DrawingMode } from '../hooks/useDrawingInput.ts';
import { usePlayheadPosition } from '../hooks/usePlayheadPosition.ts';
import { useScoreAnnotations } from '../hooks/useScoreAnnotations.ts';
import type { ScoreViewPan } from '../hooks/useScoreViewMemory.ts';
import { useScoreViewport } from '../hooks/useScoreViewport.ts';
import { useScoreViewResources } from '../hooks/useScoreViewResources.ts';
import { useVisibleScorePages } from '../hooks/useVisibleScorePages.ts';
import { annotationGraphicColors } from '../utils/annotationColors.ts';
import { isAnnotationModifierPressed } from '../utils/annotationInput.ts';
import { selectionKey } from '../utils/api.ts';
import type { Api } from '../utils/api.ts';
import { DRAWING_COLORS } from '../utils/drawing.ts';
import {
  BASE_PAGE_WIDTH,
  CANVAS_PADDING,
  CANVAS_TOP_PADDING,
  PAGE_GAP,
} from '../utils/scoreLayout.ts';
import type { ScoreLayout } from '../utils/scoreLayout.ts';
import { hitElement, hitSelection, selectionGroup } from '../utils/selectionGeometry.ts';
import { AnnotationEditor } from './AnnotationEditor.tsx';
import { DrawingLayer } from './DrawingLayer.tsx';
import { ScoreControls } from './ScoreControls.tsx';
import { ScoreLoading } from './ScoreLoading.tsx';
import { ScoreSheet } from './ScoreSheet.tsx';
import type { ScoreSheetActions } from './ScoreSheet.tsx';

interface Props {
  api: Api;
  render: RenderReference;
  layout: ScoreLayout;
  onLayoutChange: (layout: ScoreLayout) => void;
  zoom: number;
  onZoomChange: (zoom: number) => void;
  selected?: SelectionTarget[];
  selectable?: boolean;
  annotations?: SelectionAnnotation[];
  agentEditing?: boolean;
  annotate?: boolean;
  onAnnotationClose?: () => void;
  onAnnotationsChange?: (annotations: SelectionAnnotation[]) => void;
  onSelect?: (target: SelectionTarget, linked?: SelectionTarget[]) => void;
  onPoint?: (page: number, x: number, y: number) => void;
  showHitBoxes?: boolean;
  activeElementIds?: Set<string>;
  playheadElementIds?: Set<string>;
  playhead?: boolean;
  playbackSystems?: PlaybackSystemSpan[];
  onAuditionNote?: (element: GeometryElement, simultaneous: boolean) => void;
  onCueMeasure?: (page: number, measure: number) => void;
  viewKey?: string;
  initialPan?: ScoreViewPan;
  onPanChange?: (x: number, y: number) => void;
  drawing?: DrawingController;
  drawingMode?: DrawingMode;
  penSize?: number;
  eraserSize?: number;
  penColor?: DrawingColor;
}

const EMPTY_IDS = new Set<string>();
const EMPTY_SYSTEMS: PlaybackSystemSpan[] = [];
const EMPTY_PAGES: GeometryPage[] = [];
const EMPTY_SELECTION: SelectionTarget[] = [];
const EMPTY_ANNOTATIONS: SelectionAnnotation[] = [];

export function ScoreView({
  api,
  render,
  layout,
  onLayoutChange,
  zoom,
  onZoomChange,
  selected = EMPTY_SELECTION,
  selectable = false,
  annotations = EMPTY_ANNOTATIONS,
  agentEditing = false,
  annotate = false,
  onAnnotationClose,
  onAnnotationsChange,
  onSelect,
  onPoint,
  showHitBoxes = false,
  activeElementIds = EMPTY_IDS,
  playheadElementIds = EMPTY_IDS,
  playhead = false,
  playbackSystems = EMPTY_SYSTEMS,
  onAuditionNote,
  onCueMeasure,
  viewKey,
  initialPan = { x: 0, y: 0 },
  onPanChange,
  drawing,
  drawingMode = null,
  penSize = 4,
  eraserSize = 20,
  penColor = 'black',
}: Props) {
  const { t } = useI18n();
  const { geometry, error, layouts, missingGeometry, ready } = useScoreViewResources(api, render);
  const locatedPlayhead = usePlayheadPosition(
    render.renderKey,
    geometry?.renderKey === render.renderKey ? geometry.pages : EMPTY_PAGES,
    playbackSystems,
    playheadElementIds,
    playhead,
  );
  const pageHosts = useRef(new Map<number, HTMLDivElement>());
  const canvasRef = useRef<HTMLDivElement>(null);
  const drawingInput = useDrawingInput(drawing, drawingMode, penSize, eraserSize, penColor);
  const [drawingPointer, setDrawingPointer] = useState<{ x: number; y: number } | null>(null);
  const updateDrawingPointer = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!drawingMode) return;
    const session = drawingInput.active.current;
    if (event.pointerType === 'touch' && !session) {
      setDrawingPointer(null);
      return;
    }
    const overPage = document.elementFromPoint(event.clientX, event.clientY)?.closest('.page-host');
    if (!session && !overPage) {
      setDrawingPointer(null);
      return;
    }
    const bounds = session?.bounds;
    const viewportBounds = event.currentTarget.getBoundingClientRect();
    setDrawingPointer({
      x:
        (bounds ? Math.max(bounds.left, Math.min(bounds.right, event.clientX)) : event.clientX) -
        viewportBounds.left,
      y:
        (bounds ? Math.max(bounds.top, Math.min(bounds.bottom, event.clientY)) : event.clientY) -
        viewportBounds.top,
    });
  };
  const { viewport, dragging, modifierDown, pan, panBy, gesture } = useScoreViewport({
    zoom,
    onZoomChange,
    layout,
    pageHosts,
    ready,
    viewKey,
    initialPan,
    panEnabled: !annotate,
    drawingActive: drawingInput.active,
  });
  useEffect(() => {
    const node = viewport.current;
    if (!node || !drawingMode) return;
    const preventStylusGesture = (event: TouchEvent) => {
      if (
        event.cancelable &&
        Array.from(event.changedTouches).some(
          (touch) => 'touchType' in touch && touch.touchType === 'stylus',
        )
      )
        event.preventDefault();
    };
    // iPadOS may intercept consecutive Pencil strokes for handwriting recognition.
    node.addEventListener('touchstart', preventStylusGesture, { passive: false });
    node.addEventListener('touchmove', preventStylusGesture, { passive: false });
    node.addEventListener('touchend', preventStylusGesture, { passive: false });
    return () => {
      node.removeEventListener('touchstart', preventStylusGesture);
      node.removeEventListener('touchmove', preventStylusGesture);
      node.removeEventListener('touchend', preventStylusGesture);
    };
  }, [drawingMode, viewport]);
  useEffect(() => {
    onPanChange?.(pan.x, pan.y);
  }, [onPanChange, pan.x, pan.y]);
  const annotationState = useScoreAnnotations({
    annotate,
    annotations,
    onAnnotationsChange,
    onClose: onAnnotationClose,
    pageHosts,
    pages: geometry?.pages,
    renderKey: render.renderKey,
    viewport,
  });
  const {
    chooseTarget,
    chooseTargets,
    confirm,
    dismiss,
    draft,
    edit,
    editor,
    editorPlacement,
    remove,
  } = annotationState;
  const previewTargets =
    draft?.targets ?? (editor && !editor.annotation ? editor.targets : undefined);
  const previewBoxes = draft?.boxes ?? (editor && !editor.annotation ? editor.boxes : undefined);
  const previewIndex = annotations.length;
  const activeAnnotationIndex = editor?.annotation
    ? annotations.findIndex((annotation) => annotation.id === editor.annotation?.id)
    : -1;
  const graphicPreviewTargets =
    previewTargets ?? (activeAnnotationIndex >= 0 ? editor?.annotation?.targets : undefined);
  const graphicPreviewIndex =
    previewTargets || activeAnnotationIndex < 0 ? previewIndex : activeAnnotationIndex;
  const selectionKeys = useMemo(() => new Set(selected.map(selectionKey)), [selected]);
  const graphicColors = useMemo(
    () =>
      annotationGraphicColors(
        geometry?.pages ?? [],
        annotations,
        graphicPreviewTargets
          ? { index: graphicPreviewIndex, targets: graphicPreviewTargets }
          : undefined,
      ),
    [annotations, geometry?.pages, graphicPreviewIndex, graphicPreviewTargets],
  );
  const currentGeometry =
    geometry?.renderKey === render.renderKey && geometry.sourceHash === render.manifest.sourceHash;
  const canSelect = !drawingMode && selectable && ready && currentGeometry;
  const canAnnotate = !drawingMode && annotate && ready && currentGeometry;
  const canPlay = !drawingMode && !!onAuditionNote && ready && currentGeometry;
  const marquee = useAnnotationMarquee({
    annotate: canAnnotate,
    canvas: canvasRef,
    pages: geometry?.pages,
    pageHosts,
    viewport,
    pan,
    panBy,
    onSelect: chooseTargets,
  });
  const interaction = useRef({
    canSelect,
    canAnnotate,
    canPlay,
    gesture,
    marquee,
    chooseTarget,
    editor,
    dismiss,
    onAuditionNote,
    onCueMeasure,
    onSelect,
    onPoint,
    geometry,
    selectionKeys,
    edit,
  });
  useLayoutEffect(() => {
    interaction.current = {
      canSelect,
      canAnnotate,
      canPlay,
      gesture,
      marquee,
      chooseTarget,
      editor,
      dismiss,
      onAuditionNote,
      onCueMeasure,
      onSelect,
      onPoint,
      geometry,
      selectionKeys,
      edit,
    };
  });
  const actions = useMemo<ScoreSheetActions>(
    () => ({
      pointerDown(pageLayout, event) {
        const current = interaction.current;
        if (!current.canAnnotate || event.button !== 0) return;
        const ctm = event.currentTarget.getScreenCTM();
        if (!ctm) return;
        const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(ctm.inverse());
        if (!hitElement(pageLayout, point.x, point.y)) current.marquee.begin(event);
      },
      click(pageLayout, event) {
        const current = interaction.current;
        if (current.gesture.consumeClick() || current.marquee.consumeClick()) return;
        if (!current.canSelect && !current.canAnnotate && !current.canPlay) return;
        const ctm = event.currentTarget.getScreenCTM();
        if (!ctm) return;
        const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(ctm.inverse());
        const target = hitSelection(pageLayout, point.x, point.y);
        if (current.canAnnotate) {
          if (target)
            current.chooseTarget(
              pageLayout.page,
              pageLayout,
              target,
              isAnnotationModifierPressed(event),
            );
          else if (current.editor) current.dismiss();
          return;
        }
        current.dismiss();
        if (event.metaKey || event.ctrlKey) return;
        if (current.canPlay) {
          const element = hitElement(pageLayout, point.x, point.y);
          if (
            element &&
            ['NoteHead', 'TabNoteHead', 'Rest', 'MultiMeasureRest'].includes(
              element.region.elementKind ?? '',
            ) &&
            element.region.target.kind === 'element'
          ) {
            current.onAuditionNote?.(element, event.altKey);
            return;
          }
          if (target?.kind === 'measure' && current.onCueMeasure) {
            current.onCueMeasure(pageLayout.page, target.locator.measureStart);
            return;
          }
        }
        if (target && current.onSelect)
          current.onSelect(
            target,
            selectionGroup(current.geometry!.pages, target, current.selectionKeys),
          );
        else current.onPoint?.(pageLayout.page, point.x, point.y);
      },
      open(annotation, bounds, page) {
        interaction.current.edit(annotation, bounds, page);
      },
    }),
    [],
  );
  const pageNumbers = useMemo(() => layouts.map(({ page }) => page), [layouts]);
  const [displayedLayouts, setDisplayedLayouts] = useState(new Map<number, GeometryPage>());
  const [paintedPages, setPaintedPages] = useState<Set<number>>(() => new Set());
  const onPageReady = useCallback((pageLayout: GeometryPage) => {
    setDisplayedLayouts((previous) => {
      if (previous.get(pageLayout.page) === pageLayout) return previous;
      return new Map(previous).set(pageLayout.page, pageLayout);
    });
  }, []);
  useEffect(() => {
    if (!displayedLayouts.size) return;
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        setPaintedPages((previous) => {
          const next = new Set(previous);
          for (const page of displayedLayouts.keys()) next.add(page);
          return next.size === previous.size ? previous : next;
        });
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [displayedLayouts]);
  const { visible, mounted } = useVisibleScorePages(
    viewport,
    pageHosts,
    pageNumbers,
    viewKey ?? 'score',
    ready,
    editor?.page,
  );
  useLayoutEffect(() => {
    setDisplayedLayouts((previous) =>
      [...previous.keys()].some((page) => !mounted.has(page))
        ? new Map([...previous].filter(([page]) => mounted.has(page)))
        : previous,
    );
    setPaintedPages((previous) => {
      if (![...previous].some((page) => !mounted.has(page))) return previous;
      return new Set([...previous].filter((page) => mounted.has(page)));
    });
  }, [mounted]);
  const pageRefs = useMemo(
    () =>
      new Map(
        pageNumbers.map((page) => [
          page,
          (node: HTMLDivElement | null) => {
            if (node) pageHosts.current.set(page, node);
            else pageHosts.current.delete(page);
          },
        ]),
      ),
    [pageNumbers],
  );
  return (
    <section
      className={`score-view ${agentEditing ? 'is-agent-editing' : ''}`}
      aria-label={t('scoreLabel')}
    >
      {error && (
        <p className="error" role="alert">
          {t('scoreLoadFailed', { error })}
        </p>
      )}
      {missingGeometry && (
        <p className="error" role="alert">
          {t('scoreLoadFailed', { error: 'missing page geometry' })}
        </p>
      )}
      <div
        ref={viewport}
        className={`score-scroll ${dragging ? 'is-panning' : ''} ${
          modifierDown && !annotate ? 'can-pan' : ''
        } ${annotate ? 'is-annotating' : ''} ${marquee.rect ? 'is-marquee-selecting' : ''} ${drawingMode ? 'is-drawing' : ''} ${drawingMode === 'eraser' || (drawingMode === 'pen' && modifierDown) ? 'is-erasing' : ''}`}
        onPointerDown={(event) => {
          const result = drawingInput.down(event);
          if (result !== 'ignore') updateDrawingPointer(event);
          if (result === 'reset') gesture.resetDirectPointers();
          if (result !== 'ignore') gesture.onPointerDown(event);
        }}
        onPointerMove={(event) => {
          if (!drawingInput.move(event)) {
            gesture.onPointerMove(event);
            updateDrawingPointer(event);
          }
        }}
        onPointerUp={(event) => {
          if (!drawingInput.up(event)) {
            gesture.onPointerUp(event);
            if (event.pointerType === 'touch') setDrawingPointer(null);
          }
        }}
        onPointerCancel={(event) => {
          if (!drawingInput.up(event)) {
            gesture.onPointerCancel(event);
            setDrawingPointer(null);
          }
        }}
        onLostPointerCapture={(event) => {
          if (event.currentTarget.hasPointerCapture(event.pointerId)) return;
          if (!drawingInput.up(event)) gesture.onPointerCancel(event);
        }}
        onPointerLeave={() => {
          if (!drawingInput.active.current) setDrawingPointer(null);
        }}
      >
        {!ready ? (
          <ScoreLoading label={t('loadingScore')} />
        ) : (
          <div
            ref={canvasRef}
            className={`score-canvas ${layout}`}
            style={{
              padding: `${CANVAS_TOP_PADDING}px ${CANVAS_PADDING}px ${CANVAS_PADDING}px`,
            }}
          >
            <div
              className={`score-pages ${layout}`}
              style={{
                gap: PAGE_GAP,
                transform: `translate(${pan.x}px, ${pan.y}px)`,
              }}
            >
              {layouts.map(({ page, layout: pageLayout }) => {
                const displayedLayout = mounted.has(page)
                  ? (displayedLayouts.get(page) ?? pageLayout)
                  : pageLayout;
                const box = displayedLayout?.viewBox;
                if (!box) return null;
                if (!pageLayout) return null;
                const ratio = (box.x1 - box.x0) / (box.y1 - box.y0);
                const width = (BASE_PAGE_WIDTH * zoom) / 100;
                return (
                  <div
                    className={`page-host ${agentEditing ? 'is-agent-editing' : ''}`}
                    key={page}
                    ref={pageRefs.get(page)}
                    data-page={page}
                    role="group"
                    aria-label={t('scoreAlt', { page })}
                    style={{
                      width,
                      height: width / ratio,
                      contentVisibility:
                        mounted.has(page) && !visible.has(page) ? 'auto' : undefined,
                      containIntrinsicSize: `auto ${width}px auto ${width / ratio}px`,
                    }}
                  >
                    {mounted.has(page) && (
                      <ScoreSheet
                        key={page}
                        api={api}
                        renderKey={geometry!.renderKey}
                        pageLayout={pageLayout}
                        priority={visible.has(page) ? 0 : 1}
                        selectionKeys={selectionKeys}
                        graphicColors={graphicColors}
                        activeElementIds={activeElementIds}
                        playhead={locatedPlayhead?.span.page === page ? locatedPlayhead : null}
                        annotations={annotations}
                        previewTargets={previewTargets}
                        previewBoxes={previewBoxes}
                        previewIndex={previewIndex}
                        draftActive={!!draft}
                        interactive={canSelect || canAnnotate || canPlay}
                        showHitBoxes={showHitBoxes}
                        agentEditing={agentEditing}
                        zoom={zoom}
                        actions={actions}
                        onPageReady={onPageReady}
                      />
                    )}
                    {drawing && paintedPages.has(page) && (
                      <DrawingLayer page={page} aspect={1 / ratio} strokes={drawingInput.strokes} />
                    )}
                  </div>
                );
              })}
            </div>
            {marquee.rect && (
              <div
                className="score-marquee-layer"
                aria-hidden="true"
                style={{ transform: `translate(${pan.x}px, ${pan.y}px)` }}
              >
                <div
                  className="score-marquee"
                  style={{
                    left: marquee.rect.x0,
                    top: marquee.rect.y0,
                    width: Math.max(0, marquee.rect.x1 - marquee.rect.x0),
                    height: Math.max(0, marquee.rect.y1 - marquee.rect.y0),
                  }}
                />
              </div>
            )}
          </div>
        )}
      </div>
      {ready && (
        <ScoreControls
          layout={layout}
          onLayout={onLayoutChange}
          zoom={zoom}
          onZoomChange={onZoomChange}
        />
      )}
      {editor &&
        editorPlacement &&
        createPortal(
          <AnnotationEditor
            key={editor.annotation?.id ?? 'new'}
            placement={editorPlacement}
            existing={Boolean(editor.annotation)}
            initialText={editor.annotation?.text ?? ''}
            onConfirm={confirm}
            onDelete={remove}
            onDismiss={dismiss}
          />,
          document.body,
        )}
      {drawingPointer &&
        drawingMode &&
        viewport.current &&
        createPortal(
          <div
            className={
              drawingMode === 'eraser' || modifierDown
                ? 'score-eraser-indicator'
                : 'score-pen-indicator'
            }
            aria-hidden="true"
            style={{
              left: drawingPointer.x,
              top: drawingPointer.y,
              width: drawingMode === 'eraser' || modifierDown ? eraserSize : penSize,
              height: drawingMode === 'eraser' || modifierDown ? eraserSize : penSize,
              backgroundColor:
                drawingMode === 'pen' && !modifierDown ? DRAWING_COLORS[penColor] : undefined,
            }}
          />,
          viewport.current,
        )}
    </section>
  );
}

import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import { describe } from '../../errors.ts';
import type { Locator, RenderGeometry, RenderReference } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { locatorKey } from '../api.ts';
import type { Api } from '../api.ts';
import { ScoreControls } from './ScoreControls.tsx';
import type { ScoreLayout } from './ScoreToolbar.tsx';

interface Props {
  api: Api;
  render: RenderReference;
  layout: ScoreLayout;
  onLayoutChange: (layout: ScoreLayout) => void;
  zoom: number;
  onZoomChange: (zoom: number) => void;
  selected?: Locator[];
  selectable?: boolean;
  onPoint?: (page: number, x: number, y: number) => void;
}

const PAGE_GAP = 24;
const CANVAS_PADDING = 32;
// Page layout is viewport-independent; zoom is the only size control.
const BASE_PAGE_WIDTH = 900;

export function ScoreView({
  api,
  render,
  layout,
  onLayoutChange,
  zoom,
  onZoomChange,
  selected = [],
  selectable = false,
  onPoint,
}: Props) {
  const { t } = useI18n();
  const [geometry, setGeometry] = useState<RenderGeometry | null>(null);
  const [images, setImages] = useState<Record<number, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [modifierDown, setModifierDown] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(zoom);
  const pendingAnchor = useRef<{
    ratioX: number;
    ratioY: number;
    offsetX: number;
    offsetY: number;
  } | null>(null);
  const drag = useRef<{
    pointerId: number;
    x: number;
    y: number;
    left: number;
    top: number;
  } | null>(null);
  const pages = [...render.manifest.pages].sort((left, right) => left.page - right.page);
  const pageKey = pages.map((page) => page.page).join(',');
  useEffect(() => {
    zoomRef.current = zoom;
  }, [zoom]);
  useEffect(() => {
    const syncModifier = (event: KeyboardEvent) => setModifierDown(event.metaKey || event.ctrlKey);
    const clearModifier = () => setModifierDown(false);
    window.addEventListener('keydown', syncModifier);
    window.addEventListener('keyup', syncModifier);
    window.addEventListener('blur', clearModifier);
    return () => {
      window.removeEventListener('keydown', syncModifier);
      window.removeEventListener('keyup', syncModifier);
      window.removeEventListener('blur', clearModifier);
    };
  }, []);
  useEffect(() => {
    let live = true;
    setGeometry(null);
    setError(null);
    api
      .geometry(render.renderKey)
      .then((value) => {
        if (live) setGeometry(value);
      })
      .catch((reason) => {
        if (live) setError(describe(reason));
      });
    return () => {
      live = false;
    };
  }, [api, render.renderKey]);
  useEffect(() => {
    let live = true;
    const urls: string[] = [];
    setImages({});
    setError(null);
    Promise.all(
      pages.map(async (page) => [page.page, await api.page(render.renderKey, page.page)] as const),
    )
      .then((loaded) => {
        if (!live) return;
        const next: Record<number, string> = {};
        for (const [page, blob] of loaded) {
          const url = URL.createObjectURL(blob);
          urls.push(url);
          next[page] = url;
        }
        setImages(next);
      })
      .catch((reason) => {
        if (live) setError(describe(reason));
      });
    return () => {
      live = false;
      urls.forEach((url) => URL.revokeObjectURL(url));
    };
  }, [api, render.renderKey, pageKey]);
  useEffect(() => {
    const node = scroll.current;
    if (!node) return;
    const handleWheel = (event: WheelEvent) => {
      if (!event.metaKey && !event.ctrlKey) return;
      event.preventDefault();
      const direction = event.deltaY < 0 ? 1 : event.deltaY > 0 ? -1 : 0;
      const largeStep = event.deltaMode !== 0 || Math.abs(event.deltaY) >= 24;
      if (!direction) return;
      const scaled = largeStep
        ? zoomRef.current * (direction > 0 ? 1.2 : 1 / 1.2)
        : zoomRef.current + direction * 10;
      const next = Math.min(400, Math.max(25, Math.round(scaled)));
      if (next === zoomRef.current) return;
      const bounds = node.getBoundingClientRect();
      const offsetX = event.clientX - bounds.left;
      const offsetY = event.clientY - bounds.top;
      pendingAnchor.current = {
        ratioX: (node.scrollLeft + offsetX) / Math.max(1, node.scrollWidth),
        ratioY: (node.scrollTop + offsetY) / Math.max(1, node.scrollHeight),
        offsetX,
        offsetY,
      };
      onZoomChange(next);
    };
    node.addEventListener('wheel', handleWheel, { passive: false });
    return () => node.removeEventListener('wheel', handleWheel);
  }, [onZoomChange]);
  useLayoutEffect(() => {
    const node = scroll.current;
    const anchor = pendingAnchor.current;
    if (!node || !anchor) return;
    node.scrollLeft = anchor.ratioX * node.scrollWidth - anchor.offsetX;
    node.scrollTop = anchor.ratioY * node.scrollHeight - anchor.offsetY;
    pendingAnchor.current = null;
  }, [zoom]);
  useLayoutEffect(() => {
    scroll.current?.scrollTo({ left: 0, top: 0 });
  }, [layout]);
  const layouts = pages.map((page) => ({
    page: page.page,
    layout: geometry?.pages.find((candidate) => candidate.page === page.page),
    url: images[page.page],
  }));
  const missingGeometry = !!geometry && layouts.some((page) => !page.layout);
  const ready =
    !!geometry &&
    !missingGeometry &&
    layouts.length > 0 &&
    layouts.every((page) => !!page.url && !!page.layout);
  const keys = new Set(selected.map(locatorKey));
  const canSelect = selectable && ready && geometry?.sourceHash === render.manifest.sourceHash;
  return (
    <section className="score-view" aria-label={t('scoreLabel')}>
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
        ref={scroll}
        className={`score-scroll ${dragging ? 'is-panning' : ''} ${modifierDown ? 'can-pan' : ''}`}
        onPointerDown={(event) => {
          if (event.button !== 0 || (!event.metaKey && !event.ctrlKey)) return;
          event.preventDefault();
          const node = event.currentTarget;
          drag.current = {
            pointerId: event.pointerId,
            x: event.clientX,
            y: event.clientY,
            left: node.scrollLeft,
            top: node.scrollTop,
          };
          node.setPointerCapture(event.pointerId);
          setDragging(true);
        }}
        onPointerMove={(event) => {
          const current = drag.current;
          if (!current || current.pointerId !== event.pointerId) return;
          event.currentTarget.scrollLeft = current.left - (event.clientX - current.x);
          event.currentTarget.scrollTop = current.top - (event.clientY - current.y);
        }}
        onPointerUp={(event) => {
          if (drag.current?.pointerId !== event.pointerId) return;
          drag.current = null;
          setDragging(false);
          if (event.currentTarget.hasPointerCapture(event.pointerId))
            event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onPointerCancel={() => {
          drag.current = null;
          setDragging(false);
        }}
      >
        {!ready ? (
          <p className="score-loading">{t('loadingScore')}</p>
        ) : (
          <div
            className={`score-canvas ${layout}`}
            style={{
              padding: CANVAS_PADDING,
            }}
          >
            <div className={`score-pages ${layout}`} style={{ gap: PAGE_GAP }}>
              {layouts.map(({ page, layout: pageLayout, url }) => {
                const box = pageLayout?.viewBox;
                if (!box) return null;
                const ratio = (box.x1 - box.x0) / (box.y1 - box.y0);
                const width = (BASE_PAGE_WIDTH * zoom) / 100;
                return (
                  <div
                    className="page-host"
                    key={page}
                    style={{
                      width,
                      height: width / ratio,
                    }}
                  >
                    <img
                      className="score-image"
                      src={url}
                      alt={t('scoreAlt', { page })}
                      draggable={false}
                    />
                    <svg
                      className={`overlay ${canSelect ? 'interactive' : ''}`}
                      viewBox={`${box.x0} ${box.y0} ${box.x1 - box.x0} ${box.y1 - box.y0}`}
                      aria-hidden="true"
                      onClick={(event) => {
                        if (!canSelect || event.metaKey || event.ctrlKey) return;
                        const ctm = event.currentTarget.getScreenCTM();
                        if (!ctm) return;
                        const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
                          ctm.inverse(),
                        );
                        onPoint?.(page, point.x, point.y);
                      }}
                    >
                      {(pageLayout?.regions ?? [])
                        .filter((region) => keys.has(locatorKey(region.locator)))
                        .map((region, index) => (
                          <rect
                            key={index}
                            className="selection-box"
                            x={region.bounds.x0}
                            y={region.bounds.y0}
                            width={region.bounds.x1 - region.bounds.x0}
                            height={region.bounds.y1 - region.bounds.y0}
                          />
                        ))}
                    </svg>
                  </div>
                );
              })}
            </div>
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
    </section>
  );
}

import { Music2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import { describe } from '../../errors.ts';
import type { Locator, RenderGeometry, RenderReference } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { locatorKey } from '../api.ts';
import type { Api } from '../api.ts';

interface Props {
  api: Api;
  render: RenderReference;
  selected?: Locator[];
  selectable?: boolean;
  onPoint?: (page: number, x: number, y: number) => void;
  onToggle?: (locator: Locator) => void;
}
export function ScoreView({
  api,
  render,
  selected = [],
  selectable = false,
  onPoint,
  onToggle,
}: Props) {
  const { t } = useI18n();
  const [page, setPage] = useState(1);
  const [geometry, setGeometry] = useState<RenderGeometry | null>(null);
  const [image, setImage] = useState<{ key: string; page: number; url: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const overlay = useRef<SVGSVGElement>(null);
  const [zoom, setZoom] = useState(100);
  const currentPage = render.manifest.pages.some((p) => p.page === page)
    ? page
    : (render.manifest.pages[0]?.page ?? 1);
  useEffect(() => {
    let live = true;
    setGeometry(null);
    api
      .geometry(render.renderKey)
      .then((g) => {
        if (live) setGeometry(g);
      })
      .catch((e) => {
        if (live) setError(describe(e));
      });
    return () => {
      live = false;
    };
  }, [api, render.renderKey]);
  useEffect(() => {
    let live = true;
    let url: string | undefined;
    setError(null);
    api
      .page(render.renderKey, currentPage)
      .then((blob) => {
        if (!live) return;
        url = URL.createObjectURL(blob);
        setImage({ key: render.renderKey, page: currentPage, url });
      })
      .catch((e) => {
        if (live) setError(describe(e));
      });
    return () => {
      live = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [api, render.renderKey, currentPage]);
  const layout = geometry?.pages.find((p) => p.page === currentPage);
  const box = layout?.viewBox;
  const ready = image?.key === render.renderKey && image.page === currentPage;
  const canSelect = selectable && ready && geometry?.sourceHash === render.manifest.sourceHash;
  const keys = new Set(selected.map(locatorKey));
  const regions = layout?.regions ?? [];
  return (
    <section className="score" aria-label={t('scoreLabel')}>
      <div className="score-bar">
        <label>
          {t('page')}{' '}
          <select
            aria-label={t('pageLabel')}
            value={currentPage}
            onChange={(e) => setPage(Number(e.target.value))}
          >
            {render.manifest.pages.map((p) => (
              <option key={p.page} value={p.page}>
                {t('pageOption', { page: p.page })}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t('zoom')}{' '}
          <select
            aria-label={t('zoomLabel')}
            value={zoom}
            onChange={(e) => setZoom(Number(e.target.value))}
          >
            {[75, 100, 125, 150, 200].map((n) => (
              <option key={n} value={n}>
                {n}%
              </option>
            ))}
          </select>
        </label>
        <span className="hint">{t('totalPages', { count: render.manifest.pages.length })}</span>
      </div>
      {error && (
        <p className="error" role="alert">
          {t('scoreLoadFailed', { error })}
        </p>
      )}
      {!ready ? (
        <p className="hint">{t('loadingScore')}</p>
      ) : (
        <div className="page-scroll">
          <div
            className="page-host"
            style={{
              width: `${zoom}%`,
              aspectRatio: box ? `${box.x1 - box.x0} / ${box.y1 - box.y0}` : undefined,
            }}
          >
            <img
              className="score-image"
              src={image.url}
              alt={t('scoreAlt', { page: currentPage })}
              draggable={false}
            />
            {box && (
              <svg
                ref={overlay}
                className={`overlay ${canSelect ? 'interactive' : ''}`}
                viewBox={`${box.x0} ${box.y0} ${box.x1 - box.x0} ${box.y1 - box.y0}`}
                aria-hidden="true"
                onClick={(e) => {
                  if (!canSelect) return;
                  const ctm = overlay.current?.getScreenCTM();
                  if (!ctm) return;
                  const point = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
                  onPoint?.(currentPage, point.x, point.y);
                }}
              >
                {regions
                  .filter((r) => keys.has(locatorKey(r.locator)))
                  .map((r, i) => (
                    <rect
                      key={i}
                      className="selection-box"
                      x={r.bounds.x0}
                      y={r.bounds.y0}
                      width={r.bounds.x1 - r.bounds.x0}
                      height={r.bounds.y1 - r.bounds.y0}
                    />
                  ))}
              </svg>
            )}
          </div>
        </div>
      )}
      {layout && layout.capability !== 'measure' && (
        <p className="hint">
          {t('locateCapability', { capability: layout.capability })}
          {layout.reason ?? t('locateBrowseOnly')}
        </p>
      )}
      {regions.length > 0 && (
        <details className="measure-picker">
          <summary>{t('keyboardSelect')}</summary>
          <div className="measure-options">
            {regions.map((r, i) => (
              <button
                type="button"
                key={`${r.systemIndex}:${i}`}
                disabled={!canSelect}
                aria-pressed={keys.has(locatorKey(r.locator))}
                onClick={() => onToggle?.(r.locator)}
              >
                <Music2 aria-hidden="true" size={14} />
                {t('staff', { staff: r.locator.staffIndex + 1 })} ·{' '}
                {r.locator.measureEnd === r.locator.measureStart
                  ? t('measure', { start: r.locator.measureStart })
                  : t('measureRange', {
                      start: r.locator.measureStart,
                      end: r.locator.measureEnd,
                    })}
              </button>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

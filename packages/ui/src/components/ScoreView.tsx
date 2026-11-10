import { useEffect, useRef, useState } from 'react';

import { describe, locatorKey } from '../api.ts';
import type { Api } from '../api.ts';
import type { Locator, RenderGeometry, RenderReference } from '../generated/protocol.ts';

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
    <section className="score" aria-label="谱面">
      <div className="score-bar">
        <label>
          页码{' '}
          <select
            aria-label="谱面页码"
            value={currentPage}
            onChange={(e) => setPage(Number(e.target.value))}
          >
            {render.manifest.pages.map((p) => (
              <option key={p.page} value={p.page}>
                第 {p.page} 页
              </option>
            ))}
          </select>
        </label>
        <label>
          缩放{' '}
          <select
            aria-label="谱面缩放"
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
        <span className="hint">共 {render.manifest.pages.length} 页</span>
      </div>
      {error && (
        <p className="error" role="alert">
          无法载入谱面：{error}
        </p>
      )}
      {!ready ? (
        <p className="hint">载入谱面…</p>
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
              alt={`乐谱第 ${currentPage} 页；小节选择可使用下方控件`}
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
          小节定位能力：{layout.capability}。
          {layout.reason ?? '此页仅能浏览，请让 Agent 直接检查源码。'}
        </p>
      )}
      {regions.length > 0 && (
        <details className="measure-picker">
          <summary>使用键盘选择小节</summary>
          <div className="measure-options">
            {regions.map((r, i) => (
              <button
                type="button"
                key={`${r.systemIndex}:${i}`}
                disabled={!canSelect}
                aria-pressed={keys.has(locatorKey(r.locator))}
                onClick={() => onToggle?.(r.locator)}
              >
                谱表 {r.locator.staffIndex + 1} · 第 {r.locator.measureStart}
                {r.locator.measureEnd !== r.locator.measureStart
                  ? `–${r.locator.measureEnd}`
                  : ''}{' '}
                小节
              </button>
            ))}
          </div>
        </details>
      )}
    </section>
  );
}

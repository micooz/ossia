// The score surface: one LilyPond SVG page inlined, plus a click-to-locate overlay.
//
// The page arrives as text from the server (the client never owns a `.ly`), so it is injected into
// a wrapper. A click in CSS pixels is converted with `getScreenCTM()` into the SVG's own user
// units, which is the space the server's geometry is expressed in; the overlay reuses the engine's
// viewBox, so a box drawn in user units lands on the right measure at any zoom level.
import { useEffect, useMemo, useState } from 'react';

import type { Api, PageLayout, VersionPayload } from '../api.ts';

interface Props {
  api: Api;
  version: VersionPayload;
  /** Selected measures, keyed `staff:measure`. */
  selected: Set<string>;
  busy: boolean;
  onSelect: (page: number, x: number, y: number) => void;
}

const VIEWBOX = /viewBox="([^"]+)"/;

export function ScoreView({ api, version, selected, busy, onSelect }: Props) {
  const [page, setPage] = useState(1);
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setSvg(null);
    setError(null);
    const rel = version.pages.find((p) => p.page === page);
    if (!rel) return;
    api
      .fetchText(rel.svgUrl)
      .then((text) => {
        if (live) setSvg(text);
      })
      // A page that vanished from the cache (the node was re-rendered) is recoverable by re-rendering.
      .catch((e) => {
        if (live) setError(String((e as Error).message));
      });
    return () => {
      live = false;
    };
  }, [api, page, version]);

  const layout: PageLayout | undefined = useMemo(
    () => version.layout.pages.find((p) => p.page === page),
    [version, page],
  );
  const viewBox = useMemo(() => (svg ? (VIEWBOX.exec(svg)?.[1] ?? null) : null), [svg]);

  function handleClick(ev: React.MouseEvent<HTMLDivElement>) {
    if (busy) return;
    const root = ev.currentTarget.querySelector('svg');
    const ctm = root?.getScreenCTM();
    if (!root || !ctm) return;
    const pt = new DOMPoint(ev.clientX, ev.clientY).matrixTransform(ctm.inverse());
    onSelect(page, pt.x, pt.y);
  }

  return (
    <section className="score">
      <header className="score-bar">
        <div className="tabs">
          {version.pages.map((p) => (
            <button
              key={p.page}
              className={p.page === page ? 'tab on' : 'tab'}
              type="button"
              onClick={() => setPage(p.page)}
            >
              第 {p.page} 页
            </button>
          ))}
        </div>
        <span className="hint">
          引擎 {version.engineTag} · 渲染 {version.renderMs} ms
          {version.log.length ? ` · ${version.log.length} 条提示` : ' · 无警告'}
        </span>
      </header>

      {error ? (
        <p className="error">无法载入谱面：{error}</p>
      ) : !svg ? (
        <p className="hint">载入中…</p>
      ) : (
        <div
          className="page-host"
          onClick={handleClick}
          role="button"
          tabIndex={0}
          aria-label="点击小节以选择"
          title="点击小节进行选择"
        >
          <div className="inlined" dangerouslySetInnerHTML={{ __html: svg }} />
          {layout && viewBox && (
            <svg
              className="overlay"
              viewBox={viewBox}
              preserveAspectRatio="xMidYMid meet"
              aria-hidden
            >
              {layout.systems.flatMap((sys) =>
                layout.columns
                  .filter((col) => col.system === sys.system)
                  .flatMap((col) =>
                    [...selected]
                      // A selected key is `staff:measure`. The grid collapses staff-parallel
                      // columns (layout-join.ts), so a measure column belongs to the whole
                      // system: highlight it on every staff whose measure number matches,
                      // otherwise a click resolved to the lower staff would draw no box at all.
                      .filter((k) => k.endsWith(`:${col.measure}`))
                      .map((key) => {
                        const staff = Number(key.split(':')[0]);
                        const st = sys.staffs[staff] ?? sys.staffs[0];
                        if (!st) return [];
                        return (
                          <rect
                            key={`${key}@${sys.system}:${col.column}`}
                            className="sel"
                            x={col.x0}
                            y={st.top}
                            width={Math.max(0.5, col.x1 - col.x0)}
                            height={st.bot - st.top}
                            rx={0.6}
                          />
                        );
                      }),
                  ),
              )}
            </svg>
          )}
        </div>
      )}
    </section>
  );
}

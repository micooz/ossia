// Render one version on demand (chain 1 "首次渲染", chain 4 "回退后重新渲染").
// POST starts a job so a ~10 s SVG render never holds an HTTP request open.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, ok, fail, startJob } from '@/server/http.ts';
import { rendered, peekRendered, renderSummary, type ScoreEntry } from '@/server/score.ts';
import { getDocument, getVersion } from '@/server/store.ts';

const url = (req: Request) => new URL(req.url);

export async function GET(req: Request, ctx: { params: Promise<{ docId: string; hash: string }> }) {
  return withApi(req, async () => {
    const { docId, hash } = await ctx.params;
    const doc = getDocument(docId);
    if (!doc) return fail(404, 'no-such-document', docId);
    if (!getVersion(docId, hash)) return fail(404, 'no-such-version', hash);
    const e = peekRendered(hash);
    if (!e) return ok({ pending: true });
    return ok(versionPayload(docId, hash, e));
  });
}

export async function POST(
  req: Request,
  ctx: { params: Promise<{ docId: string; hash: string }> },
) {
  return withApi(req, async () => {
    const { docId, hash } = await ctx.params;
    const doc = getDocument(docId);
    if (!doc) return fail(404, 'no-such-document', docId);
    if (!getVersion(docId, hash)) return fail(404, 'no-such-version', hash);
    const done = peekRendered(hash);
    if (done && !url(req).searchParams.has('force')) {
      return ok({ jobId: null, done: versionPayload(docId, hash, done) });
    }
    const { jobId } = startJob(
      'render',
      2,
      () => rendered(hash, doc.engine_tag, { force: !!done }),
      renderSummary,
    );
    return ok({ jobId }, { status: 202 });
  });
}

/** Geometry + measure ranges that accompany a version — never source text (G6). */
export function versionPayload(docId: string, hash: string, e: ScoreEntry) {
  const t = e.table;
  const base = `/api/v1/documents/${docId}/versions/${hash}`;
  return {
    hash,
    engineTag: e.record.engineTag,
    renderMs: Math.round(e.record.ms),
    log: e.record.log,
    pageCount: e.record.pages.length,
    pages: e.record.pages.map((p) => ({ page: p.page, svgUrl: `${base}/svg/${p.page}` })),
    pdfUrl: e.record.pdf ? `${base}/pdf` : null,
    midiUrl: e.record.midi ? `${base}/midi` : null,
    // Geometry per page: the client clicks in one page's coordinate space and draws the selection
    // box there. `columns.measure` is the GLOBAL measure number (the grid join is measured in
    // layout-join.ts), which is what a locator is expressed in; `column` is the local index that
    // `hitTest` reports, kept so a client can debug a mis-hit without the source.
    layout: {
      pageCount: t.pages.length,
      staffSpace: t.grid.staffSpace || null,
      pages: t.pages.map((pg, i) => ({
        page: i + 1,
        staffSpace: pg.layout.staffSpace,
        systems: pg.layout.systems.map((sys, si) => ({
          system: si,
          top: sys.top,
          bot: sys.bot,
          barlines: sys.barXs,
          measures: Math.max(0, sys.barXs.length - 1),
          staffs: sys.staffs.map((st) => ({ top: st.top, bot: st.bot, x0: st.x0, x1: st.x1 })),
        })),
        columns: t.grid.columns
          .filter((c) => c.page === i + 1)
          .map((c) => ({
            system: c.system,
            staff: c.staff,
            measure: c.measure,
            column: c.column,
            x0: c.x0,
            x1: c.x1,
          })),
      })),
    },
    // L1 truth: which voice is engraved on which staff, and how many measures each voice holds.
    staves: t.staffs.map((s) => ({ staff: s.staff, voices: s.voices.map((v) => v.name) })),
    measures: Object.fromEntries(
      Object.entries(t.measures).map(([k, ms]) => [
        k,
        {
          count: ms.length,
          maxMeasure: ms.length ? ms[ms.length - 1]!.measureEnd : 0,
        },
      ]),
    ),
  };
}

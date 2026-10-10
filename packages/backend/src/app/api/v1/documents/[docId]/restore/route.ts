// Chain 4 — restore: a new chain node that points at an older version's content, then re-render.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, ok, fail, startJob } from '@/server/http.ts';
import { rendered, renderSummary } from '@/server/score.ts';
import { getDocument, getVersion, restore } from '@/server/store.ts';

export async function POST(req: Request, ctx: { params: Promise<{ docId: string }> }) {
  return withApi(req, async () => {
    const { docId } = await ctx.params;
    const doc = getDocument(docId);
    if (!doc) return fail(404, 'no-such-document', docId);
    const body = (await req.json().catch(() => null)) as {
      baseHash?: string;
      targetHash?: string;
    } | null;
    if (!body?.baseHash || !body.targetHash)
      return fail(400, 'bad-request', 'expects { baseHash, targetHash }');
    if (!getVersion(docId, body.targetHash)) return fail(404, 'no-such-version', body.targetHash);
    const r = restore({ docId, baseHash: body.baseHash, targetHash: body.targetHash });
    if (!r.ok) return fail(r.reason === 'conflict' ? 409 : 404, r.reason, 'restore rejected');
    // Re-render the restored content eagerly so the next poll finds pages ready (chain 4).
    const { jobId } = startJob('render', 1, () =>
      rendered(r.version.content_hash, doc.engine_tag).then(renderSummary),
    );
    return ok(
      {
        jobId,
        version: { seq: r.version.seq, hash: r.version.content_hash, origin: r.version.origin },
      },
      { status: 202 },
    );
  });
}

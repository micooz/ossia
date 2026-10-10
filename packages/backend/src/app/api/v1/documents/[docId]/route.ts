// Document metadata + the version chain (history view). Never returns source text.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, ok, fail } from '@/server/http.ts';
import { peekRendered } from '@/server/score.ts';
import { getDocument, listVersions } from '@/server/store.ts';

export async function GET(req: Request, ctx: { params: Promise<{ docId: string }> }) {
  return withApi(req, async () => {
    const { docId } = await ctx.params;
    const doc = getDocument(docId);
    if (!doc) return fail(404, 'no-such-document', docId);
    const list = listVersions(docId);
    // The head is the *tip node*, not "content equal to head_hash": a restore reuses an old
    // content hash, so comparing hashes would mark two rows head at once.
    const headSeq = Math.max(0, ...list.map((v) => v.seq));
    const versions = list.map((v) => ({
      seq: v.seq,
      hash: v.content_hash,
      parentHash: v.parent_hash,
      origin: v.origin,
      author: v.author,
      createdAt: v.created_at,
      isHead: v.seq === headSeq,
      meta: v.meta ? JSON.parse(v.meta) : null,
      // Pages already rendered for this node: the history view can show a real preview without
      // paying for another engine run.
      preview: (() => {
        const e = peekRendered(v.content_hash);
        return e
          ? {
              pageCount: e.record.pages.length,
              ms: e.record.ms,
              svgUrl: `/api/v1/documents/${docId}/versions/${v.content_hash}/svg/1`,
            }
          : null;
      })(),
    }));
    return ok({
      document: {
        id: doc.id,
        title: doc.title,
        headHash: doc.head_hash,
        engineTag: doc.engine_tag,
        baseline: !!doc.baseline_hash,
        createdAt: doc.created_at,
        updatedAt: doc.updated_at,
      },
      versions,
    });
  });
}

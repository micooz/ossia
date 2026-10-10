// Service E (MVP scope) — export PDF and MIDI of a version. MusicXML patch replay is deferred:
// it needs the unwritten ADR-0003, so the MVP does not pretend to support it.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, fail } from '@/server/http.ts';
import { rendered, peekBinary } from '@/server/score.ts';
import { getDocument, getVersion } from '@/server/store.ts';

const FORMATS = ['pdf', 'midi'] as const;

export async function GET(
  req: Request,
  ctx: { params: Promise<{ docId: string; format: string }> },
) {
  return withApi(req, async () => {
    const { docId, format } = await ctx.params;
    if (!FORMATS.includes(format as (typeof FORMATS)[number])) {
      return fail(400, 'unsupported-format', `format must be one of ${FORMATS.join(', ')}`, {
        formats: FORMATS,
      });
    }
    const doc = getDocument(docId);
    if (!doc) return fail(404, 'no-such-document', docId);
    const url = new URL(req.url);
    const hash = url.searchParams.get('hash') || doc.head_hash;
    if (!getVersion(docId, hash)) return fail(404, 'no-such-version', hash);
    const kind = format as 'pdf' | 'midi';
    let buf = peekBinary(hash, kind);
    if (!buf) {
      // Export is where a render is paid for if the version was never viewed.
      await rendered(hash, doc.engine_tag);
      buf = peekBinary(hash, kind);
    }
    if (!buf) return fail(500, 'export-failed', 'the engine produced no artifact for this version');
    const name = `${(doc.title || 'score').replace(/[^\w.-]+/g, '_')}-${hash.slice(0, 8)}.${kind === 'pdf' ? 'pdf' : 'midi'}`;
    return new Response(new Uint8Array(buf), {
      headers: {
        'content-type': kind === 'pdf' ? 'application/pdf' : 'audio/midi',
        'content-disposition': `attachment; filename="${name}"`,
      },
    });
  });
}

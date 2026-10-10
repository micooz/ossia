export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, fail } from '@/server/http.ts';
import { peekBinary } from '@/server/score.ts';
import { getDocument } from '@/server/store.ts';

export async function GET(req: Request, ctx: { params: Promise<{ docId: string; hash: string }> }) {
  return withApi(req, async () => {
    const { docId, hash } = await ctx.params;
    const buf = peekBinary(hash, 'pdf');
    if (!buf) return fail(404, 'not-rendered', 'render this version before downloading the PDF');
    return new Response(new Uint8Array(buf), {
      headers: {
        'content-type': 'application/pdf',
        'content-disposition': `inline; filename="ossia-${hash.slice(0, 8)}.pdf"`,
      },
    });
  });
}

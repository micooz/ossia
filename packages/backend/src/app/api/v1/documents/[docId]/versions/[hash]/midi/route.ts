export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, fail } from '@/server/http.ts';
import { peekBinary } from '@/server/score.ts';

export async function GET(req: Request, ctx: { params: Promise<{ docId: string; hash: string }> }) {
  return withApi(req, async () => {
    const { hash } = await ctx.params;
    const buf = peekBinary(hash, 'midi');
    if (!buf) return fail(404, 'not-rendered', 'render this version before playing it back');
    return new Response(new Uint8Array(buf), {
      headers: {
        'content-type': 'audio/midi',
        'content-disposition': `attachment; filename="ossia-${hash.slice(0, 8)}.midi"`,
      },
    });
  });
}

// Serve one rendered SVG page. The page is the engraving result of a content hash, so it is
// immutable and can be cached forever by the client.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, fail } from '@/server/http.ts';
import { peekPage } from '@/server/score.ts';

export async function GET(
  req: Request,
  ctx: { params: Promise<{ docId: string; hash: string; page: string }> },
) {
  return withApi(req, async () => {
    const { hash, page } = await ctx.params;
    const svg = peekPage(hash, Number(page));
    if (svg == null) return fail(404, 'no-such-page', `page ${page} of ${hash} is not rendered`);
    return new Response(svg, {
      headers: {
        'content-type': 'image/svg+xml; charset=utf-8',
        'cache-control': 'immutable, max-age=31536000',
      },
    });
  });
}

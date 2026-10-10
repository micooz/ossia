// Chain 2 — selection: page-space point -> locator. Geometry only, no source (G6).
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { locate, EditError } from '@/server/edit.ts';
import { withApi, ok, fail } from '@/server/http.ts';

export async function POST(req: Request, ctx: { params: Promise<{ docId: string }> }) {
  return withApi(req, async () => {
    const { docId } = await ctx.params;
    const body = (await req.json().catch(() => null)) as {
      hash?: string;
      page?: number;
      x?: number;
      y?: number;
    } | null;
    if (!body?.hash || typeof body.x !== 'number' || typeof body.y !== 'number') {
      return fail(400, 'bad-request', 'expects { hash, page?, x, y } with x/y in SVG page units');
    }
    try {
      return ok({ locator: locate(docId, body.hash, body.page ?? 1, body.x, body.y) });
    } catch (e) {
      if (e instanceof EditError)
        return fail(e.code === 'no-such-document' ? 404 : 422, e.code, e.message, e.detail ?? {});
      throw e;
    }
  });
}

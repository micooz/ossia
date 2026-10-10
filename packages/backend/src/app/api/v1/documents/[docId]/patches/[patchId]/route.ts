// Candidate lifecycle: read a task, accept it (CAS commit) or discard it. The audit row keeps the
// instruction and the gate report even after a discard (G2: nothing unverified is applied, and
// nothing verified is lost).
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { acceptPatch, discardPatch, EditError } from '@/server/edit.ts';
import { withApi, ok, fail } from '@/server/http.ts';
import { getPatch, patchView } from '@/server/store.ts';

export async function GET(
  req: Request,
  ctx: { params: Promise<{ docId: string; patchId: string }> },
) {
  return withApi(req, async () => {
    const { docId, patchId } = await ctx.params;
    const p = getPatch(patchId);
    if (!p || p.doc_id !== docId) return fail(404, 'no-such-patch', patchId);
    return ok(patchView(p));
  });
}

export async function POST(
  req: Request,
  ctx: { params: Promise<{ docId: string; patchId: string }> },
) {
  return withApi(req, async () => {
    const { docId, patchId } = await ctx.params;
    const body = (await req.json().catch(() => null)) as { action?: string } | null;
    const action = body?.action;
    if (action !== 'accept' && action !== 'discard')
      return fail(400, 'bad-request', 'action must be accept or discard');
    try {
      return ok(action === 'accept' ? acceptPatch(docId, patchId) : discardPatch(docId, patchId));
    } catch (e) {
      if (e instanceof EditError)
        return fail(e.code === 'conflict' ? 409 : 404, e.code, e.message, e.detail ?? {});
      throw e;
    }
  });
}

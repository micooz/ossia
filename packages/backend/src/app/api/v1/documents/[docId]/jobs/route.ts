// Chain 3 — start a natural-language edit task. Returns a job id; the UI polls /jobs/{id}.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { runEdit, EditError, type Locator } from '@/server/edit.ts';
import { withApi, ok, fail, startJob, jobView } from '@/server/http.ts';

export async function POST(req: Request, ctx: { params: Promise<{ docId: string }> }) {
  return withApi(req, async () => {
    const { docId } = await ctx.params;
    const body = (await req.json().catch(() => null)) as {
      baseHash?: string;
      locators?: Locator[];
      instruction?: string;
      model?: string;
      accept?: boolean;
    } | null;
    if (
      !body?.baseHash ||
      !Array.isArray(body.locators) ||
      !body.locators.length ||
      !body.instruction
    ) {
      return fail(
        400,
        'bad-request',
        'expects { baseHash, locators: [{staff|voice, measureStart, measureEnd?}], instruction }',
      );
    }
    // Two phases are visible to the client: model+gates, then the result render.
    const { jobId } = startJob('edit', 2, (job) =>
      runEdit({
        docId,
        baseHash: body.baseHash!,
        locators: body.locators!,
        instruction: body.instruction!,
        model: body.model,
        accept: !!body.accept,
        onPhase: (phase, step) => {
          job.phase = phase;
          job.step = step;
        },
      }),
    );
    return ok({ jobId }, { status: 202 });
  });
}

export async function GET(req: Request, ctx: { params: Promise<{ docId: string }> }) {
  return withApi(req, async () => {
    const { docId } = await ctx.params;
    const url = new URL(req.url);
    const id = url.searchParams.get('jobId');
    if (!id) return fail(400, 'bad-request', 'expects ?jobId=');
    const job = (await import('@/server/http.ts')).getJob(id);
    if (!job) return fail(404, 'no-such-job', id);
    return ok(jobView(job));
  });
}

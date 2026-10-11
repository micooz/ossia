// Job polling: 250 ms rather than SSE, because the SPA is one build shared by
// macOS/Windows/iOS/Android, where EventSource over a custom protocol is a portability risk.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { withApi, ok, fail, getJob, jobView } from '@/server/http.ts';

export async function GET(req: Request, ctx: { params: Promise<{ jobId: string }> }) {
  return withApi(req, async () => {
    const { jobId } = await ctx.params;
    const job = getJob(jobId);
    if (!job) return fail(404, 'no-such-job', jobId);
    return ok(jobView(job));
  });
}

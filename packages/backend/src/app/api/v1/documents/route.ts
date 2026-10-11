// Service A surface: import a MusicXML/MIDI baseline (POST) and list documents (GET).
// Chain 1 of the core flow (see .docs): musicxml2ly/midi2ly -> `.ly` -> semantic check -> version
// chain -> first render. Source and baseline never leave the server (G6/D9).
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
import { createHash } from 'node:crypto';

import { readLock } from '@/server/engine.ts';
import { withApi, ok, fail, startJob } from '@/server/http.ts';
import { checkImport, type ImportReport } from '@/server/import-gate.ts';
import { musicxmlToLy, midiToLy, RenderError } from '@/server/render.ts';
import { rendered, renderSummary } from '@/server/score.ts';
import { createDocument, listDocuments, getText } from '@/server/store.ts';

/** Stable id for one warning line, so a confirmation can name the exact items it accepts. */
export const digest = (w: string): string =>
  createHash('sha256').update(w).digest('hex').slice(0, 12);

const reportView = (r: ImportReport) => ({
  status: r.status,
  measures: r.measures,
  voices: r.voices,
  noteEvents: r.noteEvents,
  blocking: r.blocking.map((w) => ({
    warning: w,
    digest: digest(w),
    reason: 'timeline-compressed',
  })),
  losses: r.losses.map((w) => ({ warning: w, digest: digest(w) })),
  notes: r.notes,
  hint:
    r.status === 'refused'
      ? 'Negative skip 会压缩/错位时间线，§3.11 明确「不能以人工确认放行」：需声部预处理后重转（ADR-OPEN-2）或拒绝该文件。'
      : r.status === 'needs-confirmation'
        ? '转换器报告了局部损失，确认后入库：重新 POST 同一文件并带 confirm=<digest,…> 或 confirm=all'
        : null,
});

export async function GET(req: Request) {
  return withApi(req, () =>
    ok({
      documents: listDocuments().map((d) => ({
        id: d.id,
        title: d.title,
        headHash: d.head_hash,
        engineTag: d.engine_tag,
        createdAt: d.created_at,
        updatedAt: d.updated_at,
      })),
    }),
  );
}

export async function POST(req: Request) {
  return withApi(req, async () => {
    const form = await req.formData().catch(() => null);
    const file = form?.get('file');
    if (!(file instanceof File))
      return fail(400, 'missing-file', 'multipart field "file" is required');
    const bytes = Buffer.from(await file.arrayBuffer());
    if (!bytes.length) return fail(400, 'empty-file', 'uploaded file is empty');
    const name = file.name || 'input.xml';
    const isMidi = /\.(midi?|mid)$/i.test(name);
    const title = (name.replace(/\.[^.]+$/, '') || 'untitled').slice(0, 200);
    const engineTag = readLock().tag;
    // §6.1: a warning never enters silently. The client re-POSTs with the digests it acknowledged;
    // `all` covers the localised-loss class, which has no per-item decision to make.
    const confirm = String(form?.get('confirm') ?? '');
    const ack =
      confirm === 'all'
        ? new Set(['all'])
        : new Set(
            confirm
              .split(',')
              .map((s) => s.trim())
              .filter(Boolean),
          );

    let ly: string;
    let log: string[];
    let baseline: Buffer | undefined;
    try {
      if (isMidi) {
        const r = await midiToLy(bytes);
        ly = r.ly;
        log = r.log;
      } else {
        const r = await musicxmlToLy(bytes, name);
        ly = r.ly;
        log = r.log;
        baseline = bytes;
      }
    } catch (e) {
      if (e instanceof RenderError) return fail(422, 'convert-failed', e.message, { log: e.log });
      throw e;
    }

    const check = checkImport(ly, log);
    if (check.status === 'refused') {
      // Never confirmable: the loss is musical, not cosmetic, so no amount of user clicking makes
      // the stored source faithful to the baseline (§3.11 conclusion 1).
      return fail(
        422,
        'needs-review',
        'import refused: converter reported blocking problems',
        reportView(check),
      );
    }
    if (check.status === 'needs-confirmation') {
      const needed = check.warnings.map(digest);
      const granted = ack.has('all') || needed.every((d) => ack.has(d));
      if (!granted) {
        return fail(
          422,
          'confirm-required',
          'import requires review: converter reported localised losses',
          { ...reportView(check), requiredConfirm: needed },
        );
      }
    }

    const { doc, version } = createDocument({
      title,
      ly,
      baseline,
      engineTag,
      meta:
        check.status === 'clean'
          ? null
          : {
              importWarnings: 'acknowledged',
              blocking: check.blocking.length,
              losses: check.losses.length,
            },
    });
    // The head node carries no preview yet; render it now so the first view does not wait.
    const { jobId } = startJob('render', 2, () =>
      rendered(version.content_hash, doc.engine_tag).then(renderSummary),
    );
    return ok(
      {
        document: {
          id: doc.id,
          title: doc.title,
          headHash: doc.head_hash,
          engineTag: doc.engine_tag,
          createdAt: doc.created_at,
        },
        version: { seq: version.seq, hash: version.content_hash, origin: version.origin },
        stats: reportView(check),
        acknowledged: check.status !== 'clean',
        render: { jobId },
        bytes: getText(version.content_hash).length,
      },
      { status: 201 },
    );
  });
}

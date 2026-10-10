// Liveness only. The product surface lives under /api/v1; the desktop shell polls this while it
// waits for `next start` to bind, so it must not touch the engine or the database.
export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export function GET() {
  return Response.json({ ok: true, service: 'ossia-api', version: 'v1', pid: process.pid });
}

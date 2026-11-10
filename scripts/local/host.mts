import { randomBytes } from 'node:crypto';
import fs from 'node:fs';
import { createServer, type ServerResponse } from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { fixtures, hash, json, write } from './common.mts';

const viewIds = ['00000000-0000-4000-8000-000000000001', '00000000-0000-4000-8000-000000000002'];
const secret = () => randomBytes(24).toString('hex');
export async function startHostProbe(
  options: { port?: number; ttlMs?: number; reportPath?: string; now?: () => number } = {},
) {
  const now = options.now ?? Date.now;
  const ttl = options.ttlMs ?? 300_000;
  const views = new Map(
    viewIds.map((viewId) => [
      viewId,
      { viewId, selection: [] as number[], streams: new Set<ServerResponse>() },
    ]),
  );
  const launches = new Map(
    viewIds.map((viewId) => [viewId, { credential: secret(), expires: now() + ttl, used: false }]),
  );
  const tokens = new Map<string, { viewId: string; expires: number }>();
  const events: Record<string, unknown>[] = [];
  const sourceHash = hash(fs.readFileSync(path.join(fixtures, 'inputs/polyphonic-pages.ly')));
  let origin = '';
  function record(kind: string, viewId?: string, detail?: unknown) {
    events.push({ seq: events.length + 1, kind, viewId, detail });
    if (options.reportPath)
      write(options.reportPath, json({ formatVersion: 1, probeOnly: true, events }));
  }
  const server = createServer(async (req, res) => {
    res.setHeader('Cache-Control', 'no-store');
    res.setHeader('X-Content-Type-Options', 'nosniff');
    res.setHeader('Referrer-Policy', 'no-referrer');
    res.setHeader(
      'Content-Security-Policy',
      "default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; require-trusted-types-for 'script'",
    );
    const send = (status: number, value: unknown) => {
      res.writeHead(status, { 'Content-Type': 'application/json' });
      res.end(json(value));
    };
    try {
      if (req.headers.host !== new URL(origin).host) return send(403, { error: 'host' });
      const url = new URL(req.url ?? '/', origin);
      if (req.headers.origin && req.headers.origin !== origin)
        return send(403, { error: 'origin' });
      const assets: Record<string, [string, string]> = {
        '/': ['host.html', 'text/html; charset=utf-8'],
        '/probe.js': ['host-browser.js', 'application/javascript'],
        '/probe.css': ['host.css', 'text/css'],
      };
      const asset = assets[url.pathname];
      if (req.method === 'GET' && asset) {
        res.writeHead(200, { 'Content-Type': asset[1] });
        return res.end(fs.readFileSync(new URL(asset[0], import.meta.url)));
      }
      if (req.method === 'POST' && req.headers.origin !== origin)
        return send(403, { error: 'origin' });
      const readBody = async () => {
        let body = '';
        for await (const chunk of req) {
          body += String(chunk);
          if (Buffer.byteLength(body) > 4096) throw new Error('body-too-large');
        }
        return JSON.parse(body) as Record<string, unknown>;
      };
      if (url.pathname === '/bootstrap' && req.method === 'POST') {
        const data = await readBody();
        const viewId = typeof data.viewId === 'string' ? data.viewId : '';
        const launch = launches.get(viewId);
        if (
          !launch ||
          launch.used ||
          launch.expires <= now() ||
          launch.credential !== data.credential
        ) {
          record('bootstrap-denied', viewId);
          return send(401, { error: 'expired-or-invalid-launch' });
        }
        launch.used = true;
        const token = secret();
        tokens.set(token, { viewId, expires: now() + 3_600_000 });
        record('bootstrap', viewId);
        return send(200, { token, viewId });
      }
      const auth = tokens.get((req.headers.authorization ?? '').replace(/^Bearer /, ''));
      if (!auth || auth.expires <= now()) return send(401, { error: 'unauthorized' });
      const viewId = url.searchParams.get('viewId') ?? '';
      if (auth.viewId !== viewId) return send(403, { error: 'wrong-view' });
      const view = views.get(viewId)!;
      const state = () => ({ viewId, sourceHash, selection: view.selection });
      if (url.pathname === '/state' && req.method === 'GET') return send(200, state());
      if (url.pathname === '/report' && req.method === 'GET') return send(200, { events });
      if (url.pathname === '/select' && req.method === 'POST') {
        const body = await readBody();
        if (!Number.isInteger(body.measure) || Number(body.measure) < 1 || Number(body.measure) > 4)
          return send(400, { error: 'invalid-measure' });
        const measure = Number(body.measure);
        view.selection = view.selection.includes(measure)
          ? view.selection.filter((m) => m !== measure)
          : [...view.selection, measure].sort((a, b) => a - b);
        record('selection', viewId, view.selection);
        for (const stream of view.streams) stream.write(`data: ${JSON.stringify(state())}\n\n`);
        return send(200, state());
      }
      if (url.pathname === '/events' && req.method === 'GET') {
        res.writeHead(200, { 'Content-Type': 'text/event-stream', Connection: 'keep-alive' });
        view.streams.add(res);
        record('stream-open', viewId);
        res.write(`data: ${JSON.stringify(state())}\n\n`);
        const heartbeat = setInterval(() => res.write(': heartbeat\n\n'), 15_000);
        res.on('close', () => {
          clearInterval(heartbeat);
          view.streams.delete(res);
          record('stream-close', viewId);
        });
        return;
      }
      send(404, { error: 'not-found' });
    } catch {
      if (!res.headersSent) send(400, { error: 'invalid-request' });
      else res.end();
    }
  });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(options.port ?? 0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listening address');
  origin = `http://127.0.0.1:${address.port}`;
  return {
    origin,
    launches: viewIds.map((viewId) => ({
      viewId,
      credential: launches.get(viewId)!.credential,
      url: `${origin}/?viewId=${viewId}#start=${launches.get(viewId)!.credential}`,
    })),
    events,
    close: async () => {
      server.closeAllConnections();
      await new Promise<void>((resolve, reject) =>
        server.close((e) => (e ? reject(e) : resolve())),
      );
    },
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const probe = await startHostProbe({
    port: Number(process.env.OSSIA_PROBE_PORT ?? 0),
    reportPath: path.join(fixtures, 'evidence/host-session.json'),
  });
  console.log(
    'Temporary host probe; launch URLs expire after five minutes. Credentials are not written to disk.',
  );
  for (const launch of probe.launches) console.log(launch.url);
  const stop = () => {
    void probe.close();
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}

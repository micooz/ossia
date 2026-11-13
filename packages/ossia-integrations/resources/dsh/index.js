import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

export const inject = ['webServer'];

const CHANNEL = '/ossia-selection';
const RECONNECT_MS = 1000;
const DISCOVERY_MS = 2000;

export async function apply(ctx) {
  const config = JSON.parse(await readFile(new URL('./config.json', import.meta.url), 'utf8'));
  if (typeof config.home !== 'string' || config.home.length === 0) {
    throw new Error('ossia-dsh-plugin: config.json must contain an absolute Ossia home');
  }

  const epoch = randomUUID();
  let revision = 0;
  let latest = { epoch, revision, count: null, documentId: null, at: 0 };
  const bridge = new SelectionBridge(config.home, (count, documentId) => {
    revision += 1;
    latest = { epoch, revision, count, documentId, at: Date.now() };
  });

  ctx.effect(
    () =>
      ctx.webServer.register({
        kind: 'prefix',
        path: CHANNEL,
        handler: (request, response) => handleRpc(request, response, () => latest),
      }),
    'ossia-selection: host rpc',
  );

  ctx.effect(() => {
    bridge.start();
    return () => bridge.stop();
  }, 'ossia-selection: event bridge');
}

class SelectionBridge {
  constructor(home, publish) {
    this.home = home;
    this.publish = publish;
    this.controller = null;
  }

  start() {
    if (this.controller !== null) return;
    this.controller = new AbortController();
    void this.run(this.controller.signal);
  }

  stop() {
    this.controller?.abort();
    this.controller = null;
  }

  async run(signal) {
    while (!signal.aborted) {
      const delay = await this.once(signal);
      if (!signal.aborted) await sleep(delay, signal);
    }
  }

  async once(signal) {
    const discovery = await readDiscovery(this.home);
    if (discovery === null) return DISCOVERY_MS;
    try {
      const status = await json(`${discovery.runtime.webBaseUrl}/api/v1/status`, signal);
      if (!Number.isInteger(status.seq) || status.seq < 0)
        throw new Error('Ossia status has no event cursor');
      await this.stream(
        discovery.runtime.webBaseUrl,
        discovery.runtime.instanceId,
        status.seq,
        signal,
      );
    } catch {
      if (signal.aborted) return RECONNECT_MS;
    }
    return RECONNECT_MS;
  }

  async stream(base, instanceId, after, signal) {
    const response = await fetch(
      `${base}/api/v1/events?instanceId=${encodeURIComponent(instanceId)}&after=${after}`,
      { headers: { accept: 'text/event-stream' }, signal },
    );
    if (!response.ok || response.body === null) {
      throw new Error(`Ossia event stream failed with HTTP ${response.status}`);
    }
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) return;
        buffer += decoder.decode(value, { stream: true });
        if (buffer.length > 1024 * 1024) return;
        let boundary;
        while ((boundary = /\r?\n\r?\n/.exec(buffer)) !== null) {
          const frame = buffer.slice(0, boundary.index);
          buffer = buffer.slice(boundary.index + boundary[0].length);
          const event = parseFrame(frame);
          if (event.event === 'resync-required') return;
          if (event.event !== 'runtime' || event.data.length === 0) continue;
          let eventValue;
          try {
            eventValue = JSON.parse(event.data);
          } catch {
            continue;
          }
          if (eventValue?.type !== 'selection-changed') continue;
          const count = eventValue.payload?.targetCount;
          if (!Number.isInteger(count) || count < 0) continue;
          this.publish(
            count,
            typeof eventValue.documentId === 'string' ? eventValue.documentId : null,
          );
        }
      }
    } finally {
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
}

async function readDiscovery(home) {
  try {
    const value = JSON.parse(await readFile(join(home, 'run', 'runtime.json'), 'utf8'));
    if (
      typeof value?.runtime?.webBaseUrl !== 'string' ||
      typeof value.runtime.instanceId !== 'string'
    ) {
      return null;
    }
    return value;
  } catch {
    return null;
  }
}

async function json(url, signal) {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Ossia status failed with HTTP ${response.status}`);
  return response.json();
}

function parseFrame(frame) {
  let event = '';
  const data = [];
  for (const line of frame.split(/\r?\n/)) {
    if (line.startsWith('event:')) event = line.slice(6).trim();
    else if (line.startsWith('data:')) data.push(line.slice(5).replace(/^ /, ''));
  }
  return { event, data: data.join('\n') };
}

function handleRpc(request, response, readLatest) {
  if (request.method !== 'POST') {
    response.writeHead(405);
    response.end();
    return;
  }
  if (
    request.headers['content-type']?.split(';', 1)[0]?.trim().toLowerCase() !== 'application/json'
  ) {
    response.writeHead(415);
    response.end();
    return;
  }
  const chunks = [];
  let size = 0;
  request.on('data', (chunk) => {
    size += chunk.length;
    if (size > 1024 * 1024) {
      request.destroy();
      return;
    }
    chunks.push(chunk);
  });
  request.on('end', () => {
    let message;
    try {
      message = JSON.parse(Buffer.concat(chunks).toString('utf8'));
    } catch {
      response.writeHead(400);
      response.end();
      return;
    }
    const rpcId = typeof message?.rpcId === 'string' ? message.rpcId : 'invalid';
    if (message?.type !== 'client-request' || typeof message?.method !== 'string') {
      respond(response, rpcId, {
        ok: false,
        error: {
          code: 'gateway/bad-request',
          message: 'invalid client-request message',
          details: {},
        },
      });
      return;
    }
    const result =
      message.method === 'latest'
        ? { ok: true, value: readLatest() }
        : {
            ok: false,
            error: {
              code: 'not-found',
              message: `unknown Ossia selection endpoint ${message.method}`,
              details: {},
            },
          };
    respond(response, rpcId, result);
  });
  request.on('error', () => {
    if (!response.headersSent) response.writeHead(500);
    response.end();
  });
}

function respond(response, rpcId, result) {
  response.writeHead(200, { 'content-type': 'application/json' });
  response.end(JSON.stringify({ type: 'server-response', rpcId, result }));
}

function sleep(ms, signal) {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const timer = setTimeout(done, ms);
    signal.addEventListener('abort', done, { once: true });
    function done() {
      clearTimeout(timer);
      signal.removeEventListener('abort', done);
      resolve();
    }
  });
}

export const __test = { parseFrame };

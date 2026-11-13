import type { BrowserContext } from '@playwright/test';

import type {
  DocumentContext,
  DocumentState,
  DocumentSummary,
  Job,
  OutputArgs,
  RenderGeometry,
  RenderReference,
  RuntimeEvent,
  Selection,
  Version,
} from '../../src/generated/protocol.ts';

export const hash = 'a'.repeat(64),
  newer = 'b'.repeat(64);
export const firstId = '11111111-1111-4111-8111-111111111111';
export const secondId = '22222222-2222-4222-8222-222222222222';
export const versionId = '33333333-3333-4333-8333-333333333333';
export const olderVersionId = '44444444-4444-4444-8444-444444444444';
const reference: RenderReference = {
  renderKey: hash,
  manifest: {
    sourceHash: hash,
    engineBuildId: 'test',
    optionsHash: hash,
    pages: [1, 2].map((page) => ({ page, path: `${page}.svg`, bytes: 20, sha256: hash })),
    pdf: null,
    midi: null,
    diagnostics: [],
    formatErrors: [],
  },
};
export function scoreDocument(id = firstId): DocumentState {
  return {
    documentId: id,
    title: id === firstId ? '合成钢琴练习' : '另一份乐谱',
    sourcePath: '/synthetic/score.ly',
    headVersionId: versionId,
    sourceHash: hash,
    dirty: false,
    editState: 'idle',
    lastSuccessfulRender: reference.manifest,
    lastCheck: null,
  };
}
function versionList(documentId: string): Version[] {
  return [
    {
      versionId,
      documentId,
      seq: 2,
      parentVersionId: olderVersionId,
      sourceHash: hash,
      origin: 'edit',
      author: 'test',
      summary: '当前版本',
      createdAt: '2026-11-10T00:00:00Z',
    },
    {
      versionId: olderVersionId,
      documentId,
      seq: 1,
      parentVersionId: null,
      sourceHash: hash,
      origin: 'import',
      author: 'test',
      summary: '初始版本',
      createdAt: '2026-11-09T00:00:00Z',
    },
  ];
}
function geometry(): RenderGeometry {
  return {
    renderKey: hash,
    sourceHash: hash,
    pages: [1, 2].map((page) => ({
      page,
      viewBox: { x0: 0, y0: 0, x1: 100, y1: 140 },
      capability: 'measure',
      reason: null,
      regions: [0, 1].map((staffIndex) => ({
        locator: { staffIndex, measureStart: page, measureEnd: page, voice: null },
        systemIndex: 0,
        bounds: { x0: 10, x1: 80, y0: 20 + staffIndex * 30, y1: 30 + staffIndex * 30 },
      })),
    })),
  };
}
function doneJob(id: string, result: unknown): Job {
  return {
    jobId: id,
    documentId: null,
    kind: 'test',
    state: 'done',
    sourceHash: null,
    phase: 'finished',
    result: result as Job['result'],
    error: null,
  };
}

/** Install a synthetic document-scoped runtime with controllable selections and events. */
export async function mockScoreRuntime(
  context: BrowserContext,
  options: {
    languageEvent?: 'en-US';
    delays?: { context?: number; versionRender?: number };
  } = {},
) {
  let seq = 1;
  let loseEvents = false;
  let rejectSelection = false;
  const states = [scoreDocument(), scoreDocument(secondId)];
  const selections = new Map<string, Selection | null>();
  const points: { page: number; x: number; y: number }[] = [];
  const restores: unknown[] = [];
  const outputs: OutputArgs[] = [];
  const reveals: string[] = [];
  const events: RuntimeEvent[] = [];
  const emit = (type: string, documentId = firstId) => {
    events.push({ instanceId: 'instance', seq: ++seq, type, documentId, payload: {} });
  };
  const contextFor = (documentId: string): DocumentContext | null => {
    const document = states.find((state) => state.documentId === documentId);
    if (!document) return null;
    return {
      instanceId: 'instance',
      seq,
      document,
      selection: selections.get(documentId) ?? null,
      regions: [],
      render: document.lastSuccessfulRender
        ? { renderKey: hash, manifest: document.lastSuccessfulRender }
        : null,
      editActivity: null,
    };
  };
  await context.route('**/api/v1/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    const path = url.pathname.replace('/api/v1', '');
    const payload =
      req.postData() && req.headers()['content-type']?.includes('application/json')
        ? req.postDataJSON()
        : {};
    const reply = (body: unknown, status = 200) =>
      route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
    if (path === '/events') {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      if (loseEvents) return route.abort();
      const body =
        (options.languageEvent
          ? `event: runtime\ndata: ${JSON.stringify({
              instanceId: 'instance',
              seq,
              type: 'ui-language-changed',
              payload: { language: options.languageEvent },
            })}\n\n`
          : ': keep-alive\n\n') +
        events
          .filter((event) => event.seq > Number(url.searchParams.get('after')))
          .map((event) => `event: runtime\ndata: ${JSON.stringify(event)}\n\n`)
          .join('');
      return route.fulfill({ contentType: 'text/event-stream', body });
    }
    if (path === '/status')
      return reply({
        runtime: {
          runtimeVersion: '0.1.0',
          protocolVersion: 1,
          schemaVersion: 1,
          instanceId: 'instance',
          home: '/synthetic/library',
          webBaseUrl: 'http://localhost:3211',
          capabilities: ['document-context'],
        },
        engineBuildId: 'test',
        documentCount: states.length,
        instanceId: 'instance',
        seq,
        jobs: [],
        documentation: { available: true, version: '2.26.0', root: '/synthetic/docs', error: null },
        language: null,
      });
    if (path === '/outputs') {
      const args = payload as OutputArgs;
      outputs.push(args);
      return reply(
        doneJob(`output-${outputs.length}`, {
          source: args.source,
          documentId: firstId,
          sourceHash: hash,
          engineBuildId: 'test',
          format: args.format,
          page: args.page ?? null,
          path: `/synthetic/output.${args.format}`,
          bytes: 1024,
          sha256: hash,
          mediaType: 'application/octet-stream',
        }),
      );
    }
    if (path.startsWith('/jobs/') && path.endsWith('/output')) {
      return route.fulfill({
        contentType: 'application/octet-stream',
        body: `synthetic ${path.split('/')[2]}`,
      });
    }
    if (path.startsWith('/jobs/') && path.endsWith('/reveal')) {
      reveals.push(path.split('/')[2] ?? '');
      return reply({ opened: true });
    }
    if (path === '/renders/' + hash + '/geometry') return reply(geometry());
    if (path.includes('/pages/'))
      return route.fulfill({
        contentType: 'image/svg+xml',
        body: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140"><rect x="10" y="20" width="70" height="10" fill="#ddd"/><rect x="10" y="50" width="70" height="10" fill="#ddd"/></svg>',
      });
    if (path === '/documents' && req.method() === 'GET') {
      const items: DocumentSummary[] = states.map(
        ({ documentId, title, headVersionId, sourceHash, dirty, editState }) => ({
          documentId,
          title,
          headVersionId,
          sourceHash,
          dirty,
          editState,
        }),
      );
      return reply({ items, total: items.length, offset: 0, nextOffset: null });
    }
    const parts = path.split('/').filter(Boolean);
    const documentId = parts[0] === 'documents' ? (parts[1] ?? '') : '';
    if (!contextFor(documentId)) return reply({ error: { code: 'not-found', message: path } }, 404);
    if (parts[2] === 'context') {
      if (options.delays?.context)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.context));
      return reply(contextFor(documentId)!);
    }
    if (parts[2] === 'versions' && parts[4] === 'render') {
      if (options.delays?.versionRender)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.versionRender));
      return reply(doneJob('version-render', reference));
    }
    if (parts[2] === 'versions')
      return reply({
        items: versionList(documentId),
        total: 2,
        offset: 0,
        nextOffset: null,
      });
    if (parts[2] === 'locate') {
      points.push(payload);
      const staffIndex = payload.y > 40 ? 1 : 0;
      if (payload.x < 10 || payload.x > 80 || payload.y < 20 || payload.y > 60)
        return reply({ capability: 'none', reason: 'no-hit', region: null });
      return reply({
        capability: 'measure',
        reason: null,
        region: {
          locator: {
            staffIndex,
            measureStart: payload.page,
            measureEnd: payload.page,
            voice: null,
          },
          capability: 'measure',
          reason: null,
          spans: [],
        },
      });
    }
    if (parts[2] === 'selection') {
      if (rejectSelection)
        return reply({ error: { code: 'stale-selection', message: 'source changed' } }, 409);
      selections.set(documentId, payload.locators.length ? payload : null);
      emit('selection-changed', documentId);
      return reply(contextFor(documentId));
    }
    if (parts[2] === 'restore') {
      restores.push(payload);
      seq++;
      return reply(doneJob('restore', null));
    }
    return reply({ error: { code: 'not-found', message: path } }, 404);
  });
  return {
    points,
    restores,
    outputs,
    reveals,
    selections,
    states,
    emit,
    contextFor,
    disconnect: (value: boolean) => {
      loseEvents = value;
    },
    stale: () => {
      rejectSelection = true;
    },
    change: () => {
      states[0]!.sourceHash = newer;
      states[0]!.dirty = true;
      selections.clear();
      emit('source-changed');
    },
  };
}

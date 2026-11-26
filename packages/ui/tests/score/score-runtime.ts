import { createServer, type ServerResponse } from 'node:http';

import type { BrowserContext } from '@playwright/test';

import type {
  DocumentContext,
  DocumentState,
  DocumentSummary,
  Drawing,
  GeometryElement,
  Job,
  OutputArgs,
  RenderGeometry,
  RenderReference,
  ScorePlayback,
  ScoreSettings,
  RuntimeEvent,
  Selection,
  Version,
  UiLanguage,
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
function versionList(documentId: string, count = 2): Version[] {
  const versions: Version[] = [
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
  for (let seq = 3; seq <= count; seq++) {
    const hex = seq.toString(16).padStart(8, '0');
    versions.unshift({
      versionId: `${hex}-aaaa-4aaa-8aaa-${hex.padStart(12, '0')}`,
      documentId,
      seq,
      parentVersionId: versions[0]!.versionId,
      sourceHash: hash,
      origin: 'edit',
      author: 'test',
      summary: `版本 ${seq}`,
      createdAt: '2026-11-10T00:00:00Z',
    });
  }
  return versions;
}
function geometry(
  elements: GeometryElement[] = [],
  elementsByPage?: Record<number, GeometryElement[]>,
  pageCount = 2,
): RenderGeometry {
  return {
    renderKey: hash,
    sourceHash: hash,
    pages: Array.from({ length: pageCount }, (_, index) => index + 1).map((page) => ({
      page,
      viewBox: { x0: 0, y0: 0, x1: 100, y1: 140 },
      capability: 'measure',
      reason: null,
      elements: elementsByPage?.[page] ?? elements,
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
    language?: UiLanguage;
    title?: string;
    elements?: GeometryElement[];
    elementsByPage?: Record<number, GeometryElement[]>;
    svg?: string;
    playback?: ScorePlayback;
    pageCount?: number;
    versionCount?: number;
    eventsDelay?: number;
    webBaseUrl?: string;
    delays?: { status?: number; context?: number; documents?: number; versionRender?: number };
  } = {},
) {
  let language = options.language ?? null;
  let rejectLanguage = false;
  let seq = 1;
  let loseEvents = false;
  let rejectSelection = false;
  const reference: RenderReference = {
    renderKey: hash,
    manifest: {
      ...scoreDocument().lastSuccessfulRender!,
      pages: Array.from({ length: options.pageCount ?? 2 }, (_, index) => ({
        page: index + 1,
        path: `${index + 1}.svg`,
        bytes: 20,
        sha256: hash,
      })),
    },
  };
  const states: DocumentState[] = [scoreDocument(), scoreDocument(secondId)].map((document) => ({
    ...document,
    title: document.documentId === firstId ? (options.title ?? document.title) : document.title,
    lastSuccessfulRender: reference.manifest,
  }));
  const selections = new Map<string, Selection | null>();
  const drawings = new Map<string, Drawing>();
  const scoreSettings = new Map<string, ScoreSettings>();
  const points: { page: number; x: number; y: number }[] = [];
  const restores: unknown[] = [];
  const outputs: OutputArgs[] = [];
  const reveals: string[] = [];
  const versionRenders: string[] = [];
  const geometryRequests: string[] = [];
  const pageRequests: string[] = [];
  const playbackRequests: string[] = [];
  const events: RuntimeEvent[] = [];
  let eventsPaused = false;
  const pendingEvents: RuntimeEvent[] = [];
  const activeJobs: Job[] = [];
  const streams = new Set<ServerResponse>();
  const eventFrame = (event: RuntimeEvent) => `event: runtime\ndata: ${JSON.stringify(event)}\n\n`;
  const emit = (type: string, documentId = firstId, payload: RuntimeEvent['payload'] = {}) => {
    const event = { instanceId: 'instance', seq: ++seq, type, documentId, payload };
    events.push(event);
    if (eventsPaused) pendingEvents.push(event);
    else for (const stream of streams) stream.write(eventFrame(event));
  };
  const eventServer = createServer((request, response) => {
    if (loseEvents) {
      request.socket.destroy();
      return;
    }
    response.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      'access-control-allow-origin': '*',
    });
    response.write(': keep-alive\n\n');
    if (options.languageEvent) {
      response.write(
        eventFrame({
          instanceId: 'instance',
          seq,
          type: 'ui-language-changed',
          payload: { language: options.languageEvent },
        }),
      );
    }
    const after = Number(new URL(request.url!, 'http://localhost').searchParams.get('after'));
    for (const event of events.filter((entry) => entry.seq > after))
      response.write(eventFrame(event));
    streams.add(response);
    response.on('close', () => streams.delete(response));
  });
  await new Promise<void>((resolve) => eventServer.listen(0, '127.0.0.1', resolve));
  const address = eventServer.address();
  if (!address || typeof address === 'string') throw new Error('event server did not bind a port');
  context.on('close', () => {
    eventServer.closeAllConnections();
    eventServer.close();
  });
  const contextFor = (documentId: string): DocumentContext | null => {
    const document = states.find((state) => state.documentId === documentId);
    if (!document) return null;
    return {
      instanceId: 'instance',
      seq,
      document,
      selection: selections.get(documentId) ?? null,
      regions: [],
      annotationRegions: [],
      render: document.lastSuccessfulRender
        ? {
            renderKey: document.lastSuccessfulRender.sourceHash,
            manifest: document.lastSuccessfulRender,
          }
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
      if (options.eventsDelay === undefined) {
        return route.continue({
          url: `http://127.0.0.1:${address.port}${url.pathname}${url.search}`,
        });
      }
      await new Promise((resolve) => setTimeout(resolve, options.eventsDelay ?? 1500));
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
    if (path === '/ui-language' && req.method() === 'PUT') {
      if (rejectLanguage)
        return reply({ error: { code: 'storage-error', message: 'save failed' } }, 500);
      language = payload.language;
      emit('ui-language-changed', firstId, { language });
      return reply(language);
    }
    if (path === '/status') {
      const status = {
        runtime: {
          runtimeVersion: '0.1.0',
          protocolVersion: 8,
          schemaVersion: 3,
          instanceId: 'instance',
          home: '/synthetic/library',
          webBaseUrl: options.webBaseUrl ?? 'http://localhost:3211',
          capabilities: ['document-context'],
        },
        engineBuildId: 'test',
        documentCount: states.length,
        instanceId: 'instance',
        seq,
        jobs: activeJobs,
        documentation: { available: true, version: '2.26.0', root: '/synthetic/docs', error: null },
        language,
      };
      if (options.delays?.status)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.status));
      return reply(status);
    }
    if (path === '/outputs') {
      const args = payload as OutputArgs;
      outputs.push(args);
      if ((args.pages?.length ?? 0) > 1)
        return reply(
          doneJob(`output-${outputs.length}`, {
            source: args.source,
            documentId: firstId,
            sourceHash: hash,
            engineBuildId: 'test',
            format: args.format,
            directory: '/synthetic',
            files: args.pages!.map((page) => ({
              page,
              bounds: { x0: 0, y0: 0, x1: 100, y1: 140 },
              path: `/synthetic/${page}`,
              bytes: 10,
              sha256: hash,
              mediaType: 'image/svg+xml',
            })),
          }),
        );
      return reply(
        doneJob(`output-${outputs.length}`, {
          source: args.source,
          documentId: firstId,
          sourceHash: hash,
          engineBuildId: 'test',
          format: args.format,
          page: args.pages?.[0] ?? null,
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
    if (/^\/jobs\/[^/]+\/outputs\/\d+$/.test(path))
      return route.fulfill({ contentType: 'application/octet-stream', body: `synthetic ${path}` });
    if (path.startsWith('/jobs/') && path.endsWith('/reveal')) {
      reveals.push(path.split('/')[2] ?? '');
      return reply({ opened: true });
    }
    if (path === '/renders/' + hash + '/geometry') {
      geometryRequests.push(path);
      return reply(geometry(options.elements, options.elementsByPage, options.pageCount));
    }
    if (path.startsWith('/renders/') && path.endsWith('/playback')) {
      playbackRequests.push(path);
      return reply(
        options.playback ??
          ({
            renderKey: path.split('/')[2]!,
            sourceHash: path.split('/')[2]!,
            durationSeconds: 2,
            parts: [{ id: 'part', name: 'Piano', instrument: 'piano' }],
            notes: [
              {
                partId: 'part',
                instrument: 'piano' as const,
                pitch: 60,
                startSeconds: 0,
                durationSeconds: 0.8,
              },
            ],
            groups: options.elements?.length
              ? [
                  {
                    partId: 'part',
                    elementIds: options.elements
                      .filter((element) => element.region.elementKind === 'NoteHead')
                      .flatMap((element) =>
                        element.region.target.kind === 'element'
                          ? [element.region.target.elementId]
                          : [],
                      ),
                    pitches: [60],
                    staffIndex: options.elements?.[0]?.region.staffIndex ?? 0,
                    voice: null,
                    startSeconds: 0,
                    durationSeconds: 0.8,
                  },
                ].filter((group) => group.elementIds.length)
              : [],
            systems: [
              {
                page: 1,
                systemIndex: 0,
                measureStart: 1,
                measureEnd: 1,
                startSeconds: 0,
                endSeconds: 2,
                x0: 10,
                x1: 80,
                y0: 18,
                y1: 32,
              },
            ],
          } satisfies ScorePlayback),
      );
    }
    if (path.includes('/pages/')) {
      pageRequests.push(path);
      return route.fulfill({
        contentType: 'image/svg+xml',
        body:
          options.svg ??
          `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140"><rect x="10" y="20" width="70" height="10" fill="#ddd"/><rect x="10" y="50" width="70" height="10" fill="#ddd"/>${(options.elements ?? []).flatMap((e) => e.graphicIds.map((id) => `<g data-ossia-graphic="${id}"><path d="M${e.bounds.x0} ${e.bounds.y0}h2v2h-2z" fill="black"/></g>`)).join('')}</svg>`,
      });
    }
    if (path === '/documents' && req.method() === 'GET') {
      if (options.delays?.documents)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.documents));
      const items: DocumentSummary[] = states.map(
        ({ documentId, title, headVersionId, sourceHash, dirty, editState }) => ({
          documentId,
          title,
          updatedAt:
            documentId === secondId ? '2026-11-12T00:00:00.000Z' : '2026-11-10T00:00:00.000Z',
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
    if (parts[2] === 'settings') {
      const current = scoreSettings.get(documentId) ?? { revision: 0, instruments: {} };
      if (req.method() === 'GET') return reply(current);
      if (payload.revision !== current.revision)
        return reply({ error: { code: 'conflict', message: 'settings changed' } }, 409);
      const next = { revision: current.revision + 1, instruments: payload.instruments };
      scoreSettings.set(documentId, next);
      emit('score-settings-changed', documentId, { revision: next.revision });
      return reply(next);
    }
    if (parts[2] === 'drawing') {
      const current = drawings.get(documentId) ?? { revision: 0, strokes: [] };
      if (req.method() === 'GET') return reply(current);
      if (payload.revision !== current.revision)
        return reply({ error: { code: 'conflict', message: 'drawing changed' } }, 409);
      const next = { revision: current.revision + 1, strokes: payload.strokes };
      drawings.set(documentId, next);
      emit('drawing-changed', documentId, { revision: next.revision });
      return reply(next);
    }
    if (parts[2] === 'context') {
      if (options.delays?.context)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.context));
      return reply(contextFor(documentId)!);
    }
    if (parts[2] === 'versions' && parts[4] === 'render') {
      versionRenders.push(parts[3] ?? '');
      if (options.delays?.versionRender)
        await new Promise((resolve) => setTimeout(resolve, options.delays?.versionRender));
      return reply(doneJob('version-render', reference));
    }
    if (parts[2] === 'versions') {
      const items = versionList(documentId, options.versionCount);
      return reply({ items, total: items.length, offset: 0, nextOffset: null });
    }
    if (parts[2] === 'locate') {
      points.push(payload);
      const staffIndex = payload.y > 40 ? 1 : 0;
      if (payload.x < 10 || payload.x > 80 || payload.y < 20 || payload.y > 60)
        return reply({ capability: 'none', reason: 'no-hit', region: null });
      return reply({
        capability: 'measure',
        reason: null,
        region: {
          target: {
            kind: 'measure',
            locator: {
              staffIndex,
              measureStart: payload.page,
              measureEnd: payload.page,
              voice: null,
            },
          },
          occurrences: [],
          label: 'Measure',
          elementKind: null,
          insertionAnchor: null,
          staffIndex,
          voice: null,
          capability: 'measure',
          reason: null,
          spans: [],
        },
      });
    }
    if (parts[2] === 'selection') {
      if (rejectSelection)
        return reply({ error: { code: 'stale-selection', message: 'source changed' } }, 409);
      selections.set(
        documentId,
        payload.targets.length || (payload.annotations ?? []).length ? payload : null,
      );
      emit('selection-changed', documentId, {
        clientId: req.headers()['x-ossia-client-id'] ?? null,
        targetCount: payload.targets.length,
        annotationCount: (payload.annotations ?? []).length,
      });
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
    versionRenders,
    geometryRequests,
    pageRequests,
    playbackRequests,
    setJob: (job: Job | null) => {
      activeJobs.splice(0, activeJobs.length, ...(job ? [job] : []));
    },
    selections,
    drawings,
    scoreSettings,
    rejectLanguage: (reject: boolean) => {
      rejectLanguage = reject;
    },
    states,
    emit,
    pauseEvents: () => {
      eventsPaused = true;
    },
    resumeEvents: () => {
      eventsPaused = false;
      for (const event of pendingEvents.splice(0)) {
        for (const stream of streams) stream.write(eventFrame(event));
      }
    },
    contextFor,
    disconnect: (value: boolean) => {
      loseEvents = value;
      if (value) for (const stream of streams) stream.destroy();
    },
    stale: () => {
      rejectSelection = true;
    },
    change: () => {
      states[0] = { ...states[0]!, sourceHash: newer, dirty: true };
      selections.clear();
      emit('source-changed');
    },
    trash: (documentId = firstId) => {
      const index = states.findIndex((state) => state.documentId === documentId);
      if (index >= 0) states.splice(index, 1);
      emit('document-deleted', documentId);
    },
    untrash: (documentId = firstId) => {
      if (!states.some((state) => state.documentId === documentId)) {
        states.push(scoreDocument(documentId));
      }
      emit('document-restored', documentId);
    },
  };
}

import type { BrowserContext } from '@playwright/test';

import type {
  DocumentState,
  RenderGeometry,
  RenderReference,
  ViewSnapshot,
} from '../../src/generated/protocol.ts';

export const hash = 'a'.repeat(64),
  newer = 'b'.repeat(64);
const documentId = '11111111-1111-4111-8111-111111111111';
export const secondId = '22222222-2222-4222-8222-222222222222';
const versionId = '33333333-3333-4333-8333-333333333333';
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
export function scoreDocument(id = documentId): DocumentState {
  return {
    documentId: id,
    title: id === documentId ? '合成钢琴练习' : '另一份乐谱',
    sourcePath: '/synthetic/score.ly',
    headVersionId: versionId,
    sourceHash: hash,
    dirty: false,
    editState: 'idle',
    lastSuccessfulRender: reference.manifest,
    lastCheck: null,
  };
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
/** Install a synthetic runtime with controllable documents, views and events. */
export async function mockScoreRuntime(
  context: BrowserContext,
  options: { languageEvent?: 'en-US' } = {},
) {
  let seq = 1;
  let exchanges = 0;
  let expired = false;
  let loseEvents = false;
  const views = new Map<string, ViewSnapshot>();
  const docs = [scoreDocument(), scoreDocument(secondId)];
  const points: { page: number; x: number; y: number }[] = [];
  const mutations: { expectedSourceHash: string }[] = [];
  let rejectSelection = false;
  let conflict = false;
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
    if (path === '/sessions/exchange') {
      exchanges++;
      const id = payload.ticket;
      const view: ViewSnapshot = {
        viewId: id,
        instanceId: 'instance',
        seq,
        document: docs[0]!,
        selection: null,
        regions: [],
        render: reference,
        editActivity: null,
      };
      views.set(id, view);
      return reply({ token: id, view });
    }
    if (expired) return reply({ error: { code: 'unauthorized', message: 'expired' } }, 401);
    const id = req.headers().authorization?.replace('Bearer ', '') ?? '';
    const view = views.get(id);
    if (!view) return reply({ error: { code: 'unauthorized', message: 'no session' } }, 401);
    if (path === '/events') {
      await new Promise((resolve) => setTimeout(resolve, 1500));
      if (loseEvents) return route.abort();
      const body = options.languageEvent
        ? `event: runtime\ndata: ${JSON.stringify({
            instanceId: 'instance',
            seq,
            type: 'ui-language-changed',
            payload: { language: options.languageEvent },
          })}\n\n`
        : ': keep-alive\n\n';
      return route.fulfill({ contentType: 'text/event-stream', body });
    }
    if (path === '/documents') return reply(docs);
    if (path === '/status')
      return reply({
        runtime: { instanceId: 'instance' },
        engineBuildId: 'test',
        instanceId: 'instance',
        seq,
        jobs: [],
      });
    if (path.endsWith('/versions'))
      return reply([
        {
          versionId,
          documentId: view.document?.documentId,
          seq: 1,
          parentVersionId: null,
          sourceHash: hash,
          origin: 'import',
          author: 'test',
          summary: '初始版本',
          createdAt: '2026-11-10T00:00:00Z',
        },
      ]);
    if (path === `/views/${id}`) return reply({ ...view, seq });
    if (path.endsWith('/document')) {
      view.document = docs.find((d) => d.documentId === payload.documentId)!;
      view.selection = null;
      seq++;
      return reply({ ...view, seq });
    }
    if (path.endsWith('/geometry')) return reply(geometry());
    if (path.includes('/pages/'))
      return route.fulfill({
        contentType: 'image/svg+xml',
        body: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 140"><rect x="10" y="20" width="70" height="10" fill="#ddd"/><rect x="10" y="50" width="70" height="10" fill="#ddd"/></svg>',
      });
    if (path.endsWith('/locate')) {
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
    if (path.endsWith('/selection')) {
      if (rejectSelection)
        return reply({ error: { code: 'stale-selection', message: 'source changed' } }, 409);
      view.selection = payload.locators.length ? payload : null;
      seq++;
      return reply({ ...view, seq });
    }
    if (path.endsWith('/edits') || path.endsWith('/restore')) {
      mutations.push(payload);
      return reply(
        {
          error: {
            code: conflict ? 'edit-busy' : 'dirty-workspace',
            message: conflict ? '另一任务正在编辑' : '工作区存在未提交修改',
          },
        },
        409,
      );
    }
    return reply({ error: { code: 'not-found', message: path } }, 404);
  });
  return {
    points,
    mutations,
    views,
    docs,
    exchanges: () => exchanges,
    expire: () => {
      expired = true;
    },
    disconnect: (value: boolean) => {
      loseEvents = value;
    },
    stale: () => {
      rejectSelection = true;
    },
    conflict: () => {
      conflict = true;
    },
    change: () => {
      docs[0]!.sourceHash = newer;
      docs[0]!.dirty = true;
      docs[0]!.lastCheck = {
        jobId: 'check',
        documentId,
        kind: 'check',
        state: 'error',
        sourceHash: newer,
        phase: 'finished',
        result: {
          diagnostics: [{ message: 'synthetic syntax error', position: { line: 4, column: 2 } }],
        },
        error: { code: 'compile-failed', message: '排版失败' },
      };
      for (const v of views.values()) v.selection = null;
      seq++;
    },
  };
}

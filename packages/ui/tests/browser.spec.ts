import { expect, test } from '@playwright/test';
import type { BrowserContext } from '@playwright/test';

import type {
  AgentIntegration,
  AgentsSnapshot,
  DocumentState,
  ManagementPreferences,
  ManagementSnapshot,
  RenderGeometry,
  RenderReference,
  ViewSnapshot,
} from '../src/generated/protocol.ts';

const hash = 'a'.repeat(64),
  newer = 'b'.repeat(64);
const documentId = '11111111-1111-4111-8111-111111111111';
const secondId = '22222222-2222-4222-8222-222222222222';
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
function document(id = documentId): DocumentState {
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
async function mock(context: BrowserContext, options: { languageEvent?: 'en-US' } = {}) {
  let seq = 1;
  let exchanges = 0;
  let expired = false;
  let loseEvents = false;
  const views = new Map<string, ViewSnapshot>();
  const docs = [document(), document(secondId)];
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

test('live language events update an open score and browser color scheme is independent', async ({
  context,
  page,
}) => {
  await mock(context, { languageEvent: 'en-US' });
  await page.goto('/score.html#ticket=language-view');
  await expect(page.getByText('Local service connected')).toBeVisible();
  await expect(page.getByLabel('Current score')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Workspace score' })).toBeVisible();
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test('ticket is single-use, refresh and independent views preserve selection', async ({
  context,
  page,
}) => {
  const h = await mock(context);
  await page.goto('/score.html#ticket=view-one');
  await expect(page.getByText('本地服务已连接')).toBeVisible();
  expect(page.url()).not.toContain('ticket=');
  expect(h.exchanges()).toBe(1);
  await page.getByText('使用键盘选择小节').click();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 1 小节');
  const other = await context.newPage();
  await other.goto('/score.html#ticket=view-two');
  await other.getByLabel('当前乐谱').selectOption(secondId);
  await expect(other.locator('.document-status h2')).toHaveText('另一份乐谱');
  await expect(page.locator('.document-status h2')).toHaveText('合成钢琴练习');
  await page.reload();
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 1 小节');
  expect(h.exchanges()).toBe(2);
  await page.getByRole('button', { name: '清空选区' }).click();
  await expect(page.locator('.selection')).toContainText('尚未选择小节');
});
test('SVG transforms, staff boxes, pages, zoom and keyboard work at narrow and wide widths', async ({
  context,
  page,
}) => {
  const h = await mock(context);
  await page.goto('/score.html#ticket=geometry');
  await expect(page.locator('.score-image')).toBeVisible();
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 1000 });
    await page.getByLabel('谱面缩放').selectOption('150');
    const box = await page.locator('.overlay').boundingBox();
    expect(box).not.toBeNull();
    await page
      .locator('.overlay')
      .click({ position: { x: box!.width * 0.4, y: (box!.height * 55) / 140 } });
    await expect(page.locator('.selection li')).toHaveText('谱表 2 · 第 1 小节');
    const rect = page.locator('.selection-box');
    await expect(rect).toHaveAttribute('y', '50');
    await expect(rect).toHaveAttribute('height', '10');
    expect(h.points.at(-1)!.y).toBeCloseTo(55, 0);
    await page.getByRole('button', { name: '清空选区' }).click();
  }
  await page.getByLabel('谱面页码').selectOption('2');
  await page.getByText('使用键盘选择小节').click();
  const button = page.getByRole('button', { name: '谱表 1 · 第 2 小节', exact: true });
  await button.focus();
  await page.keyboard.press('Space');
  await expect(page.locator('.selection li')).toHaveText('谱表 1 · 第 2 小节');
  await page.keyboard.press('Enter');
  await expect(page.locator('.selection li')).toHaveCount(0);
});
test('failed compilation retains old pages, clears selection, reports positions and busy conflict', async ({
  context,
  page,
}) => {
  const h = await mock(context);
  await page.goto('/score.html#ticket=failure');
  await page.getByText('使用键盘选择小节').click();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  h.change();
  await expect(page.getByText('上次成功结果：', { exact: false })).toBeVisible();
  await expect(page.locator('.selection li')).toHaveCount(0);
  await expect(page.locator('.score-image')).toBeVisible();
  await expect(page.locator('.diagnostics')).toContainText('第 4 行，第 2 列');
  await expect(
    page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }),
  ).toBeDisabled();
  await page.getByLabel('谱面页码').selectOption('2');
  await expect(page.locator('.score-image')).toHaveAttribute('alt', /第 2 页/);
  h.conflict();
  await page.getByRole('button', { name: '丢弃工作区修改' }).click();
  await page.getByRole('button', { name: '确认丢弃' }).click();
  await expect(page.locator('.message.error')).toContainText('另一任务正在编辑');
  await expect(page.locator('.document-status')).toContainText('工作区有未提交修改');
});
test('stale selection is reported; reconnect preserves state and a restarted runtime needs a new link', async ({
  context,
  page,
}) => {
  const h = await mock(context);
  await page.goto('/score.html#ticket=reconnect');
  await page.getByText('使用键盘选择小节').click();
  h.stale();
  await page.getByRole('button', { name: '谱表 1 · 第 1 小节', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('source changed');
  h.disconnect(true);
  await expect(page.getByText('连接中断，正在重连…')).toBeVisible();
  h.disconnect(false);
  await expect(page.getByText('本地服务已连接')).toBeVisible();
  h.expire();
  await expect(page.getByRole('heading', { name: '视图已失效' })).toBeVisible();
});
test('no ticket explains how to open; browser management cannot invoke native commands', async ({
  page,
}) => {
  await page.goto('/score.html');
  await expect(page.getByRole('alert')).toContainText('ossia_open');
  await page.goto('/main.html');
  await expect(page.getByRole('alert')).toContainText('桌面应用');
  await expect(page.getByRole('button', { name: '启动服务' })).toBeDisabled();
  await expect(page.getByRole('button', { name: '本地服务' })).toHaveAttribute(
    'aria-current',
    'page',
  );
  await expect(page.locator('[data-tauri-drag-region="deep"]')).toHaveCount(2);
  await page.getByRole('button', { name: 'Agent 接入' }).click();
  await expect(page.getByText('把 Ossia 服务接入到支持的 Agent 中。')).toBeVisible();
  await expect(page.getByRole('button', { name: '全部' })).toHaveCount(0);
  await page.getByRole('button', { name: '常规' }).click();
  await expect(page.getByRole('heading', { name: '常规' })).toBeVisible();
  await expect(page.getByLabel('语言')).toBeDisabled();
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(
    await page.evaluate(() =>
      getComputedStyle(globalThis.document.documentElement).getPropertyValue('--page').trim(),
    ),
  ).toBe('#1c1c20');
});

test.describe('English management shell', () => {
  test.use({ locale: 'en-US' });

  test('uses the selected system language without native settings', async ({ page }) => {
    await page.goto('/main.html');
    await expect(page.getByText('Local service connected')).toHaveCount(0);
    await expect(page.getByRole('button', { name: 'Local service' })).toBeVisible();
    await expect(page.getByRole('alert')).toContainText('Open the management window');
    await page.getByRole('button', { name: 'General' }).click();
    await expect(page.getByRole('heading', { name: 'General' })).toBeVisible();
    await expect(page.getByLabel('Language')).toHaveValue('en-US');
  });
});

test('management shows details and target config paths only for connected agents', async ({
  page,
}) => {
  const preferences: ManagementPreferences = {
    theme: 'system',
    language: 'zh-CN',
    trayClickAction: 'window',
    trayClickActionEditable: true,
    launchAtLogin: false,
    launchAtLoginEditable: true,
    appVersion: '0.1.0',
  };
  const status: ManagementSnapshot = {
    home: '/tmp/ossia',
    engineRoot: '/tmp/lilypond',
    runtime: null,
    error: null,
  };
  const connected: AgentIntegration = {
    id: 'codex',
    name: 'Codex',
    icon: 'codex',
    method: 'plugin',
    state: 'installed',
    installed: true,
    enabled: true,
    managed: true,
    version: '0.167.0',
    executable: '/Applications/ChatGPT.app/Contents/MacOS/codex',
    configRoot: '/Users/example/.codex',
    configPath: '/Users/example/.codex/config.toml',
    profile: 'default',
    path: '/project/.ossia/integrations/hosts/codex/marketplace/plugins/ossia',
    reloadHint: null,
    message: null,
    conflicts: [],
  };
  const available: AgentIntegration = {
    ...connected,
    id: 'dsh',
    name: 'DeepSeek Harness',
    icon: 'dsh',
    state: 'available',
    installed: false,
    enabled: false,
    configRoot: '/Users/example/.dsh',
    configPath: '/Users/example/.dsh/profiles/desktop/package.json',
  };
  const agents: AgentsSnapshot = {
    agents: [connected, available],
    syncing: false,
    error: null,
  };
  const payload = JSON.stringify({ preferences, status, agents });
  await page.addInitScript((raw) => {
    const { preferences, status, agents } = JSON.parse(raw) as {
      preferences: object;
      status: object;
      agents: object;
    };
    let callbackId = 0;
    const internals = {
      transformCallback: () => ++callbackId,
      invoke: async (command: string) => {
        if (command === 'management_preferences') return preferences;
        if (command === 'management_status') return status;
        if (command === 'management_agents_state') return agents;
        if (command === 'plugin:event|listen') return 1;
        if (command === 'plugin:event|unlisten') return null;
        throw new Error(`unexpected Tauri command: ${command}`);
      },
    };
    const ownWindow = window as Window & {
      __TAURI_INTERNALS__?: typeof internals;
      __TAURI_EVENT_PLUGIN_INTERNALS__?: {
        unregisterListener: () => void;
      };
    };
    ownWindow.__TAURI_INTERNALS__ = internals;
    ownWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {},
    };
  }, payload);
  await page.goto('/main.html');
  await page.getByRole('button', { name: 'Agent 接入' }).click();
  await expect(page.getByRole('button', { name: '全部' })).toHaveCount(0);
  await expect(page.locator('.agent-detail')).toHaveCount(1);
  await expect(page.getByText('Agent 版本')).toBeVisible();
  await expect(page.getByText('目标配置路径')).toBeVisible();
  await expect(page.getByText('/Users/example/.codex/config.toml')).toBeVisible();
  await expect(page.getByText('/Users/example/.dsh/profiles/desktop/package.json')).toHaveCount(0);
});

test('discard confirmation keeps the original hash when source changes while the dialog is open', async ({
  context,
  page,
}) => {
  const h = await mock(context);
  h.docs[0]!.dirty = true;
  await page.goto('/score.html#ticket=confirm-hash');
  await page.getByRole('button', { name: '丢弃工作区修改' }).click();
  h.change();
  await expect(page.locator('.document-status')).toContainText(newer.slice(0, 12));
  await page.getByRole('button', { name: '确认丢弃' }).click();
  await expect.poll(() => h.mutations.length).toBe(1);
  expect(h.mutations[0]!.expectedSourceHash).toBe(hash);
});

test('import presents precise losses, carries confirmation and never bypasses refusal', async ({
  context,
  page,
}) => {
  await mock(context);
  const confirmation = 'c'.repeat(64);
  let confirmed = false;
  let refused = false;
  await page.route('**/api/v1/documents', async (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postData() ?? '';
    confirmed = body.includes(confirmation);
    const report = {
      status: refused ? 'refused' : confirmed ? 'clean' : 'needs-confirmation',
      confirmation,
      inputHash: hash,
      engineBuildId: 'synthetic',
      source: null,
      sourceHash: null,
      confirmed,
      measures: 2,
      voices: 1,
      noteEvents: 2,
      losses: ['合成损失：歌词未保留'],
      blocking: refused ? ['合成阻断：Negative skip'] : [],
      diagnostics: [],
    };
    await route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({
        jobId: 'import-job',
        documentId: null,
        kind: 'import-request',
        state: 'done',
        phase: 'finished',
        sourceHash: null,
        error: null,
        result: {
          report,
          document: !refused && confirmed ? document() : null,
          render: null,
          job: null,
        },
      }),
    });
  });
  await page.goto('/score.html#ticket=import');
  expect(await page.evaluate(() => '__TAURI_INTERNALS__' in window)).toBe(false);
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'synthetic.musicxml',
    mimeType: 'text/xml',
    buffer: Buffer.from('<score-partwise/>'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.getByText('合成损失：歌词未保留')).toBeVisible();
  expect(confirmed).toBe(false);
  await page.getByRole('button', { name: '知悉损失，确认导入' }).click();
  await expect.poll(() => confirmed).toBe(true);
  await expect(page.locator('.import-report')).toHaveCount(0);
  refused = true;
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'refused.musicxml',
    mimeType: 'text/xml',
    buffer: Buffer.from('<score-partwise/>'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.getByText('合成阻断：Negative skip')).toBeVisible();
  await expect(page.getByRole('button', { name: '知悉损失，确认导入' })).toHaveCount(0);
  await page.getByLabel('单文件 LilyPond / MusicXML / MIDI').setInputFiles({
    name: 'unsupported.mxl',
    mimeType: 'application/zip',
    buffer: Buffer.from('compressed'),
  });
  await page.getByRole('button', { name: '导入', exact: true }).click();
  await expect(page.locator('.import')).toContainText('暂不支持 .mxl');
});

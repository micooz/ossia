import { expect, type Page } from '@playwright/test';

import type {
  AgentIntegration,
  AgentsChange,
  AgentsSnapshot,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementScorePreview,
  ManagementScoreSummary,
  ManagementSnapshot,
  OmrSettingsChange,
  StorageCleanupReport,
  StorageStatus,
  UpdateSnapshot,
} from '../../src/generated/protocol.ts';

/** Install the management window bridge with isolated preferences and Agent state. */
export async function mockManagement(
  page: Page,
  {
    empty = false,
    initializing = false,
    dshState = 'available',
    workbuddies = [],
    claude = false,
    pi = false,
    initialRuntime = false,
    scores = [],
    archivedScores = [],
    previewStates = {},
    updateState,
    releaseNotesState,
    githubTokenMasked = null,
    omrService = 'ossia',
    omrUrl = null,
    omrTokenMasked = null,
    omrCustomTokenMasked = null,
    agentPaths = {},
  }: {
    claude?: boolean | 'desktop' | 'code';
    pi?: boolean;
    empty?: boolean;
    initializing?: boolean;
    dshState?: AgentIntegration['state'];
    workbuddies?: Array<{
      id?: string;
      state?: AgentIntegration['state'];
      installed?: boolean;
      enabled?: boolean;
      configRoot?: string;
      path?: string;
      version?: string;
    }>;
    initialRuntime?: boolean;
    scores?: ManagementScoreSummary[];
    archivedScores?: ManagementScoreSummary[];
    previewStates?: Record<string, ManagementScorePreview>;
    updateState?: UpdateSnapshot;
    releaseNotesState?: UpdateSnapshot;
    githubTokenMasked?: string | null;
    omrService?: 'ossia' | 'custom';
    omrUrl?: string | null;
    omrTokenMasked?: string | null;
    omrCustomTokenMasked?: string | null;
    agentPaths?: { codex?: string; dsh?: string };
  } = {},
) {
  const preferences: ManagementPreferences = {
    theme: 'system',
    language: 'zh-CN',
    trayClickAction: 'window',
    startupAction: 'window',
    trayClickActionEditable: true,
    storageCacheLimit: 'gib1',
    storageAutoCleanup: true,
    lanAccess: false,
    autoDownload: true,
    updateChannel: 'stable',
    updateChannelPreference: 'auto',
    updateSource: 'ossia',
    githubTokenMasked,
    omrService,
    omrOssiaUrl: 'https://ossia.maisow.com/omr',
    omrUrl,
    omrTokenMasked,
    omrCustomTokenMasked,
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
  if (initialRuntime) {
    status.runtime = {
      runtimeVersion: '0.1.0',
      protocolVersion: 10,
      schemaVersion: 2,
      instanceId: 'test',
      home: status.home,
      webBaseUrl: preferences.lanAccess ? 'http://192.168.1.20:3212' : 'http://127.0.0.1:3212',
      capabilities: [],
    };
  }
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
    profile: null,
    path: agentPaths.codex ?? '/project/.ossia/integrations/hosts/codex/marketplace/plugins/ossia',
    reloadHint: { kind: 'restart-session', agent: 'Codex' },
    message: null,
    conflicts: [],
    hosts: [],
  };
  const available: AgentIntegration = {
    ...connected,
    id: 'dsh',
    name: 'DeepSeek Harness',
    icon: 'dsh',
    state: dshState,
    installed: false,
    enabled: false,
    configRoot: '/Users/example/.dsh',
    configPath: '/Users/example/.dsh/profiles/desktop/package.json',
    executable: '/Applications/DeepSeek Harness.app/Contents/Resources/runtime/cli/bin/dsh',
    path: agentPaths.dsh ?? '/project/.ossia/integrations/hosts/dsh/ossia-dsh-plugin',
    profile: 'desktop',
    version: '0.2.2-rc.1',
    reloadHint: { kind: 'restart-session', agent: 'DSH' },
    conflicts:
      dshState === 'conflict'
        ? [
            {
              target: '/project/.ossia/integrations/hosts/dsh/ossia-dsh-plugin',
              kind: 'user-config',
              message: 'the managed DSH bundle link points to another location',
            },
          ]
        : [],
  };
  const workbuddyAgents: AgentIntegration[] = workbuddies.map((item, index) => {
    const state = item.state ?? 'available';
    const installed = item.installed ?? (state === 'installed' || state === 'disabled');
    const configRoot =
      item.configRoot ?? `/Users/example/.workbuddy${index === 0 ? '' : `-${index + 1}`}`;
    return {
      ...connected,
      id: item.id ?? `workbuddy:${index}`,
      name: 'WorkBuddy',
      icon: 'workbuddy',
      state,
      installed,
      enabled: item.enabled ?? state === 'installed',
      version: item.version ?? '5.11.3 (CLI 2.176.0)',
      executable:
        '/Applications/WorkBuddy.app/Contents/Resources/app.asar.unpacked/cli/bin/codebuddy',
      configRoot,
      configPath: `${configRoot}/plugins/installed_plugins.json`,
      profile: null,
      path:
        item.path ??
        `/project/.ossia/integrations/hosts/workbuddy/${index}/marketplace/plugins/ossia`,
      reloadHint: { kind: 'workbuddy-reload-and-trust' },
      message:
        state === 'broken'
          ? { kind: 'workbuddy-plugin-missing' }
          : state === 'disabled'
            ? { kind: 'workbuddy-plugin-disabled' }
            : state === 'conflict'
              ? { kind: 'workbuddy-unmanaged-config' }
              : null,
      conflicts:
        state === 'conflict'
          ? [
              {
                target: `/project/.ossia/integrations/hosts/workbuddy/${index}/marketplace`,
                kind: 'user-config',
                message: 'the WorkBuddy marketplace name belongs to another source',
              },
            ]
          : [],
    };
  });
  const claudeAgent: AgentIntegration = {
    ...connected,
    id: 'claude',
    name: 'Claude',
    icon: 'claude',
    state: 'broken',
    enabled: false,
    hosts: [
      {
        name: 'Claude Desktop',
        error: null,
        method: 'mcp',
        state: 'installed',
        version: '1',
        configPath: '/claude/desktop.json',
        reloadHint: { kind: 'claude-desktop-restart' },
        conflicts: [],
      },
      {
        name: 'Claude Code',
        error: null,
        method: 'plugin',
        state: 'broken',
        version: '2',
        configPath: '/claude/code.json',
        reloadHint: { kind: 'restart-session', agent: 'Claude Code' },
        conflicts: [],
      },
    ],
  };
  if (claude === 'desktop' || claude === 'code') {
    claudeAgent.hosts = claudeAgent.hosts.filter((host) =>
      claude === 'desktop' ? host.name === 'Claude Desktop' : host.name === 'Claude Code',
    );
    claudeAgent.name = claude === 'desktop' ? 'Claude Desktop' : 'Claude Code';
  }
  const agents: AgentsSnapshot = {
    agents: empty
      ? []
      : [
          connected,
          available,
          ...workbuddyAgents,
          ...(claude ? [claudeAgent] : []),
          ...(pi ? [{ ...connected, id: 'pi', name: 'Pi', icon: 'pi' }] : []),
        ],
    connectionOrder: empty ? [] : ['codex'],
    syncing: false,
    error: null,
  };
  const storage: StorageStatus = {
    home: '/tmp/ossia',
    totalBytes: 1_500_000_000,
    coreBytes: 1_400_000_000,
    cacheBytes: 100_000_000,
    cacheLimitBytes: 1_073_741_824,
    databaseBytes: 10_000_000,
    documentsBytes: 2_000_000,
    contentsBytes: 3_000_000,
    artifactsBytes: 100_000_000,
    transientBytes: 0,
    integrationsBytes: 1_000_000,
    documentationBytes: 0,
    jobCount: 42,
    renderCount: 18,
    coreWarning: false,
    idle: true,
  };
  const update: UpdateSnapshot = updateState ?? {
    state: 'idle',
    isDebugBuild: false,
    current: 'v0.1.0',
    source: 'ossia',
    channel: 'stable',
    latest: null,
    error: null,
    done: null,
    total: null,
    notes: null,
    staged: false,
  };
  const payload = JSON.stringify({
    preferences,
    status,
    agents,
    storage,
    initializing,
    scores,
    archivedScores,
    previewStates,
    update,
    releaseNotesState,
  });
  await page.addInitScript((raw) => {
    const {
      preferences,
      status,
      agents,
      storage,
      initializing,
      scores,
      archivedScores,
      previewStates,
      update,
      releaseNotesState,
    } = JSON.parse(raw) as {
      preferences: ManagementPreferences;
      status: ManagementSnapshot;
      agents: AgentsSnapshot;
      storage: StorageStatus;
      initializing: boolean;
      scores: ManagementScoreSummary[];
      archivedScores: ManagementScoreSummary[];
      previewStates: Record<string, ManagementScorePreview>;
      update: UpdateSnapshot;
      releaseNotesState?: UpdateSnapshot;
    };
    let callbackId = 0;
    let releaseAgentChange: (() => void) | null = null;
    const commands: string[] = [];
    const agentChanges: AgentsChange[] = [];
    const previewCalls: string[] = [];
    const openedScores: string[] = [];
    const openedLinks: string[] = [];
    const readyPreview: ManagementScorePreview = {
      state: 'ready',
      dataUrl:
        'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIxIiBoZWlnaHQ9IjEiPjwvc3ZnPg==',
    };
    const outcomes = new Map<string, ManagementScorePreview>();
    for (const score of scores) {
      outcomes.set(score.documentId, previewStates[score.documentId] ?? readyPreview);
    }
    for (const score of archivedScores) {
      outcomes.set(score.documentId, previewStates[score.documentId] ?? readyPreview);
    }
    const internals = {
      transformCallback: () => ++callbackId,
      invoke: async (
        command: string,
        args?: {
          token?: string | null;
          change?: AgentsChange | ManagementPreferencesChange | OmrSettingsChange;
          args?: {
            limit?: number;
            offset?: number;
            documentId?: string;
            retry?: boolean;
          };
        },
      ) => {
        commands.push(command);
        if (command === 'management_preferences') return preferences;
        if (command === 'update_state' || command === 'update_check') return update;
        if (command === 'release_notes') return releaseNotesState ?? update;
        if (command === 'update_apply') return update;
        if (command === 'update_cancel') {
          Object.assign(update, { state: 'idle', latest: null, done: null, total: null });
          return { ...update };
        }
        if (command === 'update_token_set') {
          preferences.githubTokenMasked = args?.token ? '••••••••test' : null;
          return { ...preferences };
        }
        if (command === 'open_external_url') {
          openedLinks.push((args as { url?: string } | undefined)?.url ?? '');
          return null;
        }
        if (command === 'management_preferences_change' && args?.change && 'theme' in args.change) {
          Object.assign(preferences, args.change);
          preferences.updateSource = args.change.updateSource;
          preferences.updateChannel = args.change.updateChannel === 'beta' ? 'beta' : 'stable';
          preferences.updateChannelPreference = args.change.updateChannel;
          if (status.runtime) {
            status.runtime.webBaseUrl = preferences.lanAccess
              ? 'http://192.168.1.20:3212'
              : 'http://127.0.0.1:3212';
          }
          return { ...preferences };
        }
        if (
          command === 'management_omr_settings_set' &&
          args?.change &&
          'clearToken' in args.change
        ) {
          preferences.omrService = args.change.service;
          if (args.change.clearToken) {
            if (args.change.service === 'ossia') preferences.omrTokenMasked = null;
            else preferences.omrCustomTokenMasked = null;
          } else if (args.change.token) {
            const masked = `••••••••${args.change.token.slice(-4)}`;
            if (args.change.service === 'ossia') preferences.omrTokenMasked = masked;
            else preferences.omrCustomTokenMasked = masked;
          }
          if (args.change.clearUrl) {
            preferences.omrUrl = null;
          } else if (args.change.url !== null && args.change.url !== undefined) {
            preferences.omrUrl = args.change.url;
          }
          return { ...preferences };
        }
        if (command === 'management_status') return status;
        if (command === 'management_storage_status') return status.runtime ? storage : null;
        if (command === 'management_storage_cleanup') {
          const report: StorageCleanupReport = {
            freedBytes: 2_500_000,
            removedJobs: 2,
            removedRenders: 1,
            removedContentObjects: 0,
            removedCacheFiles: 3,
            removedStagingDirs: 0,
            overLimit: false,
            warnings: [],
          };
          storage.cacheBytes = Math.max(0, storage.cacheBytes - report.freedBytes);
          storage.totalBytes = Math.max(0, storage.totalBytes - report.freedBytes);
          storage.artifactsBytes = storage.cacheBytes;
          storage.jobCount = Math.max(0, storage.jobCount - report.removedJobs);
          storage.renderCount = Math.max(0, storage.renderCount - report.removedRenders);
          return report;
        }
        if (command === 'management_agents_state') {
          if (initializing) {
            await new Promise<void>((resolve) => {
              ownWindow.finishAgentDetection = resolve;
            });
          }
          return agents;
        }
        if (command === 'management_agents_change' && args?.change && 'agentIds' in args.change) {
          agentChanges.push(args.change as AgentsChange);
          await new Promise<void>((resolve) => {
            releaseAgentChange = resolve;
          });
          for (const agent of agents.agents) {
            if (!args.change.agentIds.includes(agent.id)) continue;
            agent.installed = args.change.enabled;
            agent.enabled = args.change.enabled;
            agent.managed = args.change.enabled;
            agent.state = args.change.enabled ? 'installed' : 'available';
            agents.connectionOrder = agents.connectionOrder.filter((id) => id !== agent.id);
            if (args.change.enabled) agents.connectionOrder.push(agent.id);
          }
          return { snapshot: agents, problems: [] };
        }
        if (command === 'management_open_data_directory') return null;
        if (command === 'management_start') {
          status.runtime = {
            runtimeVersion: '0.1.0',
            protocolVersion: 10,
            schemaVersion: 2,
            instanceId: 'test',
            home: status.home,
            webBaseUrl: preferences.lanAccess
              ? 'http://192.168.1.20:3212'
              : 'http://127.0.0.1:3212',
            capabilities: [],
          };
          return status;
        }
        if (command === 'management_stop') {
          status.runtime = null;
          return null;
        }
        if (command === 'management_scores' || command === 'management_archived_scores') {
          const source = command === 'management_scores' ? scores : archivedScores;
          const offset = args?.args?.offset ?? 0;
          const limit = args?.args?.limit ?? 50;
          const items = source.slice(offset, offset + limit);
          const end = offset + items.length;
          return {
            items,
            total: source.length,
            offset,
            nextOffset: end < source.length ? end : null,
          };
        }
        if (command === 'management_score_preview') {
          const documentId = args?.args?.documentId;
          if (!documentId) throw new Error('score preview requires documentId');
          previewCalls.push(documentId);
          return outcomes.get(documentId) ?? { state: 'missing' };
        }
        if (command === 'management_archive_score') {
          const documentId = args?.args?.documentId;
          const index = scores.findIndex((score) => score.documentId === documentId);
          if (index < 0) throw { code: 'not-found', message: 'score not found' };
          const score = scores[index];
          if (!score) throw { code: 'not-found', message: 'score not found' };
          scores.splice(index, 1);
          const archivedAt = new Date().toISOString();
          archivedScores.unshift({ ...score, archivedAt });
          return { ...score, deletedAt: archivedAt };
        }
        if (command === 'management_restore_score') {
          const documentId = args?.args?.documentId;
          const index = archivedScores.findIndex((score) => score.documentId === documentId);
          if (index < 0) throw { code: 'not-found', message: 'archived score not found' };
          const score = archivedScores[index];
          if (!score) throw { code: 'not-found', message: 'archived score not found' };
          archivedScores.splice(index, 1);
          scores.unshift({ ...score, archivedAt: null });
          return {
            documentId: score.documentId,
            url: `http://127.0.0.1:3212/scores?documentId=${score.documentId}`,
            title: score.title,
            headVersionId: null,
            sourceHash: '0'.repeat(64),
            dirty: false,
            editState: 'idle',
          };
        }
        if (command === 'management_delete_score') {
          const documentId = args?.args?.documentId;
          const index = archivedScores.findIndex((score) => score.documentId === documentId);
          if (index < 0) throw { code: 'not-found', message: 'archived score not found' };
          archivedScores.splice(index, 1);
          outcomes.delete(documentId ?? '');
          return { documentId };
        }
        if (command === 'management_open_score') {
          const documentId = args?.args?.documentId;
          if (!documentId) throw new Error('open score requires documentId');
          if (!scores.some((score) => score.documentId === documentId)) {
            throw { code: 'not-found', message: 'active score not found' };
          }
          openedScores.push(documentId);
          return null;
        }
        if (command === 'plugin:event|listen') return 1;
        if (command === 'plugin:event|unlisten') return null;
        throw new Error(`unexpected Tauri command: ${command}`);
      },
    };
    const ownWindow = window as Window & {
      __TAURI_INTERNALS__?: typeof internals;
      managementCommands?: string[];
      managementAgentChanges?: AgentsChange[];
      scorePreviewCalls?: string[];
      openedScores?: string[];
      openedLinks?: string[];
      setScores?: (scores: ManagementScoreSummary[]) => void;
      setScorePreview?: (documentId: string, outcome: ManagementScorePreview) => void;
      finishAgentDetection?: () => void;
      releaseAgentChange?: () => boolean;
      __TAURI_EVENT_PLUGIN_INTERNALS__?: {
        unregisterListener: () => void;
      };
    };
    ownWindow.managementCommands = commands;
    ownWindow.managementAgentChanges = agentChanges;
    ownWindow.scorePreviewCalls = previewCalls;
    ownWindow.openedScores = openedScores;
    ownWindow.openedLinks = openedLinks;
    ownWindow.setScores = (next) => {
      scores.splice(0, scores.length, ...next);
      for (const score of next) {
        if (!outcomes.has(score.documentId)) outcomes.set(score.documentId, readyPreview);
      }
    };
    ownWindow.setScorePreview = (documentId, outcome) => outcomes.set(documentId, outcome);
    ownWindow.releaseAgentChange = () => {
      const release = releaseAgentChange;
      releaseAgentChange = null;
      release?.();
      return Boolean(release);
    };
    ownWindow.__TAURI_INTERNALS__ = internals;
    ownWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {},
    };
  }, payload);
}

/** Release the pending Agent mutation installed by {@link mockManagement}. */
export async function releaseAgentChange(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as Window & { releaseAgentChange?: () => boolean }).releaseAgentChange?.() ??
          false,
      ),
    )
    .toBe(true);
}

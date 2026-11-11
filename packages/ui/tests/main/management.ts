import type { Page } from '@playwright/test';

import type {
  AgentIntegration,
  AgentsChange,
  AgentsSnapshot,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementSnapshot,
  StorageCleanupReport,
  StorageStatus,
} from '../../src/generated/protocol.ts';

/** Install the management window bridge with isolated preferences and Agent state. */
export async function mockManagement(page: Page, { empty = false, initializing = false } = {}) {
  const preferences: ManagementPreferences = {
    theme: 'system',
    language: 'zh-CN',
    trayClickAction: 'window',
    trayClickActionEditable: true,
    storageCacheLimit: 'gib1',
    storageAutoCleanup: true,
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
    profile: null,
    path: '/project/.ossia/integrations/hosts/codex/marketplace/plugins/ossia',
    reloadHint: { kind: 'restart-session', agent: 'Codex' },
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
    executable: '/Applications/DeepSeek Harness.app/Contents/Resources/runtime/cli/bin/dsh',
    path: '/project/.ossia/integrations/hosts/dsh/ossia-dsh-plugin',
    profile: 'desktop',
    version: '0.2.2-rc.1',
    reloadHint: { kind: 'restart-session', agent: 'DSH' },
  };
  const agents: AgentsSnapshot = {
    agents: empty ? [] : [connected, available],
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
  const payload = JSON.stringify({ preferences, status, agents, storage, initializing });
  await page.addInitScript((raw) => {
    const { preferences, status, agents, storage, initializing } = JSON.parse(raw) as {
      preferences: ManagementPreferences;
      status: ManagementSnapshot;
      agents: AgentsSnapshot;
      storage: StorageStatus;
      initializing: boolean;
    };
    let callbackId = 0;
    const commands: string[] = [];
    const internals = {
      transformCallback: () => ++callbackId,
      invoke: async (
        command: string,
        args?: { change: AgentsChange | ManagementPreferencesChange },
      ) => {
        commands.push(command);
        if (command === 'management_preferences') return preferences;
        if (command === 'management_preferences_change' && args && 'theme' in args.change) {
          Object.assign(preferences, args.change);
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
        if (command === 'management_agents_change' && args && 'agentIds' in args.change) {
          await new Promise((resolve) => setTimeout(resolve, 500));
          for (const agent of agents.agents) {
            if (!args.change.agentIds.includes(agent.id)) continue;
            agent.installed = args.change.enabled;
            agent.enabled = args.change.enabled;
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
            protocolVersion: 1,
            schemaVersion: 1,
            instanceId: 'test',
            home: status.home,
            webBaseUrl: 'http://127.0.0.1:3212',
            capabilities: [],
          };
          return status;
        }
        if (command === 'management_stop') {
          status.runtime = null;
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
      finishAgentDetection?: () => void;
      __TAURI_EVENT_PLUGIN_INTERNALS__?: {
        unregisterListener: () => void;
      };
    };
    ownWindow.managementCommands = commands;
    ownWindow.__TAURI_INTERNALS__ = internals;
    ownWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => {},
    };
  }, payload);
}

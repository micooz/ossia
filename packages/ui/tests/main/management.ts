import type { Page } from '@playwright/test';

import type {
  AgentIntegration,
  AgentsChange,
  AgentsSnapshot,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementSnapshot,
} from '../../src/generated/protocol.ts';

/** Install the management window bridge with isolated preferences and Agent state. */
export async function mockManagement(page: Page, { empty = false, initializing = false } = {}) {
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
  const payload = JSON.stringify({ preferences, status, agents, initializing });
  await page.addInitScript((raw) => {
    const { preferences, status, agents, initializing } = JSON.parse(raw) as {
      preferences: ManagementPreferences;
      status: ManagementSnapshot;
      agents: AgentsSnapshot;
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

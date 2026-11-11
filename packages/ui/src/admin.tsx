import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Bot, Info, ServerCog, Settings, ShieldAlert, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { AboutPage } from './admin/AboutPage.tsx';
import { AgentsPage } from './admin/AgentsPage.tsx';
import { ServicePage } from './admin/ServicePage.tsx';
import { SettingsPage } from './admin/SettingsPage.tsx';
import { platformName } from './admin/types.ts';
import type { Page } from './admin/types.ts';
import { NavItem } from './admin/ui.tsx';
import { describe } from './api.ts';
import mark from './assets/ossia-mark.svg';
import type {
  AgentIntegration,
  AgentProblem,
  AgentsMutation,
  AgentsSnapshot,
  DoctorReport,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementSnapshot,
  TrayClickAction,
  UiLanguage,
  UiTheme,
} from './generated/protocol.ts';
import { I18nProvider, matchSystemLanguage, useI18n } from './i18n.tsx';

import './admin.css';

function Admin() {
  const native = '__TAURI_INTERNALS__' in window;
  const detectedLanguage = matchSystemLanguage(navigator.languages);
  const [preferences, setPreferences] = useState<ManagementPreferences | null>(null);
  const [preferenceError, setPreferenceError] = useState('');

  useEffect(() => {
    document.body.dataset.platform = !native ? 'browser' : platformName();
  }, [native]);

  useEffect(() => {
    if (!native) return;
    let live = true;
    void invoke<ManagementPreferences>('management_preferences')
      .then(async (loaded) => {
        if (!loaded.language) {
          const initialized = await invoke<ManagementPreferences>('management_preferences_change', {
            change: {
              theme: loaded.theme,
              language: detectedLanguage,
              trayClickAction: loaded.trayClickAction,
            } satisfies ManagementPreferencesChange,
          });
          if (live) setPreferences(initialized);
        } else if (live) {
          setPreferences(loaded);
        }
      })
      .catch((error) => {
        if (live) setPreferenceError(describe(error));
      });
    return () => {
      live = false;
    };
  }, [detectedLanguage, native]);

  const theme = preferences?.theme ?? 'system';
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme === 'system' ? 'light dark' : theme;
  }, [theme]);

  const language = preferences?.language ?? detectedLanguage;
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  return (
    <I18nProvider language={language}>
      <AdminContent
        native={native}
        preferences={preferences}
        setPreferences={setPreferences}
        preferenceError={preferenceError}
        setPreferenceError={setPreferenceError}
      />
    </I18nProvider>
  );
}

function AdminContent({
  native,
  preferences,
  setPreferences,
  preferenceError,
  setPreferenceError,
}: {
  native: boolean;
  preferences: ManagementPreferences | null;
  setPreferences: (preferences: ManagementPreferences) => void;
  preferenceError: string;
  setPreferenceError: (error: string) => void;
}) {
  const { t } = useI18n();
  const [page, setPage] = useState<Page>('service');
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const [doctor, setDoctor] = useState<DoctorReport | null>(null);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirmStop, setConfirmStop] = useState(false);
  const [preferencesBusy, setPreferencesBusy] = useState(false);
  const [agents, setAgents] = useState<AgentsSnapshot | null>(null);
  const [agentProblems, setAgentProblems] = useState<AgentProblem[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  async function loadAgents() {
    if (!native) return;
    try {
      setAgents(await invoke<AgentsSnapshot>('management_agents_state'));
    } catch (caught) {
      setError(describe(caught));
    }
  }

  async function refreshStatus() {
    if (!native) return;
    try {
      setState(await invoke<ManagementSnapshot>('management_status'));
    } catch (caught) {
      setError(describe(caught));
    }
  }

  useEffect(() => {
    if (!native) return;
    void loadAgents();
    let live = true;
    const disposers: (() => void)[] = [];
    void listen('agents-updated', () => {
      if (live) void loadAgents();
    }).then((dispose) => disposers.push(dispose));
    void listen<{ message?: string }>('runtime-start-failed', (event) => {
      if (live) setError(event.payload.message ?? JSON.stringify(event.payload));
    }).then((dispose) => disposers.push(dispose));
    void refreshStatus();
    const timer = window.setInterval(() => void refreshStatus(), 2000);
    return () => {
      live = false;
      for (const dispose of disposers) dispose();
      window.clearInterval(timer);
    };
  }, [native]);

  async function action(command: 'management_start' | 'management_stop' | 'management_doctor') {
    if (!native) return;
    setWorking(true);
    setError('');
    try {
      if (command === 'management_doctor') {
        setDoctor(await invoke<DoctorReport>(command));
      } else {
        await invoke(command);
      }
      await refreshStatus();
    } catch (caught) {
      setError(describe(caught));
    } finally {
      setWorking(false);
    }
  }

  async function savePreferences(next: ManagementPreferences) {
    if (!native || !preferences || preferencesBusy) return;
    const previous = preferences;
    setPreferenceError('');
    setPreferencesBusy(true);
    setPreferences(next);
    try {
      const saved = await invoke<ManagementPreferences>('management_preferences_change', {
        change: {
          theme: next.theme,
          language: next.language,
          trayClickAction: next.trayClickAction,
        } satisfies ManagementPreferencesChange,
      });
      setPreferences(saved);
    } catch (caught) {
      setPreferences(previous);
      setPreferenceError(`${t('saveFailed')}: ${describe(caught)}`);
    } finally {
      setPreferencesBusy(false);
    }
  }

  async function setTheme(theme: UiTheme) {
    if (!preferences || preferencesBusy || preferences.theme === theme) return;
    await savePreferences({ ...preferences, theme });
  }

  async function setLanguage(language: UiLanguage) {
    if (!preferences || preferencesBusy || preferences.language === language) return;
    await savePreferences({ ...preferences, language });
  }

  async function setTrayClickAction(trayClickAction: TrayClickAction) {
    if (
      !preferences ||
      preferencesBusy ||
      !preferences.trayClickActionEditable ||
      preferences.trayClickAction === trayClickAction
    )
      return;
    await savePreferences({ ...preferences, trayClickAction });
  }

  async function setAutostart(enabled: boolean) {
    if (!native || !preferences || preferencesBusy || !preferences.launchAtLoginEditable) return;
    setPreferenceError('');
    setPreferencesBusy(true);
    try {
      const launchAtLogin = await invoke<boolean>('management_autostart_set', { enabled });
      setPreferences({ ...preferences, launchAtLogin });
    } catch (caught) {
      setPreferenceError(`${t('startupFailed')}: ${describe(caught)}`);
    } finally {
      setPreferencesBusy(false);
    }
  }

  async function changeAgents(agentIds: string[], enabled: boolean) {
    if (!native || working || agentIds.length === 0) return;
    setWorking(true);
    setError('');
    setAgentProblems([]);
    try {
      const result = await invoke<AgentsMutation>('management_agents_change', {
        change: { agentIds, enabled },
      });
      setAgents(result.snapshot);
      setAgentProblems(result.problems);
      if (result.problems.length > 0) {
        setError(result.problems.map((problem) => problem.message).join('; '));
      }
    } catch (caught) {
      setError(describe(caught));
    } finally {
      setWorking(false);
    }
  }

  function toggleAgent(agent: AgentIntegration) {
    if (agent.state === 'conflict' || agent.state === 'unavailable') {
      setSelected((current) => (current === agent.id ? null : agent.id));
      return;
    }
    if (agent.state === 'update-required') {
      void changeAgents([agent.id], true);
      return;
    }
    void changeAgents([agent.id], !(agent.installed && agent.enabled));
  }

  const list = agents?.agents ?? [];
  const busy = working || !!agents?.syncing;
  const pages: Record<Page, { group: string; title: string }> = {
    service: { group: t('integrationGroup'), title: t('service') },
    agents: { group: t('integrationGroup'), title: t('agents') },
    general: { group: t('settings'), title: t('general') },
    about: { group: t('settings'), title: t('about') },
  };
  const current = pages[page];
  const platform = native ? platformName() : 'browser';

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-bar" data-tauri-drag-region="deep">
          <img className="brand-mark" src={mark} alt="" />
          <span className="brand-name">ossia</span>
        </div>
        <nav className="navigation" aria-label={t('settings')}>
          <div className="nav-group">
            <p className="nav-heading">{t('integrationGroup')}</p>
            <NavItem
              active={page === 'service'}
              icon={<ServerCog />}
              label={t('service')}
              onClick={() => setPage('service')}
            />
            <NavItem
              active={page === 'agents'}
              icon={<Bot />}
              label={t('agents')}
              onClick={() => setPage('agents')}
            />
          </div>
          <div className="nav-group">
            <p className="nav-heading">{t('settings')}</p>
            <NavItem
              active={page === 'general'}
              icon={<Settings />}
              label={t('general')}
              onClick={() => setPage('general')}
            />
            <NavItem
              active={page === 'about'}
              icon={<Info />}
              label={t('about')}
              onClick={() => setPage('about')}
            />
          </div>
        </nav>
      </aside>
      <main className="workspace">
        <header className="page-header" data-tauri-drag-region="deep">
          <div className="breadcrumb" aria-label={`${current.group} / ${current.title}`}>
            <span>{current.group}</span>
            <span className="breadcrumb-divider">/</span>
            <strong>{current.title}</strong>
          </div>
          <div className="header-actions">
            <span className={`service-badge${state?.runtime ? ' is-on' : ''}`} role="status">
              <span />
              {state?.runtime ? t('serviceRunning') : t('serviceStopped')}
            </span>
            <div className="window-controls" hidden={!native || platform === 'macos'}>
              <button
                type="button"
                aria-label={t('close')}
                title={t('close')}
                onClick={() => void getCurrentWindow().close()}
              >
                <X />
              </button>
            </div>
          </div>
        </header>

        <div className="page-content">
          {(error || preferenceError || state?.error) && (
            <div className="alert error" role="alert">
              <ShieldAlert aria-hidden="true" />
              <span>{error || preferenceError || state?.error?.message}</span>
            </div>
          )}
          {!native && (
            <div className="alert warn" role="alert">
              <Info aria-hidden="true" />
              <span>{t('nonNativeAdmin')}</span>
            </div>
          )}

          {page === 'service' && (
            <ServicePage
              native={native}
              state={state}
              doctor={doctor}
              working={working}
              confirmStop={confirmStop}
              onAction={(command) => {
                if (command === 'management_stop') setConfirmStop(false);
                void action(command);
              }}
              onRefresh={() => void refreshStatus()}
              onToggleConfirm={() => setConfirmStop(!confirmStop)}
            />
          )}
          {page === 'agents' && (
            <AgentsPage
              native={native}
              agents={agents}
              list={list}
              busy={busy}
              selected={selected}
              problems={agentProblems}
              onToggle={toggleAgent}
            />
          )}
          {page === 'general' && (
            <SettingsPage
              native={native}
              preferences={preferences}
              busy={preferencesBusy}
              platform={platform}
              onTheme={(theme) => void setTheme(theme)}
              onLanguage={(language) => void setLanguage(language)}
              onTrayClick={(action) => void setTrayClickAction(action)}
              onAutostart={(enabled) => void setAutostart(enabled)}
            />
          )}
          {page === 'about' && <AboutPage preferences={preferences} state={state} />}
        </div>
      </main>
    </div>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Admin />);

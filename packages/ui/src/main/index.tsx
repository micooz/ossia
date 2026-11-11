import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Bot, Info, Settings, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';

import { describe } from '../errors.ts';
import type {
  AgentIntegration,
  AgentProblem,
  AgentsMutation,
  AgentsSnapshot,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementSnapshot,
  StorageCacheLimit,
  StorageCleanupReport,
  StorageStatus,
  TrayClickAction,
  UiLanguage,
  UiTheme,
} from '../generated/protocol.ts';
import { I18nProvider, matchSystemLanguage, useI18n } from '../i18n.tsx';
import { AboutPage } from './AboutPage.tsx';
import { AgentsPage } from './AgentsPage.tsx';
import mark from './assets/ossia-mark.svg';
import { ServiceDialog } from './ServiceDialog.tsx';
import { SettingsPage } from './SettingsPage.tsx';
import { ToastRegion, useToasts } from './Toast.tsx';
import type { ToastKind } from './Toast.tsx';
import { platformName } from './types.ts';
import type { Page } from './types.ts';
import { NavItem, formatBytes } from './ui.tsx';

import './main.css';

function Main() {
  const native = '__TAURI_INTERNALS__' in window;
  const detectedLanguage = matchSystemLanguage(navigator.languages);
  const toast = useToasts();
  const [preferences, setPreferences] = useState<ManagementPreferences | null>(null);

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
              storageCacheLimit: loaded.storageCacheLimit,
              storageAutoCleanup: loaded.storageAutoCleanup,
            } satisfies ManagementPreferencesChange,
          });
          if (live) setPreferences(initialized);
        } else if (live) {
          setPreferences(loaded);
        }
      })
      .catch((error) => {
        if (live) toast.show(describe(error), 'error');
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
      <MainContent
        native={native}
        preferences={preferences}
        setPreferences={setPreferences}
        notify={toast.show}
      />
      <ToastRegion toasts={toast.toasts} onDismiss={toast.dismiss} />
    </I18nProvider>
  );
}

function MainContent({
  native,
  preferences,
  setPreferences,
  notify,
}: {
  native: boolean;
  preferences: ManagementPreferences | null;
  setPreferences: (preferences: ManagementPreferences) => void;
  notify: (message: string, kind?: ToastKind) => void;
}) {
  const { t } = useI18n();
  const [page, setPage] = useState<Page>(() => {
    const saved = localStorage.getItem('ossia:management:last-page');
    return saved === 'general' || saved === 'about' ? saved : 'agents';
  });
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const serviceDialog = useRef<HTMLDialogElement>(null);
  const [openingDirectory, setOpeningDirectory] = useState(false);
  const [pendingAgents, setPendingAgents] = useState<string[]>([]);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirmStop, setConfirmStop] = useState(false);
  const [preferencesBusy, setPreferencesBusy] = useState(false);
  const [agents, setAgents] = useState<AgentsSnapshot | null>(null);
  const [agentProblems, setAgentProblems] = useState<AgentProblem[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [storageStatus, setStorageStatus] = useState<StorageStatus | null>(null);
  const [storageResult, setStorageResult] = useState<StorageCleanupReport | null>(null);
  const [storageBusy, setStorageBusy] = useState(false);

  useEffect(() => {
    localStorage.setItem('ossia:management:last-page', page);
  }, [page]);

  useEffect(() => {
    if (!native) notify(t('nonNativeMain'), 'warning');
  }, [native, notify, t]);

  useEffect(() => {
    if (state?.error) notify(state.error.message, 'error');
  }, [notify, state?.error?.message]);

  async function loadAgents() {
    if (!native) return;
    try {
      setAgents(await invoke<AgentsSnapshot>('management_agents_state'));
    } catch (caught) {
      notify(describe(caught), 'error');
    }
  }

  async function refreshStatus() {
    if (!native) return;
    try {
      setState(await invoke<ManagementSnapshot>('management_status'));
    } catch (caught) {
      notify(describe(caught), 'error');
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
      if (live) notify(event.payload.message ?? JSON.stringify(event.payload), 'error');
    }).then((dispose) => disposers.push(dispose));
    void refreshStatus();
    const timer = window.setInterval(() => void refreshStatus(), 2000);
    return () => {
      live = false;
      for (const dispose of disposers) dispose();
      window.clearInterval(timer);
    };
  }, [native]);

  useEffect(() => {
    if (!native) return;
    if (page !== 'general' || !state?.runtime) {
      setStorageStatus(null);
      setStorageResult(null);
      return;
    }
    let live = true;
    const refresh = () => {
      void invoke<StorageStatus>('management_storage_status')
        .then((status) => {
          if (live) setStorageStatus(status);
        })
        .catch((caught) => {
          if (live) notify(`${t('storageLoadFailed')}: ${describe(caught)}`, 'error');
        });
    };
    refresh();
    const timer = window.setInterval(refresh, 5000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [native, page, state?.runtime?.instanceId, t]);

  async function action(command: 'management_start' | 'management_stop') {
    if (!native) return;
    setWorking(true);
    setError('');
    try {
      await invoke(command);
      await refreshStatus();
      notify(
        command === 'management_start' ? t('serviceStartedToast') : t('serviceStoppedToast'),
        'success',
      );
    } catch (caught) {
      const message = describe(caught);
      setError(message);
      notify(message, 'error');
    } finally {
      setWorking(false);
    }
  }

  async function openDirectory() {
    if (!native || openingDirectory) return;
    setOpeningDirectory(true);
    setError('');
    try {
      await invoke('management_open_data_directory');
    } catch (caught) {
      const message = describe(caught);
      setError(message);
      notify(message, 'error');
    } finally {
      setOpeningDirectory(false);
    }
  }

  async function savePreferences(next: ManagementPreferences) {
    if (!native || !preferences || preferencesBusy) return;
    const previous = preferences;
    setPreferencesBusy(true);
    setPreferences(next);
    try {
      const saved = await invoke<ManagementPreferences>('management_preferences_change', {
        change: {
          theme: next.theme,
          language: next.language,
          trayClickAction: next.trayClickAction,
          storageCacheLimit: next.storageCacheLimit,
          storageAutoCleanup: next.storageAutoCleanup,
        } satisfies ManagementPreferencesChange,
      });
      setPreferences(saved);
      return true;
    } catch (caught) {
      setPreferences(previous);
      notify(`${t('saveFailed')}: ${describe(caught)}`, 'error');
      return false;
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
    setPreferencesBusy(true);
    try {
      const launchAtLogin = await invoke<boolean>('management_autostart_set', { enabled });
      setPreferences({ ...preferences, launchAtLogin });
    } catch (caught) {
      notify(`${t('startupFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setPreferencesBusy(false);
    }
  }

  async function cleanStorage() {
    if (!native || !state?.runtime || storageBusy) return;
    setStorageBusy(true);
    try {
      const result = await invoke<StorageCleanupReport>('management_storage_cleanup');
      setStorageResult(result);
      setStorageStatus(await invoke<StorageStatus>('management_storage_status'));
      notify(
        t('storageCleanupResult', {
          size: formatBytes(result.freedBytes),
          jobs: result.removedJobs,
          renders: result.removedRenders,
        }),
        'success',
      );
    } catch (caught) {
      notify(`${t('storageCleanupFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setStorageBusy(false);
    }
  }

  async function setStorageLimit(limit: StorageCacheLimit) {
    if (!preferences || preferencesBusy || preferences.storageCacheLimit === limit) return;
    const previous = preferences.storageCacheLimit;
    const saved = await savePreferences({ ...preferences, storageCacheLimit: limit });
    const rank: Record<StorageCacheLimit, number> = {
      mib512: 0,
      gib1: 1,
      gib3: 2,
      gib10: 3,
    };
    if (!saved || rank[limit] >= rank[previous] || !native || !state?.runtime) return;
    setStorageBusy(true);
    try {
      const result = await invoke<StorageCleanupReport>('management_storage_cleanup');
      setStorageResult(result);
      setStorageStatus(await invoke<StorageStatus>('management_storage_status'));
      notify(
        t('storageCleanupResult', {
          size: formatBytes(result.freedBytes),
          jobs: result.removedJobs,
          renders: result.removedRenders,
        }),
        'success',
      );
    } catch (caught) {
      if ((caught as { code?: string } | null)?.code === 'conflict') {
        notify(t('storageBusyTasks'), 'warning');
      } else {
        notify(`${t('storageCleanupFailed')}: ${describe(caught)}`, 'error');
      }
    } finally {
      setStorageBusy(false);
    }
  }

  async function setAutoCleanup(enabled: boolean) {
    if (!preferences || preferencesBusy || preferences.storageAutoCleanup === enabled) return;
    await savePreferences({ ...preferences, storageAutoCleanup: enabled });
  }

  async function changeAgents(agentIds: string[], enabled: boolean) {
    if (!native || working || agentIds.length === 0) return;
    setWorking(true);
    setAgentProblems([]);
    setPendingAgents(agentIds);
    try {
      const result = await invoke<AgentsMutation>('management_agents_change', {
        change: { agentIds, enabled },
      });
      setAgents(result.snapshot);
      setAgentProblems(result.problems);
      if (result.problems.length > 0) {
        notify(result.problems.map((problem) => problem.message).join('; '), 'error');
      }
    } catch (caught) {
      notify(describe(caught), 'error');
    } finally {
      setWorking(false);
      setPendingAgents([]);
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
            <button
              type="button"
              aria-haspopup="dialog"
              onClick={() => {
                setConfirmStop(false);
                serviceDialog.current?.showModal();
                void refreshStatus();
              }}
              className={`service-badge${state?.runtime ? ' is-on' : ''}`}
            >
              <span />
              {state?.runtime ? t('serviceRunning') : t('serviceStopped')}
            </button>
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
          {page === 'agents' && (
            <AgentsPage
              native={native}
              agents={agents}
              list={list}
              busy={busy}
              pendingAgents={pendingAgents}
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
              onStorageLimit={(limit) => void setStorageLimit(limit)}
              onCleanup={() => void cleanStorage()}
              onAutoCleanup={(enabled) => void setAutoCleanup(enabled)}
              runtimeRunning={!!state?.runtime}
              storage={storageStatus}
              storageBusy={storageBusy}
              storageResult={storageResult}
            />
          )}
          {page === 'about' && (
            <AboutPage
              preferences={preferences}
              state={state}
              native={native}
              openingDirectory={openingDirectory}
              onOpenDirectory={() => void openDirectory()}
            />
          )}
        </div>
      </main>
      <ServiceDialog
        dialogRef={serviceDialog}
        native={native}
        state={state}
        working={working}
        confirmStop={confirmStop}
        openingDirectory={openingDirectory}
        error={error || state?.error?.message || ''}
        onOpenDirectory={() => void openDirectory()}
        onAction={(command) => {
          setConfirmStop(false);
          void action(command);
        }}
        onToggleConfirm={() => setConfirmStop(!confirmStop)}
      />
    </div>
  );
}

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');
createRoot(root).render(<Main />);

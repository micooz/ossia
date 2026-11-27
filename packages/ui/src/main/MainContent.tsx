import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { ArrowUpCircle, Bot, Info, Library, Settings, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';

import type { ToastKind } from '../components/Toast.tsx';
import { describe } from '../errors.ts';
import type {
  AgentIntegration,
  AgentProblem,
  AgentsMutation,
  AgentsSnapshot,
  ManagementPreferences,
  ManagementPreferencesChange,
  ManagementSnapshot,
  OmrService,
  OmrSettingsChange,
  StartupAction,
  StorageCacheLimit,
  StorageCleanupReport,
  StorageStatus,
  TrayClickAction,
  UiLanguage,
  UiTheme,
} from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import icon from './assets/ossia-icon.svg';
import logo from './assets/ossia-logo.svg';
import { AboutPage } from './components/AboutPage.tsx';
import { AgentsPage } from './components/AgentsPage.tsx';
import { MyScoresPage } from './components/MyScoresPage.tsx';
import { ServiceDialog } from './components/ServiceDialog.tsx';
import { SettingsPage } from './components/SettingsPage.tsx';
import { NavItem, formatBytes } from './components/ui.tsx';
import { useUpdate } from './hooks/useUpdate.ts';
import { platformName } from './types.ts';
import type { Page } from './types.ts';
import { openDialog } from './utils/dialog.ts';

interface MainContentProps {
  native: boolean;
  preferences: ManagementPreferences | null;
  setPreferences: Dispatch<SetStateAction<ManagementPreferences | null>>;
  notify: (message: string, kind?: ToastKind) => void;
}

export function MainContent({ native, preferences, setPreferences, notify }: MainContentProps) {
  const { t } = useI18n();
  const [page, setPage] = useState<Page>(() => {
    const saved = localStorage.getItem('ossia:management:last-page');
    return saved === 'scores' || saved === 'general' || saved === 'about' ? saved : 'agents';
  });
  const [state, setState] = useState<ManagementSnapshot | null>(null);
  const serviceDialog = useRef<HTMLDialogElement>(null);
  const [openingDirectory, setOpeningDirectory] = useState(false);
  const [pendingAgents, setPendingAgents] = useState<string[]>([]);
  const pendingAgentIds = useRef(new Set<string>());
  const agentQueue = useRef(Promise.resolve());
  const agentRequest = useRef(0);
  const previousPage = useRef(page);
  const [error, setError] = useState('');
  const [working, setWorking] = useState(false);
  const [confirmStop, setConfirmStop] = useState(false);
  const [preferencesBusy, setPreferencesBusy] = useState(false);
  const [agents, setAgents] = useState<AgentsSnapshot | null>(null);
  const [agentLoadError, setAgentLoadError] = useState('');
  const [agentProblems, setAgentProblems] = useState<AgentProblem[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [storageStatus, setStorageStatus] = useState<StorageStatus | null>(null);
  const [storageResult, setStorageResult] = useState<StorageCleanupReport | null>(null);
  const [storageBusy, setStorageBusy] = useState(false);
  const [pendingLanAccess, setPendingLanAccess] = useState<boolean | null>(null);
  const update = useUpdate(native, (message) => notify(message, 'error'));

  useEffect(() => {
    localStorage.setItem('ossia:management:last-page', page);
  }, [page]);

  useEffect(() => {
    if (!native) notify(t('nonNativeMain'), 'warning');
  }, [native, notify, t]);

  useEffect(() => {
    if (state?.error) notify(state.error.message, 'error');
  }, [notify, state?.error?.message]);

  async function loadAgents(silent = false) {
    if (!native) return;
    if (pendingAgentIds.current.size > 0) {
      void agentQueue.current.then(() => loadAgents(silent));
      return;
    }
    const request = ++agentRequest.current;
    try {
      const snapshot = await invoke<AgentsSnapshot>('management_agents_state');
      if (request !== agentRequest.current) return;
      setAgents(snapshot);
      setAgentLoadError('');
    } catch (caught) {
      if (request !== agentRequest.current || silent) return;
      setAgents(null);
      setAgentLoadError(describe(caught));
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
    if (native && page === 'agents' && previousPage.current !== 'agents') void loadAgents(true);
    previousPage.current = page;
  }, [native, page]);

  useEffect(() => {
    if (!native) return;
    const refresh = () => void loadAgents(true);
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
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
          ...(previous.theme !== next.theme && { theme: next.theme }),
          ...(previous.language !== next.language && next.language && { language: next.language }),
          ...(previous.trayClickAction !== next.trayClickAction && {
            trayClickAction: next.trayClickAction,
          }),
          ...(previous.startupAction !== next.startupAction && {
            startupAction: next.startupAction,
          }),
          ...(previous.storageCacheLimit !== next.storageCacheLimit && {
            storageCacheLimit: next.storageCacheLimit,
          }),
          ...(previous.storageAutoCleanup !== next.storageAutoCleanup && {
            storageAutoCleanup: next.storageAutoCleanup,
          }),
          ...(previous.lanAccess !== next.lanAccess && { lanAccess: next.lanAccess }),
          ...(previous.autoDownload !== next.autoDownload && { autoDownload: next.autoDownload }),
          ...(previous.updateChannelPreference !== next.updateChannelPreference && {
            updateChannel: next.updateChannelPreference,
          }),
          ...(previous.updateSource !== next.updateSource && { updateSource: next.updateSource }),
        } satisfies ManagementPreferencesChange,
      });
      setPreferences(
        (current) =>
          current && {
            ...saved,
            language: current.language !== next.language ? current.language : saved.language,
          },
      );
      if (previous.lanAccess !== saved.lanAccess) await refreshStatus();
      return true;
    } catch (caught) {
      setPreferences(
        (current) =>
          current && {
            ...previous,
            language: current.language !== next.language ? current.language : previous.language,
          },
      );
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

  async function setStartupAction(startupAction: StartupAction) {
    if (!preferences || preferencesBusy || preferences.startupAction === startupAction) return;
    await savePreferences({ ...preferences, startupAction });
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
      setPreferences((current) => current && { ...current, launchAtLogin });
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

  async function setAutoDownload(enabled: boolean) {
    if (!preferences || preferencesBusy || preferences.autoDownload === enabled) return;
    await savePreferences({ ...preferences, autoDownload: enabled });
  }

  async function setUpdateChannel(updateChannel: ManagementPreferences['updateChannel']) {
    if (!preferences || preferencesBusy || preferences.updateChannel === updateChannel) return;
    await savePreferences({
      ...preferences,
      updateChannel,
      updateChannelPreference: updateChannel,
    });
  }

  async function setUpdateSource(updateSource: ManagementPreferences['updateSource']) {
    if (!preferences || preferencesBusy || preferences.updateSource === updateSource) return;
    await savePreferences({ ...preferences, updateSource });
  }

  async function setUpdateToken(token: string | null) {
    if (!native) return false;
    try {
      const saved = await invoke<ManagementPreferences>('update_token_set', { token });
      setPreferences(
        (current) =>
          current && {
            ...saved,
            language:
              current.language !== preferences?.language ? current.language : saved.language,
          },
      );
      return true;
    } catch (caught) {
      notify(
        `${t(token ? 'githubTokenSaveFailed' : 'githubTokenRemoveFailed')}: ${describe(caught)}`,
        'error',
      );
      return false;
    }
  }

  async function saveOmrSettings(change: OmrSettingsChange) {
    if (!native) return false;
    setPreferencesBusy(true);
    try {
      const saved = await invoke<ManagementPreferences>('management_omr_settings_set', {
        change,
      });
      setPreferences(
        (current) =>
          current && {
            ...saved,
            language:
              current.language !== preferences?.language ? current.language : saved.language,
          },
      );
      return true;
    } catch (caught) {
      notify(`${t('omrSettingsSaveFailed')}: ${describe(caught)}`, 'error');
      return false;
    } finally {
      setPreferencesBusy(false);
    }
  }

  async function clearOmrToken(service: OmrService) {
    if (!native) return false;
    setPreferencesBusy(true);
    try {
      const saved = await invoke<ManagementPreferences>('management_omr_settings_set', {
        change: { service, url: null, token: null, clearUrl: false, clearToken: true },
      });
      setPreferences(
        (current) =>
          current && {
            ...saved,
            language:
              current.language !== preferences?.language ? current.language : saved.language,
          },
      );
      return true;
    } catch (caught) {
      notify(`${t('omrSettingsSaveFailed')}: ${describe(caught)}`, 'error');
      return false;
    } finally {
      setPreferencesBusy(false);
    }
  }

  function openExternal(url: string) {
    if (!native) return;
    void invoke('open_external_url', { url }).catch((caught) => notify(describe(caught), 'error'));
  }

  async function applyLanAccess() {
    if (!preferences || preferencesBusy || pendingLanAccess === null) return;
    const enabled = pendingLanAccess;
    setPendingLanAccess(null);
    if (await savePreferences({ ...preferences, lanAccess: enabled })) {
      notify(t(enabled ? 'lanEnabledToast' : 'lanDisabledToast'), 'success');
    }
  }

  function changeAgents(agentIds: string[], enabled: boolean, overwrite = false) {
    if (!native || agentIds.length === 0 || agentIds.some((id) => pendingAgentIds.current.has(id)))
      return;
    agentRequest.current++;
    for (const id of agentIds) pendingAgentIds.current.add(id);
    setPendingAgents([...pendingAgentIds.current]);
    setAgentProblems((current) => current.filter((problem) => !agentIds.includes(problem.agentId)));
    agentQueue.current = agentQueue.current.then(() =>
      applyAgentChange(agentIds, enabled, overwrite),
    );
  }

  async function applyAgentChange(agentIds: string[], enabled: boolean, overwrite: boolean) {
    try {
      const result = await invoke<AgentsMutation>('management_agents_change', {
        change: { agentIds, enabled, overwrite },
      });
      setAgents(result.snapshot);
      setAgentProblems((current) => [...current, ...result.problems]);
      if (result.problems.length > 0) {
        notify(result.problems.map((problem) => problem.message).join('; '), 'error');
      }
    } catch (caught) {
      notify(describe(caught), 'error');
    } finally {
      for (const id of agentIds) pendingAgentIds.current.delete(id);
      setPendingAgents([...pendingAgentIds.current]);
    }
  }

  function toggleAgent(agent: AgentIntegration) {
    if (agent.error) {
      if (!agent.managed || pendingAgentIds.current.has(agent.id)) return;
      agentRequest.current++;
      pendingAgentIds.current.add(agent.id);
      setPendingAgents([...pendingAgentIds.current]);
      agentQueue.current = agentQueue.current.then(async () => {
        try {
          setAgents(
            await invoke<AgentsSnapshot>('management_agents_forget', { agentId: agent.id }),
          );
          setAgentProblems((current) => current.filter((problem) => problem.agentId !== agent.id));
          notify(t('forgetAgentDone'), 'success');
        } catch (caught) {
          notify(describe(caught), 'error');
        } finally {
          pendingAgentIds.current.delete(agent.id);
          setPendingAgents([...pendingAgentIds.current]);
        }
      });
      return;
    }
    if (agent.state === 'conflict') {
      changeAgents([agent.id], true, true);
      return;
    }
    if (agent.state === 'unavailable') {
      setSelected((current) => (current === agent.id ? null : agent.id));
      return;
    }
    if (agent.state === 'update-required') {
      changeAgents([agent.id], true);
      return;
    }
    if (agent.hosts.length > 0 && agent.managed) {
      changeAgents([agent.id], false);
      return;
    }
    changeAgents([agent.id], !(agent.installed && agent.enabled));
  }

  const list = agents?.agents ?? [];
  const busy = !!agents?.syncing;
  const pages: Record<Page, { group: string; title: string }> = {
    agents: { group: t('integrationGroup'), title: t('agents') },
    scores: { group: t('libraryGroup'), title: t('myScores') },
    general: { group: t('settings'), title: t('general') },
    about: { group: t('settings'), title: t('about') },
  };
  const current = pages[page];
  const platform = native ? platformName() : 'browser';
  const updateActionable =
    !!update.snapshot &&
    (update.snapshot.state === 'found' ||
      update.snapshot.state === 'ready' ||
      update.snapshot.state === 'downloading' ||
      update.snapshot.state === 'installing' ||
      (update.snapshot.state === 'checking' && update.snapshot.latest !== null) ||
      (update.snapshot.state === 'error' && update.snapshot.latest !== null));
  const updateBusy =
    update.snapshot?.state === 'checking' ||
    update.snapshot?.state === 'downloading' ||
    update.snapshot?.state === 'installing';
  const updatePercent =
    update.snapshot?.state === 'downloading' && update.snapshot.total
      ? `${Math.floor(((update.snapshot.done ?? 0) / update.snapshot.total) * 100)}%`
      : '';
  const updateHeaderLabel =
    update.snapshot?.state === 'downloading'
      ? [t('updateDownloading'), updatePercent].filter(Boolean).join(' ')
      : update.snapshot?.state === 'installing'
        ? t('updateInstalling')
        : update.snapshot?.state === 'checking'
          ? t('updateChecking')
          : update.snapshot?.isDebugBuild && update.snapshot.state === 'ready'
            ? t('updateDebugReady')
            : update.snapshot?.isDebugBuild
              ? t('updateDebugDownload')
              : t('updateAction');

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-bar" data-tauri-drag-region="deep">
          <img className="brand-icon" src={icon} alt="" />
          <img className="brand-logo" src={logo} alt="Ossia" />
          {update.snapshot?.isDebugBuild && <span className="brand-dev">DEV</span>}
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
            <p className="nav-heading">{t('libraryGroup')}</p>
            <NavItem
              active={page === 'scores'}
              icon={<Library />}
              label={t('myScores')}
              onClick={() => setPage('scores')}
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
            {native && page !== 'about' && updateActionable && (
              <div className="update-header-wrap">
                <button
                  className="update-header-action"
                  type="button"
                  disabled={
                    updateBusy ||
                    (update.snapshot?.isDebugBuild && update.snapshot.state === 'ready')
                  }
                  aria-busy={updateBusy}
                  onClick={() => {
                    if (
                      update.snapshot?.state === 'found' ||
                      update.snapshot?.state === 'ready' ||
                      (update.snapshot?.state === 'error' && update.snapshot.latest !== null)
                    ) {
                      void update.apply();
                    } else {
                      void update.check();
                    }
                  }}
                >
                  <span className="update-action-icon" aria-hidden="true">
                    <ArrowUpCircle />
                  </span>
                  {updateHeaderLabel}
                </button>
                {update.snapshot?.state === 'downloading' && (
                  <button
                    className="update-header-cancel"
                    type="button"
                    aria-label={t('cancelUpdateDownload')}
                    title={t('cancelUpdateDownload')}
                    onClick={() => void update.cancel()}
                  >
                    <X aria-hidden="true" />
                  </button>
                )}
              </div>
            )}
            <button
              type="button"
              aria-haspopup="dialog"
              onClick={(event) => {
                setConfirmStop(false);
                if (serviceDialog.current) openDialog(serviceDialog.current, event.currentTarget);
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
              loadError={agentLoadError}
              list={list}
              busy={busy}
              pendingAgents={pendingAgents}
              selected={selected}
              problems={agentProblems}
              onToggle={toggleAgent}
            />
          )}
          {page === 'scores' && (
            <MyScoresPage
              native={native}
              runtimeRunning={!!state?.runtime}
              runtimeInstanceId={state?.runtime?.instanceId ?? null}
              startingService={working}
              notify={notify}
              onStartService={() => void action('management_start')}
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
              onStartup={(action) => void setStartupAction(action)}
              onTrayClick={(action) => void setTrayClickAction(action)}
              onAutostart={(enabled) => void setAutostart(enabled)}
              onStorageLimit={(limit) => void setStorageLimit(limit)}
              onCleanup={() => void cleanStorage()}
              onAutoCleanup={(enabled) => void setAutoCleanup(enabled)}
              onLanAccess={setPendingLanAccess}
              pendingLanAccess={pendingLanAccess}
              onConfirmLanAccess={() => void applyLanAccess()}
              onCancelLanAccess={() => setPendingLanAccess(null)}
              onOmrSave={saveOmrSettings}
              onOmrClearToken={clearOmrToken}
              accessUrl={state?.runtime?.webBaseUrl ?? null}
              runtimeRunning={!!state?.runtime}
              storage={storageStatus}
              storageBusy={storageBusy}
              storageResult={storageResult}
            />
          )}
          {page === 'about' && (
            <AboutPage
              preferences={preferences}
              preferencesBusy={preferencesBusy}
              native={native}
              update={update.snapshot}
              onCheck={() => void update.check()}
              onApply={() => void update.apply()}
              onCancel={() => void update.cancel()}
              onNotes={async () => (await update.notes())?.notes ?? null}
              onAutoDownload={(enabled) => void setAutoDownload(enabled)}
              onChannel={(channel) => void setUpdateChannel(channel)}
              onSource={(source) => void setUpdateSource(source)}
              onSetUpdateToken={setUpdateToken}
              onOpenExternal={openExternal}
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
        onOpenService={() => {
          void invoke('management_open_service').catch((caught) =>
            notify(describe(caught), 'error'),
          );
        }}
        onAction={(command) => {
          setConfirmStop(false);
          void action(command);
        }}
        onToggleConfirm={() => setConfirmStop(!confirmStop)}
      />
    </div>
  );
}

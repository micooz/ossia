import type {
  ManagementPreferences,
  StorageCacheLimit,
  StorageCleanupReport,
  StorageStatus,
  TrayClickAction,
  UiLanguage,
  UiTheme,
} from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import { Segmented } from './ui.tsx';

export function SettingsPage({
  native,
  preferences,
  busy,
  platform,
  onTheme,
  onLanguage,
  onTrayClick,
  onAutostart,
  onStorageLimit,
  onCleanup,
  onAutoCleanup,
  runtimeRunning,
  storage,
  storageBusy,
  storageResult,
}: {
  native: boolean;
  preferences: ManagementPreferences | null;
  busy: boolean;
  platform: string;
  onTheme: (theme: UiTheme) => void;
  onLanguage: (language: UiLanguage) => void;
  onTrayClick: (action: TrayClickAction) => void;
  onAutostart: (enabled: boolean) => void;
  onStorageLimit: (limit: StorageCacheLimit) => void;
  onCleanup: () => void;
  onAutoCleanup: (enabled: boolean) => void;
  runtimeRunning: boolean;
  storage: StorageStatus | null;
  storageBusy: boolean;
  storageResult: StorageCleanupReport | null;
}) {
  const { language, t } = useI18n();
  const disabled = !native || !preferences || busy;
  const trayEditable = preferences?.trayClickActionEditable ?? platform !== 'linux';
  const autostartEditable = preferences?.launchAtLoginEditable ?? false;
  const formatBytes = (bytes: number) => {
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    let value = bytes;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) {
      value /= 1024;
      unit += 1;
    }
    return `${value < 10 && unit > 0 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
  };
  return (
    <section className="page-section settings-page" aria-labelledby="general-title">
      <h1 id="general-title">{t('general')}</h1>
      <div className="settings-groups">
        <section className="settings-group" aria-labelledby="appearance-title">
          <h2 className="settings-group-title" id="appearance-title">
            {t('appearance')}
          </h2>
          <div className="settings-list">
            <div className="settings-row">
              <span className="setting-label">{t('theme')}</span>
              <Segmented
                ariaLabel={t('theme')}
                value={preferences?.theme ?? 'system'}
                disabled={disabled}
                options={[
                  { value: 'system', label: t('system') },
                  { value: 'light', label: t('light') },
                  { value: 'dark', label: t('dark') },
                ]}
                onChange={(value) => onTheme(value as UiTheme)}
              />
            </div>
            <label className="settings-row">
              <span className="setting-label">{t('language')}</span>
              <select
                className="settings-select"
                value={language}
                disabled={disabled}
                onChange={(event) => onLanguage(event.target.value as UiLanguage)}
              >
                <option value="en-US">English</option>
                <option value="zh-CN">简体中文</option>
              </select>
            </label>
          </div>
        </section>
        <section className="settings-group" aria-labelledby="behavior-title">
          <h2 className="settings-group-title" id="behavior-title">
            {t('behavior')}
          </h2>
          <div className="settings-list">
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('trayClickAction')}</span>
                <span className="setting-note">
                  {t('closeToTrayNote')} {t('quitStopsRuntime')}
                </span>
                {platform === 'linux' && (
                  <span className="setting-note">{t('trayLinuxUnsupported')}</span>
                )}
              </div>
              <Segmented
                ariaLabel={t('trayClickAction')}
                value={preferences?.trayClickAction ?? 'menu'}
                disabled={disabled || !trayEditable}
                options={[
                  { value: 'menu', label: t('trayMenu') },
                  { value: 'window', label: t('trayWindow') },
                ]}
                onChange={(value) => onTrayClick(value as TrayClickAction)}
              />
            </div>
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('launchAtLogin')}</span>
                {preferences && !autostartEditable && (
                  <span className="setting-note">{t('launchAtLoginDebug')}</span>
                )}
              </div>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-checked={preferences?.launchAtLogin ?? false}
                disabled={disabled || !autostartEditable}
                onClick={() => onAutostart(!preferences?.launchAtLogin)}
              >
                <span />
              </button>
            </div>
          </div>
        </section>
        <section className="settings-group" aria-labelledby="storage-title">
          <h2 className="settings-group-title" id="storage-title">
            {t('storage')}
          </h2>
          <div className="settings-list">
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('storageUsage')}</span>
                <span className="setting-note">
                  {storage
                    ? t('storageSummary', {
                        jobs: storage.jobCount,
                        renders: storage.renderCount,
                        core: formatBytes(storage.coreBytes),
                        cache: formatBytes(storage.cacheBytes),
                        transient: formatBytes(storage.transientBytes),
                      })
                    : runtimeRunning
                      ? t('loading')
                      : t('storageServiceRequired')}
                </span>
                {storage?.coreWarning && (
                  <span className="setting-note" data-tone="warn">
                    {t('storageCoreWarning')}
                  </span>
                )}
              </div>
              <span className="setting-value">
                {storage
                  ? `${formatBytes(storage.cacheBytes)} / ${formatBytes(storage.cacheLimitBytes)}`
                  : '—'}
              </span>
            </div>
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('storageCacheLimit')}</span>
                <span className="setting-note">{t('storageNeverDelete')}</span>
              </div>
              <Segmented
                ariaLabel={t('storageCacheLimit')}
                value={preferences?.storageCacheLimit ?? 'gib1'}
                disabled={disabled}
                options={[
                  { value: 'mib512', label: '512 MiB' },
                  { value: 'gib1', label: '1 GB' },
                  { value: 'gib3', label: '3 GB' },
                  { value: 'gib10', label: '10 GB' },
                ]}
                onChange={(value) => onStorageLimit(value as StorageCacheLimit)}
              />
            </div>
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('storageCleanup')}</span>
                <span className="setting-note">
                  {!runtimeRunning
                    ? t('storageServiceRequired')
                    : storage && !storage.idle
                      ? t('storageBusyTasks')
                      : storageResult
                        ? t('storageCleanupResult', {
                            size: formatBytes(storageResult.freedBytes),
                            jobs: storageResult.removedJobs,
                            renders: storageResult.removedRenders,
                          })
                        : preferences?.storageAutoCleanup === false
                          ? t('storageAutoCleanupOff')
                          : t('storageCleanupNote')}
                </span>
              </div>
              <div className="storage-actions">
                <div className="action-row">
                  <button
                    className="text-action"
                    type="button"
                    disabled={disabled || !runtimeRunning || !storage?.idle || storageBusy}
                    onClick={onCleanup}
                  >
                    {storageBusy ? t('working') : t('storageCleanupAction')}
                  </button>
                </div>
                <button
                  className="switch"
                  type="button"
                  role="switch"
                  aria-label={t('storageCleanup')}
                  aria-checked={preferences?.storageAutoCleanup ?? true}
                  disabled={disabled}
                  onClick={() => onAutoCleanup(!(preferences?.storageAutoCleanup ?? true))}
                >
                  <span />
                </button>
              </div>
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}

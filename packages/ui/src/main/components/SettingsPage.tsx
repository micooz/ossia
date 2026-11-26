import { useEffect, useState } from 'react';

import type {
  ManagementPreferences,
  OmrService,
  OmrSettingsChange,
  StartupAction,
  StorageCacheLimit,
  StorageCleanupReport,
  StorageStatus,
  TrayClickAction,
  UiLanguage,
  UiTheme,
} from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { Segmented, formatBytes } from './ui.tsx';

export function SettingsPage({
  native,
  preferences,
  busy,
  platform,
  onTheme,
  onLanguage,
  onStartup,
  onTrayClick,
  onAutostart,
  onStorageLimit,
  onCleanup,
  onAutoCleanup,
  onLanAccess,
  pendingLanAccess,
  onConfirmLanAccess,
  onCancelLanAccess,
  onOmrSave,
  onOmrClearToken,
  accessUrl,
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
  onStartup: (action: StartupAction) => void;
  onTrayClick: (action: TrayClickAction) => void;
  onAutostart: (enabled: boolean) => void;
  onStorageLimit: (limit: StorageCacheLimit) => void;
  onCleanup: () => void;
  onAutoCleanup: (enabled: boolean) => void;
  onLanAccess: (enabled: boolean) => void;
  pendingLanAccess: boolean | null;
  onConfirmLanAccess: () => void;
  onCancelLanAccess: () => void;
  onOmrSave: (change: OmrSettingsChange) => Promise<boolean>;
  onOmrClearToken: (service: OmrService) => Promise<boolean>;
  accessUrl: string | null;
  runtimeRunning: boolean;
  storage: StorageStatus | null;
  storageBusy: boolean;
  storageResult: StorageCleanupReport | null;
}) {
  const { language, t } = useI18n();
  const disabled = !native || !preferences || busy;
  const trayEditable = preferences?.trayClickActionEditable ?? platform !== 'linux';
  const autostartEditable = preferences?.launchAtLoginEditable ?? false;
  const [omrService, setOmrService] = useState<OmrService>(preferences?.omrService ?? 'custom');
  const [omrUrl, setOmrUrl] = useState(preferences?.omrUrl ?? '');
  const [omrToken, setOmrToken] = useState('');
  const [omrBusy, setOmrBusy] = useState(false);
  const [omrError, setOmrError] = useState('');
  const keyConfigured =
    omrService === 'ossia'
      ? Boolean(preferences?.omrTokenMasked)
      : Boolean(preferences?.omrCustomTokenMasked);
  const maskedKey =
    omrService === 'ossia' ? preferences?.omrTokenMasked : preferences?.omrCustomTokenMasked;
  const serviceDirty = omrService !== (preferences?.omrService ?? 'custom');
  const saveVisible = serviceDirty || !keyConfigured;

  useEffect(() => {
    setOmrService(preferences?.omrService ?? 'custom');
    setOmrUrl(preferences?.omrUrl ?? '');
    setOmrToken('');
  }, [preferences?.omrService, preferences?.omrUrl]);

  async function saveOmr(event: React.FormEvent) {
    event.preventDefault();
    setOmrBusy(true);
    setOmrError('');
    try {
      if (
        await onOmrSave({
          service: omrService,
          url: omrService === 'custom' ? omrUrl.trim() || null : null,
          token: omrToken.trim() || null,
          clearUrl: omrService === 'custom' && !omrUrl.trim(),
          clearToken: false,
        })
      ) {
        setOmrToken('');
      } else {
        setOmrError(t('omrSettingsSaveFailed'));
      }
    } finally {
      setOmrBusy(false);
    }
  }

  async function clearOmrToken() {
    setOmrBusy(true);
    setOmrError('');
    try {
      if (!(await onOmrClearToken(omrService))) {
        setOmrError(t('omrSettingsSaveFailed'));
      } else {
        setOmrToken('');
      }
    } finally {
      setOmrBusy(false);
    }
  }

  async function saveOmrService(service: OmrService) {
    setOmrService(service);
    setOmrToken('');
    setOmrError('');
    setOmrBusy(true);
    try {
      const saved = await onOmrSave({
        service,
        url: service === 'custom' ? (preferences?.omrUrl ?? null) : null,
        token: null,
        clearUrl: false,
        clearToken: false,
      });
      if (!saved) setOmrError(t('omrSettingsSaveFailed'));
    } finally {
      setOmrBusy(false);
    }
  }

  async function saveOmrUrl() {
    if (omrService !== 'custom' || !native || !preferences || busy || omrBusy) return;
    const url = omrUrl.trim();
    if (url === (preferences.omrUrl ?? '')) return;
    setOmrBusy(true);
    setOmrError('');
    try {
      if (
        !(await onOmrSave({
          service: 'custom',
          url: url || null,
          token: null,
          clearUrl: !url,
          clearToken: false,
        }))
      ) {
        setOmrError(t('omrSettingsSaveFailed'));
      }
    } finally {
      setOmrBusy(false);
    }
  }

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
              <span className="setting-label">{t('startupAction')}</span>
              <Segmented
                ariaLabel={t('startupAction')}
                value={preferences?.startupAction ?? 'window'}
                disabled={disabled}
                options={[
                  { value: 'window', label: t('startupWindow') },
                  { value: 'tray', label: t('startupTray') },
                ]}
                onChange={(value) => onStartup(value as StartupAction)}
              />
            </div>
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
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label">{t('lanAccess')}</span>
                <span className="setting-note">{t('lanAccessNote')}</span>
                {preferences?.lanAccess && (
                  <span className="setting-note">
                    {accessUrl ? <code>{accessUrl}</code> : t('lanAccessRequiresService')}
                  </span>
                )}
              </div>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-label={t('lanAccess')}
                aria-checked={preferences?.lanAccess ?? false}
                disabled={disabled}
                onClick={() => onLanAccess(!(preferences?.lanAccess ?? false))}
              >
                <span />
              </button>
            </div>
          </div>
          {pendingLanAccess !== null && (
            <div
              className="confirm-card settings-lan-confirm"
              role="alertdialog"
              aria-labelledby="lan-confirm-title"
            >
              <strong id="lan-confirm-title">
                {t(pendingLanAccess ? 'lanEnableTitle' : 'lanDisableTitle')}
              </strong>
              <p>
                {pendingLanAccess
                  ? t(runtimeRunning ? 'lanRestartBody' : 'lanEnableBody')
                  : t(runtimeRunning ? 'lanDisableRestartBody' : 'lanDisableBody')}
              </p>
              <div className="action-row">
                <button type="button" onClick={onCancelLanAccess}>
                  {t('cancel')}
                </button>
                <button
                  className={pendingLanAccess ? 'danger' : 'primary'}
                  type="button"
                  disabled={disabled}
                  onClick={onConfirmLanAccess}
                >
                  {t(pendingLanAccess ? 'lanApply' : 'lanDisableAction')}
                </button>
              </div>
            </div>
          )}
        </section>
        <section className="settings-group" aria-labelledby="score-recognition-title">
          <h2 className="settings-group-title" id="score-recognition-title">
            {t('remoteOmr')}
          </h2>
          <form className="settings-list" onSubmit={(event) => void saveOmr(event)}>
            <div className="settings-row">
              <div className="setting-copy">
                <span className="setting-label" id="omr-enabled-label">
                  {t('omrEnabled')}
                </span>
                <span className="setting-note" id="omr-enabled-description">
                  {t('omrEnabledDescription')}
                </span>
              </div>
              <button
                className="switch"
                type="button"
                role="switch"
                aria-labelledby="omr-enabled-label"
                aria-describedby="omr-enabled-description"
                aria-checked={preferences?.omrEnabled ?? false}
                disabled={disabled || omrBusy}
                onClick={() =>
                  void onOmrSave({
                    service: omrService,
                    enabled: !preferences?.omrEnabled,
                    url: null,
                    token: null,
                    clearUrl: false,
                    clearToken: false,
                  })
                }
              >
                <span />
              </button>
            </div>
            {preferences?.omrEnabled && (
              <>
                <div className="settings-row">
                  <div className="setting-copy">
                    <span className="setting-label">{t('omrService')}</span>
                    <span className="setting-note">{t('omrServiceDescription')}</span>
                  </div>
                  <Segmented
                    ariaLabel={t('omrService')}
                    value={omrService}
                    disabled={!native || !preferences || busy || omrBusy}
                    options={[
                      { value: 'ossia', label: t('omrServiceOssia') },
                      { value: 'custom', label: t('omrServiceCustom') },
                    ]}
                    onChange={(value) => void saveOmrService(value as OmrService)}
                  />
                </div>
                {omrService === 'custom' && (
                  <div className="settings-row github-token-row">
                    <div className="setting-copy">
                      <span className="setting-label">{t('omrUrl')}</span>
                      <span className="setting-note">{t('omrUrlDescription')}</span>
                    </div>
                    <div className="github-token-control">
                      <input
                        className="github-token-input omr-settings-input"
                        type="text"
                        inputMode="url"
                        autoComplete="off"
                        spellCheck={false}
                        aria-label={t('omrUrl')}
                        placeholder="https://omr.example.com"
                        value={omrUrl}
                        disabled={!native || !preferences || busy || omrBusy}
                        onChange={(event) => {
                          setOmrUrl(event.target.value);
                          setOmrError('');
                        }}
                        onBlur={() => void saveOmrUrl()}
                      />
                    </div>
                  </div>
                )}
                <div className="settings-row github-token-row">
                  <div className="setting-copy">
                    <span className="setting-label">{t('omrToken')}</span>
                    <span className="setting-note">{t('omrTokenDescription')}</span>
                  </div>
                  <div className="github-token-control">
                    {keyConfigured ? (
                      <div className="github-token-configured">
                        <code className="github-token-masked">{maskedKey}</code>
                        {serviceDirty && (
                          <button
                            className="secondary-button"
                            type="submit"
                            disabled={!native || !preferences || busy || omrBusy}
                          >
                            {omrBusy ? t('working') : t('save')}
                          </button>
                        )}
                        <button
                          className="secondary-button"
                          type="button"
                          disabled={!native || !preferences || busy || omrBusy}
                          onClick={() => void clearOmrToken()}
                        >
                          {t('remove')}
                        </button>
                      </div>
                    ) : (
                      <div className="omr-settings-inline">
                        <input
                          className="github-token-input omr-settings-input"
                          type="password"
                          autoComplete="off"
                          autoCapitalize="off"
                          spellCheck={false}
                          aria-label={t('omrToken')}
                          placeholder={t('omrToken')}
                          value={omrToken}
                          disabled={!native || !preferences || busy || omrBusy}
                          onChange={(event) => {
                            setOmrToken(event.target.value);
                            setOmrError('');
                          }}
                        />
                        <div className="omr-settings-actions">
                          <button
                            className="secondary-button"
                            type="submit"
                            disabled={!native || !preferences || busy || omrBusy || !saveVisible}
                          >
                            {omrBusy ? t('working') : t('save')}
                          </button>
                        </div>
                      </div>
                    )}
                    {omrError && (
                      <p className="github-token-error" role="alert">
                        {omrError}
                      </p>
                    )}
                  </div>
                </div>
              </>
            )}
          </form>
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
                        core: formatBytes(storage.coreBytes),
                        cache: formatBytes(storage.cacheBytes),
                        transient: formatBytes(storage.transientBytes),
                      })
                    : runtimeRunning
                      ? t('loading')
                      : t('storageServiceRequired')}
                </span>
                {storage && (
                  <span className="setting-note">
                    {t('storageRecords', {
                      jobs: storage.jobCount,
                      renders: storage.renderCount,
                    })}
                  </span>
                )}
                {storage?.coreWarning && (
                  <span className="setting-note" data-tone="warn">
                    {t('storageCoreWarning')}
                  </span>
                )}
              </div>
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
                        ? t('storageCleanupCompleted')
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

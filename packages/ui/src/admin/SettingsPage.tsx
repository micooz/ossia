import type {
  ManagementPreferences,
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
}: {
  native: boolean;
  preferences: ManagementPreferences | null;
  busy: boolean;
  platform: string;
  onTheme: (theme: UiTheme) => void;
  onLanguage: (language: UiLanguage) => void;
  onTrayClick: (action: TrayClickAction) => void;
  onAutostart: (enabled: boolean) => void;
}) {
  const { language, t } = useI18n();
  const disabled = !native || !preferences || busy;
  const trayEditable = preferences?.trayClickActionEditable ?? platform !== 'linux';
  const autostartEditable = preferences?.launchAtLoginEditable ?? false;
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
                <span
                  className="setting-note"
                  title={`${t('closeToTrayNote')} ${t('quitStopsRuntime')}`}
                >
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
      </div>
    </section>
  );
}

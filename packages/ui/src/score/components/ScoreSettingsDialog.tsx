import { Settings } from 'lucide-react';
import { useId, useState } from 'react';

import type { PlaybackPart, SampleInstrument, UiLanguage } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { MessageKey } from '../../i18n.tsx';
import type { useScoreSettings } from '../hooks/useScoreSettings.ts';
import type { Api } from '../utils/api.ts';
import { SharedDialog } from './SharedDialog.tsx';

const names: Record<SampleInstrument, MessageKey> = {
  piano: 'samplePiano',
  guitar: 'sampleGuitar',
  violin: 'sampleViolin',
  ukulele: 'sampleUkulele',
};
const instruments = Object.keys(names) as SampleInstrument[];

/** Score-wide settings; individual sections can be added without changing the entry point. */
export function ScoreSettingsDialog({
  api,
  settings,
  parts,
  onError,
}: {
  api: Api;
  settings: ReturnType<typeof useScoreSettings>;
  parts: PlaybackPart[] | null;
  onError: (error: unknown) => void;
}) {
  const { t, language } = useI18n();
  const controlsId = useId();
  const [saving, setSaving] = useState(false);
  const [savingLanguage, setSavingLanguage] = useState(false);
  const partName = (part: PlaybackPart, index: number) => {
    const instrument = instruments.find((value) => value === part.name.toLowerCase());
    return instrument
      ? t(names[instrument])
      : part.name || t('scoreSettingsPart', { number: index + 1 });
  };
  return (
    <SharedDialog
      title={t('scoreSettings')}
      triggerLabel={t('scoreSettings')}
      triggerIcon={<Settings aria-hidden="true" size={17} />}
      triggerClassName="topbar-action"
      className="score-settings-dialog"
    >
      <section className="shared-dialog-group">
        <h3>{t('appearance')}</h3>
        <div className="score-settings-language">
          <label htmlFor={`${controlsId}-language`}>{t('interfaceLanguage')}</label>
          <select
            id={`${controlsId}-language`}
            value={language}
            disabled={savingLanguage}
            onChange={(event) => {
              const next = event.currentTarget.value as UiLanguage;
              setSavingLanguage(true);
              void api
                .setLanguage(next)
                .catch(onError)
                .finally(() => setSavingLanguage(false));
            }}
          >
            <option value="en-US">English</option>
            <option value="zh-CN">简体中文</option>
          </select>
        </div>
      </section>
      <section className="shared-dialog-group">
        <h3>{t('scoreSettingsPlayback')}</h3>
        {parts === null ? (
          <p>{t('scoreSettingsLoading')}</p>
        ) : parts.length === 0 ? (
          <p>{t('scoreSettingsEmpty')}</p>
        ) : (
          <table className="score-settings-parts">
            <thead>
              <tr>
                <th scope="col">{t('scoreSettingsPartColumn')}</th>
                <th scope="col">{t('scoreSettingsSoundColumn')}</th>
              </tr>
            </thead>
            <tbody>
              {parts.map((part, index) => (
                <tr key={part.id}>
                  <th scope="row">
                    <label htmlFor={`${controlsId}-${index}`}>
                      {partName(part, index)}
                      {part.name && parts.filter((other) => other.name === part.name).length > 1
                        ? ` ${parts.slice(0, index + 1).filter((other) => other.name === part.name).length}`
                        : ''}
                    </label>
                  </th>
                  <td>
                    <select
                      id={`${controlsId}-${index}`}
                      disabled={saving || !settings.settings}
                      value={settings.settings?.instruments[part.id] ?? 'auto'}
                      onChange={(event) => {
                        const choice = event.currentTarget.value;
                        setSaving(true);
                        void settings
                          .save(part.id, choice === 'auto' ? null : (choice as SampleInstrument))
                          .catch(onError)
                          .finally(() => setSaving(false));
                      }}
                    >
                      <option value="auto">
                        {t('scoreSettingsAuto', { instrument: t(names[part.instrument]) })}
                      </option>
                      {instruments.map((instrument) => (
                        <option key={instrument} value={instrument}>
                          {t(names[instrument])}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </SharedDialog>
  );
}

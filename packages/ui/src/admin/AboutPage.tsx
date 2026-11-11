import mark from '../assets/ossia-mark.svg';
import type { ManagementPreferences, ManagementSnapshot } from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import { InfoRow } from './ui.tsx';

export function AboutPage({
  preferences,
  state,
}: {
  preferences: ManagementPreferences | null;
  state: ManagementSnapshot | null;
}) {
  const { t } = useI18n();
  return (
    <section className="page-section" aria-labelledby="about-title">
      <div className="about-mark">
        <img src={mark} alt="" />
      </div>
      <h1 id="about-title">{t('aboutTitle')}</h1>
      <p className="page-description">{t('aboutBody')}</p>
      <div className="settings-list about-list">
        <InfoRow label={t('appVersion')} value={preferences?.appVersion ?? t('waiting')} />
        <InfoRow
          label={t('runtimeVersion')}
          value={state?.runtime?.runtimeVersion ?? t('notConnected')}
        />
        <InfoRow label={t('dataDirectory')} value={state?.home ?? t('waiting')} code />
        <InfoRow label={t('engineDirectory')} value={state?.engineRoot ?? t('waiting')} code />
      </div>
    </section>
  );
}

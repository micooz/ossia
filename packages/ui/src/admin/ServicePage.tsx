import { Play, RefreshCw, Square, Stethoscope } from 'lucide-react';

import type { DoctorReport, ManagementSnapshot } from '../generated/protocol.ts';
import { useI18n } from '../i18n.tsx';
import { InfoRow } from './ui.tsx';

export function ServicePage({
  native,
  state,
  doctor,
  working,
  confirmStop,
  onAction,
  onRefresh,
  onToggleConfirm,
}: {
  native: boolean;
  state: ManagementSnapshot | null;
  doctor: DoctorReport | null;
  working: boolean;
  confirmStop: boolean;
  onAction: (command: 'management_start' | 'management_stop' | 'management_doctor') => void;
  onRefresh: () => void;
  onToggleConfirm: () => void;
}) {
  const { t } = useI18n();
  return (
    <section className="page-section" aria-labelledby="service-title">
      <div className="page-intro">
        <h1 id="service-title">{t('service')}</h1>
        <p>{t('serviceDescription')}</p>
      </div>
      <div className="settings-list">
        <InfoRow label={t('dataDirectory')} value={state?.home ?? t('waiting')} code />
        <InfoRow label={t('engineDirectory')} value={state?.engineRoot ?? t('waiting')} code />
        <InfoRow
          label={t('runtimeVersion')}
          value={state?.runtime?.runtimeVersion ?? t('notConnected')}
        />
        <InfoRow
          label={t('accessUrl')}
          value={state?.runtime?.webBaseUrl ?? t('notConnected')}
          code
        />
        <InfoRow
          label={t('protocolSchema')}
          value={
            state?.runtime
              ? `${state.runtime.protocolVersion} / ${state.runtime.schemaVersion}`
              : t('notConnected')
          }
        />
      </div>
      <div className="action-row">
        <button
          className="primary"
          type="button"
          disabled={!native || working || !!state?.runtime}
          onClick={() => onAction('management_start')}
        >
          <Play aria-hidden="true" />
          {t('startService')}
        </button>
        <button
          type="button"
          disabled={!native || working || !state?.runtime}
          onClick={onToggleConfirm}
        >
          <Square aria-hidden="true" />
          {t('stopService')}
        </button>
        <button type="button" disabled={!native || working} onClick={onRefresh}>
          <RefreshCw aria-hidden="true" />
          {t('refresh')}
        </button>
      </div>
      <p className="hint">
        {t('closeToTrayNote')} {t('quitStopsRuntime')}
      </p>
      {confirmStop && (
        <div className="confirm-card">
          <strong>{t('stopConfirmTitle')}</strong>
          <p>{t('stopConfirmBody')}</p>
          <div className="action-row">
            <button type="button" onClick={onToggleConfirm}>
              {t('cancel')}
            </button>
            <button
              className="danger"
              type="button"
              disabled={working}
              onClick={() => onAction('management_stop')}
            >
              {t('confirmStop')}
            </button>
          </div>
        </div>
      )}
      <section className="sub-section">
        <div className="section-heading">
          <h2>{t('diagnostics')}</h2>
          <button
            type="button"
            disabled={!native || working}
            onClick={() => onAction('management_doctor')}
          >
            <Stethoscope aria-hidden="true" />
            {working ? t('working') : t('runDoctor')}
          </button>
        </div>
        {doctor && (
          <div className="doctor-result">
            <p>
              {t('engine')}：
              {doctor.engine.ok ? doctor.engine.buildId : doctor.engine.error?.message}
            </p>
            <details>
              <summary>{t('diagnosticDetails')}</summary>
              <pre>{JSON.stringify(doctor, null, 2)}</pre>
            </details>
          </div>
        )}
      </section>
    </section>
  );
}

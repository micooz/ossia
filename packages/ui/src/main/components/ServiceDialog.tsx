import { ExternalLink, LoaderCircle, Play, Square, X } from 'lucide-react';
import type { RefObject } from 'react';

import type { ManagementSnapshot } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { cancelDialog, closeDialog } from '../utils/dialog.ts';
import { InfoRow } from './ui.tsx';

export function ServiceDialog({
  dialogRef,
  native,
  state,
  working,
  confirmStop,
  openingDirectory,
  error,
  onAction,
  onToggleConfirm,
  onOpenDirectory,
  onOpenService,
}: {
  dialogRef: RefObject<HTMLDialogElement | null>;
  native: boolean;
  state: ManagementSnapshot | null;
  working: boolean;
  confirmStop: boolean;
  openingDirectory: boolean;
  error: string;
  onAction: (command: 'management_start' | 'management_stop') => void;
  onToggleConfirm: () => void;
  onOpenDirectory: () => void;
  onOpenService: () => void;
}) {
  const { t } = useI18n();
  const running = !!state?.runtime;
  return (
    <dialog
      ref={dialogRef}
      className="service-dialog"
      aria-labelledby="service-title"
      onCancel={cancelDialog}
    >
      <div className="dialog-heading">
        <h2 id="service-title" tabIndex={-1} autoFocus>
          {t('service')}
        </h2>
        <button
          type="button"
          className="icon-button"
          aria-label={t('close')}
          onClick={() => dialogRef.current && void closeDialog(dialogRef.current)}
        >
          <X />
        </button>
      </div>
      <div className="dialog-content">
        <p className="page-description">{t('serviceDescription')}</p>
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="settings-list">
          <InfoRow
            label={t('serviceStatus')}
            value={state ? t(running ? 'serviceRunning' : 'serviceStopped') : t('waiting')}
          />
          <InfoRow
            label={t('dataDirectory')}
            value={state?.home ?? t('waiting')}
            code
            action={
              <button
                type="button"
                className="icon-button subtle-open"
                aria-label={t('openDirectory')}
                title={t('openDirectory')}
                disabled={!native || !state || openingDirectory}
                onClick={onOpenDirectory}
              >
                <ExternalLink aria-hidden="true" />
              </button>
            }
          />
          <InfoRow
            label={t('accessUrl')}
            value={state?.runtime?.webBaseUrl ?? t('notConnected')}
            code
            action={
              <button
                type="button"
                className="icon-button subtle-open"
                aria-label={t('openServiceAddress')}
                title={t('openServiceAddress')}
                disabled={!native || !running}
                onClick={onOpenService}
              >
                <ExternalLink aria-hidden="true" />
              </button>
            }
          />
        </div>
        {confirmStop && running && (
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
      </div>
      <footer className="dialog-footer action-row">
        <button
          className={running ? '' : 'primary'}
          type="button"
          disabled={!native || working || !state}
          onClick={() => (running ? onToggleConfirm() : onAction('management_start'))}
        >
          {working ? (
            <LoaderCircle className="spinner" aria-hidden="true" />
          ) : running ? (
            <Square aria-hidden="true" />
          ) : (
            <Play aria-hidden="true" />
          )}
          {working ? t('working') : running ? t('stopService') : t('startService')}
        </button>
      </footer>
    </dialog>
  );
}

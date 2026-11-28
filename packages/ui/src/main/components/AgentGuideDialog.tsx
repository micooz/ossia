import { invoke } from '@tauri-apps/api/core';
import { Check, CircleAlert, Copy, RotateCw, X } from 'lucide-react';
import { useRef, useState } from 'react';

import type { AgentManualIntegration } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { copyText } from '../../score/utils/clipboard.ts';
import { cancelDialog, closeDialog, openDialog } from '../utils/dialog.ts';

export function AgentGuideDialog({ native }: { native: boolean }) {
  const { t } = useI18n();
  const dialog = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<AgentManualIntegration | null>(null);
  const [error, setError] = useState(false);
  const [loading, setLoading] = useState(false);
  const [copyStatus, setCopyStatus] = useState<'' | 'success' | 'error'>('');

  async function load() {
    setData(null);
    setError(false);
    setCopyStatus('');
    setLoading(true);
    try {
      setData(await invoke<AgentManualIntegration>('management_agent_manual_integration'));
    } catch {
      setError(true);
    } finally {
      setLoading(false);
    }
  }

  const prompt = data
    ? `${t('agentGuidePrompt')}\n\n${t('agentGuideMcp')}\n\n${JSON.stringify({ command: data.command, args: data.args, ...(Object.keys(data.env).length > 0 && { env: data.env }) })}\n\n${t('agentGuideSkill')}${data.skillPath}\n\n${t('agentGuideVerify')}`
    : '';

  return (
    <>
      <p className="agent-guide-entry">
        {t('agentGuideQuestion')}
        {t('agentGuideLinkPrefix')}
        <button
          type="button"
          className="agent-guide-link"
          disabled={!native}
          onClick={(event) => {
            if (dialog.current) openDialog(dialog.current, event.currentTarget);
            void load();
          }}
        >
          {t('agentGuideLink')}
        </button>
        {t('agentGuideLinkSuffix')}
      </p>
      <dialog
        ref={dialog}
        className="service-dialog agent-guide-dialog"
        aria-labelledby="agent-guide-title"
        onCancel={cancelDialog}
      >
        <div className="dialog-heading">
          <h2 id="agent-guide-title" tabIndex={-1} autoFocus>
            {t('agentGuideTitle')}
          </h2>
          <button
            type="button"
            className="icon-button"
            aria-label={t('close')}
            onClick={() => dialog.current && void closeDialog(dialog.current)}
          >
            <X />
          </button>
        </div>
        <div className="dialog-content">
          {!error && <p className="page-description">{t('agentGuideDescription')}</p>}
          {loading && <p role="status">{t('waiting')}</p>}
          {error && (
            <div className="agent-guide-load-error" role="alert">
              <CircleAlert aria-hidden="true" />
              <p>{t('agentGuideLoadFailed')}</p>
            </div>
          )}
          {data && <pre className="agent-guide-prompt">{prompt}</pre>}
        </div>
        <footer className="dialog-footer action-row">
          <span
            role="status"
            className={copyStatus === 'error' ? 'agent-guide-copy-error' : 'visually-hidden'}
          >
            {copyStatus === 'error' && <CircleAlert aria-hidden="true" />}
            {copyStatus === 'success'
              ? t('agentGuideCopied')
              : copyStatus === 'error'
                ? t('agentGuideCopyFailed')
                : ''}
          </span>
          {error ? (
            <button type="button" onClick={() => void load()}>
              <RotateCw aria-hidden="true" />
              {t('retry')}
            </button>
          ) : (
            <button
              type="button"
              disabled={!data || loading}
              onClick={async () => {
                try {
                  await copyText(prompt);
                  setCopyStatus('success');
                } catch {
                  setCopyStatus('error');
                }
              }}
            >
              {copyStatus === 'success' ? (
                <Check aria-hidden="true" />
              ) : (
                <Copy aria-hidden="true" />
              )}
              {t('agentGuideCopy')}
            </button>
          )}
        </footer>
      </dialog>
    </>
  );
}

import { Check, Copy, X } from 'lucide-react';
import { useEffect, useState } from 'react';

import { useI18n } from '../../i18n.tsx';
import { copyText } from '../utils/clipboard.ts';

export function ScoreAnnotationBadge({
  annotationCount,
  onClear,
}: {
  annotationCount: number;
  onClear: () => void;
}) {
  const { t } = useI18n();
  const [copyStatus, setCopyStatus] = useState<{ kind: 'success' | 'error' } | null>(null);
  useEffect(() => {
    if (!copyStatus) return;
    const timer = window.setTimeout(() => setCopyStatus(null), 2000);
    return () => window.clearTimeout(timer);
  }, [copyStatus]);
  if (annotationCount === 0) {
    return null;
  }
  const summary = t('annotationBadge', { annotations: annotationCount });
  return (
    <div className="score-annotation-badge" role="status" aria-live="polite">
      <span className="score-annotation-badge-text">{summary}</span>
      <span className="score-annotation-badge-copy-wrap">
        <button
          type="button"
          className="score-annotation-badge-action"
          aria-label={t('copyAnnotationCommand')}
          data-tooltip={t('copyAnnotationCommand')}
          onClick={async () => {
            try {
              await copyText(t('annotationCommand'));
              setCopyStatus({ kind: 'success' });
            } catch {
              setCopyStatus({ kind: 'error' });
            }
          }}
        >
          {copyStatus?.kind === 'success' ? (
            <Check aria-hidden="true" size={14} strokeWidth={2.3} />
          ) : (
            <Copy aria-hidden="true" size={14} strokeWidth={2.3} />
          )}
        </button>
        <span className="score-annotation-copy-status" role="status">
          {copyStatus?.kind === 'success'
            ? t('annotationCommandCopied')
            : copyStatus?.kind === 'error'
              ? t('copyAnnotationCommandFailed')
              : ''}
        </span>
      </span>
      <button
        type="button"
        className="score-annotation-badge-action score-annotation-badge-clear"
        aria-label={t('clearAnnotations')}
        data-tooltip={t('clearAnnotations')}
        onClick={onClear}
      >
        <X aria-hidden="true" size={14} strokeWidth={2.3} />
      </button>
    </div>
  );
}

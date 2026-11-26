import { Archive, FileMusic, LoaderCircle, RotateCcw, Trash2 } from 'lucide-react';
import { useEffect, useRef } from 'react';

import type { ManagementScoreSummary } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import type { ScorePreviewView } from '../hooks/useScorePreviews.ts';
import { friendlyScoreTime, type ScoreSortMetric } from '../utils/scoreLibrary.ts';

interface ScoreCardProps {
  score: ManagementScoreSummary;
  archived: boolean;
  timeMetric: ScoreSortMetric;
  preview: ScorePreviewView;
  busy: boolean;
  confirmingDelete: boolean;
  onVisible: (score: ManagementScoreSummary) => void;
  onOpen: (score: ManagementScoreSummary) => void;
  onArchive: (score: ManagementScoreSummary) => void;
  onRestore: (score: ManagementScoreSummary) => void;
  onDelete: (score: ManagementScoreSummary) => void;
  onRetryPreview: (score: ManagementScoreSummary) => void;
  navigableOnly?: boolean;
}

export function ScoreCard({
  score,
  archived,
  timeMetric,
  preview,
  busy,
  confirmingDelete,
  onVisible,
  onOpen,
  onArchive,
  onRestore,
  onDelete,
  onRetryPreview,
  navigableOnly = false,
}: ScoreCardProps) {
  const { language, t } = useI18n();
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    const node = root.current;
    if (!node) return;
    if (!('IntersectionObserver' in window)) {
      onVisible(score);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        onVisible(score);
      },
      { rootMargin: '200px 0px' },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [onVisible, score]);

  const friendly = friendlyScoreTime(
    timeMetric === 'created' ? score.createdAt : score.updatedAt,
    new Date(),
    language,
  );
  const time =
    friendly.kind === 'today'
      ? friendly.time
      : friendly.kind === 'yesterday'
        ? t('yesterday')
        : friendly.kind === 'day-before-yesterday'
          ? t('dayBeforeYesterday')
          : friendly.kind === 'this-week'
            ? t('thisWeek')
            : friendly.date;
  const timeText = t(timeMetric === 'created' ? 'createdOn' : 'updatedOn', { time });
  const title = score.title || score.documentId;

  function previewContent() {
    if (preview.state === 'ready' && preview.dataUrl) {
      return (
        <img
          className="score-preview-image"
          src={preview.dataUrl}
          alt={t('scorePreviewAlt', { title })}
        />
      );
    }
    if (preview.state === 'loading' || preview.state === 'pending') {
      return (
        <span className="score-preview-message" role="status">
          <LoaderCircle className="spinner" aria-hidden="true" />
          <span>{t('previewGenerating')}</span>
        </span>
      );
    }
    if (preview.state === 'failed' && !archived) {
      return (
        <button
          className="score-preview-message is-action"
          type="button"
          title={preview.error}
          onClick={() => onRetryPreview(score)}
        >
          <FileMusic aria-hidden="true" />
          <span>{t('previewFailed')}</span>
          <strong>{t('retryPreview')}</strong>
        </button>
      );
    }
    return (
      <span className="score-preview-message">
        <FileMusic aria-hidden="true" />
        <span>{t('previewUnavailable')}</span>
      </span>
    );
  }

  return (
    <article
      ref={root}
      className={`score-card${archived ? ' is-archived' : ''}`}
      data-score-id={score.documentId}
      onClick={(event) => {
        if (archived) return;
        const target = event.target;
        if (target instanceof Element && target.closest('button')) return;
        onOpen(score);
      }}
    >
      {!archived && (preview.state === 'ready' || preview.state === 'missing') ? (
        <button
          className="score-preview"
          type="button"
          aria-label={t('openScore', { title })}
          onClick={() => onOpen(score)}
        >
          {previewContent()}
        </button>
      ) : (
        <div className="score-preview">{previewContent()}</div>
      )}
      <div className="score-card-info">
        <div className="score-card-copy">
          {archived ? (
            <span className="score-card-title" title={title}>
              {title}
            </span>
          ) : (
            <button
              className="score-card-title"
              type="button"
              title={title}
              aria-label={t('openScore', { title })}
              onClick={() => onOpen(score)}
            >
              {title}
            </button>
          )}
          <p className="score-card-time">{timeText}</p>
        </div>
        {!navigableOnly && (
          <div className="score-card-actions">
            {archived ? (
              <>
                <button
                  className="score-icon-button"
                  type="button"
                  aria-label={t('restoreScore', { title })}
                  data-tooltip={t('restore')}
                  disabled={busy}
                  onClick={() => onRestore(score)}
                >
                  {busy ? (
                    <LoaderCircle className="spinner" aria-hidden="true" />
                  ) : (
                    <RotateCcw aria-hidden="true" />
                  )}
                </button>
                <button
                  className={`score-icon-button score-delete-button${confirmingDelete ? ' is-confirming' : ''}`}
                  type="button"
                  aria-label={t(confirmingDelete ? 'confirmDelete' : 'deletePermanently', {
                    title,
                  })}
                  data-tooltip={t('deletePermanently')}
                  disabled={busy}
                  onClick={() => onDelete(score)}
                >
                  {busy ? (
                    <LoaderCircle className="spinner" aria-hidden="true" />
                  ) : (
                    <Trash2 aria-hidden="true" />
                  )}
                  {confirmingDelete && <span>{t('confirmDelete')}</span>}
                </button>
              </>
            ) : (
              <button
                className="score-icon-button"
                type="button"
                aria-label={t('archiveScore', { title })}
                data-tooltip={t('archive')}
                disabled={busy}
                onClick={() => onArchive(score)}
              >
                {busy ? (
                  <LoaderCircle className="spinner" aria-hidden="true" />
                ) : (
                  <Archive aria-hidden="true" />
                )}
              </button>
            )}
          </div>
        )}
      </div>
    </article>
  );
}

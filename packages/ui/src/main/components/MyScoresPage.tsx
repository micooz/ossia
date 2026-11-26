import { invoke } from '@tauri-apps/api/core';
import {
  Archive,
  FileMusic,
  LayoutGrid,
  List,
  LoaderCircle,
  RefreshCw,
  Search,
  X,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import type { ToastKind } from '../../components/Toast.tsx';
import { describe } from '../../errors.ts';
import type { ManagementScorePage, ManagementScoreSummary } from '../../generated/protocol.ts';
import { useI18n } from '../../i18n.tsx';
import { useScorePreviews } from '../hooks/useScorePreviews.ts';
import {
  filterAndSortScores,
  parseScoreLayout,
  parseScoreSort,
  SCORE_LAYOUT_KEY,
  SCORE_SORT_KEY,
  serializeScoreSort,
  type ScoreLayout,
  type ScoreSort,
  type ScoreSortMetric,
} from '../utils/scoreLibrary.ts';
import { ScoreCard } from './ScoreCard.tsx';
import { Segmented } from './ui.tsx';

interface MyScoresPageProps {
  browser?: boolean;
  native: boolean;
  runtimeRunning: boolean;
  runtimeInstanceId: string | null;
  startingService: boolean;
  notify: (message: string, kind?: ToastKind) => void;
  onStartService: () => void;
}

async function loadAllScores(archived: boolean, browser: boolean) {
  const items: ManagementScoreSummary[] = [];
  let offset = 0;
  for (;;) {
    const page = browser
      ? await fetch(`/api/v1/library/scores?limit=200&offset=${offset}`).then(async (response) => {
          if (!response.ok) throw new Error(`HTTP ${response.status}`);
          return response.json() as Promise<ManagementScorePage>;
        })
      : await invoke<ManagementScorePage>(
          archived ? 'management_archived_scores' : 'management_scores',
          { args: { limit: 200, offset } },
        );
    items.push(...page.items);
    if (page.nextOffset === null) return items;
    offset = page.nextOffset;
  }
}

function directionTitleKey(metric: ScoreSortMetric, direction: ScoreSort['direction']) {
  if (metric === 'created') {
    return direction === 'asc' ? 'sortCreatedAsc' : 'sortCreatedDesc';
  }
  return direction === 'asc' ? 'sortModifiedAsc' : 'sortModifiedDesc';
}

function sameScores(current: ManagementScoreSummary[] | null, next: ManagementScoreSummary[]) {
  return (
    current?.length === next.length &&
    current.every(
      (score, index) =>
        score.documentId === next[index]?.documentId &&
        score.title === next[index]?.title &&
        score.createdAt === next[index]?.createdAt &&
        score.updatedAt === next[index]?.updatedAt &&
        score.archivedAt === next[index]?.archivedAt,
    )
  );
}

function SortArrows({ direction }: { direction: ScoreSort['direction'] | null }) {
  return (
    <svg
      className="sort-arrows"
      data-direction={direction ?? ''}
      viewBox="0 0 20 20"
      width="18"
      height="18"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path data-arrow="asc" d="M6 16V4m-2.5 2.5L6 4l2.5 2.5" />
      <path data-arrow="desc" d="M14 4v12m-2.5-2.5L14 16l2.5-2.5" />
    </svg>
  );
}

export function MyScoresPage({
  browser = false,
  native,
  runtimeRunning,
  runtimeInstanceId,
  startingService,
  notify,
  onStartService,
}: MyScoresPageProps) {
  const { language, t } = useI18n();
  const previews = useScorePreviews(browser);
  const [archived, setArchived] = useState(false);
  const [scores, setScores] = useState<ManagementScoreSummary[] | null>(null);
  const [query, setQuery] = useState('');
  const [layout, setLayout] = useState<ScoreLayout>(() =>
    parseScoreLayout(localStorage.getItem(SCORE_LAYOUT_KEY)),
  );
  const [sort, setSort] = useState<ScoreSort>(() =>
    parseScoreSort(localStorage.getItem(SCORE_SORT_KEY)),
  );
  const [loading, setLoading] = useState(false);
  const [showLoading, setShowLoading] = useState(false);
  const [error, setError] = useState('');
  const [reloadToken, setReloadToken] = useState(0);
  const [busyIds, setBusyIds] = useState<Set<string>>(() => new Set());
  const [openingId, setOpeningId] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState<string | null>(null);

  useEffect(() => {
    localStorage.setItem(SCORE_LAYOUT_KEY, layout);
  }, [layout]);

  useEffect(() => {
    localStorage.setItem(SCORE_SORT_KEY, serializeScoreSort(sort));
  }, [sort]);

  useEffect(() => {
    if (!confirmingDelete) return;
    const timer = window.setTimeout(() => setConfirmingDelete(null), 3000);
    return () => window.clearTimeout(timer);
  }, [confirmingDelete]);

  useEffect(() => {
    setConfirmingDelete(null);
    if ((!native && !browser) || !runtimeRunning) {
      setScores(null);
      setLoading(false);
      setShowLoading(false);
      setError('');
      return;
    }
    let live = true;
    setLoading(true);
    setShowLoading(false);
    setError('');
    const loadingTimer = window.setTimeout(() => setShowLoading(true), 300);
    void loadAllScores(archived, browser)
      .then((items) => {
        if (live) setScores((current) => (sameScores(current, items) ? current : items));
      })
      .catch((caught) => {
        if (!live) return;
        setScores(null);
        setError(describe(caught));
      })
      .finally(() => {
        window.clearTimeout(loadingTimer);
        if (live) {
          setLoading(false);
          setShowLoading(false);
        }
      });
    return () => {
      live = false;
      window.clearTimeout(loadingTimer);
    };
  }, [archived, browser, native, reloadToken, runtimeInstanceId, runtimeRunning]);

  useEffect(() => {
    if ((!native && !browser) || !runtimeRunning || loading) return;
    let live = true;
    let busy = false;
    const timer = window.setInterval(() => {
      if (busy) return;
      busy = true;
      void loadAllScores(archived, browser)
        .then((items) => {
          if (live) {
            setScores((current) => (sameScores(current, items) ? current : items));
          }
        })
        .catch(() => {})
        .finally(() => {
          busy = false;
        });
    }, 2000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [archived, browser, native, runtimeInstanceId, runtimeRunning, loading]);

  const visibleScores = filterAndSortScores(scores ?? [], query, sort, language);

  function setBusy(id: string, busy: boolean) {
    setBusyIds((current) => {
      const next = new Set(current);
      if (busy) next.add(id);
      else next.delete(id);
      return next;
    });
  }

  async function archiveScore(score: ManagementScoreSummary) {
    setBusy(score.documentId, true);
    try {
      await invoke('management_archive_score', { args: { documentId: score.documentId } });
      setScores((current) => current?.filter((item) => item.documentId !== score.documentId) ?? []);
    } catch (caught) {
      notify(`${t('archiveFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setBusy(score.documentId, false);
    }
  }

  async function restoreScore(score: ManagementScoreSummary) {
    setBusy(score.documentId, true);
    setConfirmingDelete(null);
    try {
      await invoke('management_restore_score', { args: { documentId: score.documentId } });
      previews.forget(score.documentId);
      setScores((current) => current?.filter((item) => item.documentId !== score.documentId) ?? []);
    } catch (caught) {
      notify(`${t('restoreFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setBusy(score.documentId, false);
    }
  }

  async function deleteScore(score: ManagementScoreSummary) {
    if (confirmingDelete !== score.documentId) {
      setConfirmingDelete(score.documentId);
      return;
    }
    setConfirmingDelete(null);
    setBusy(score.documentId, true);
    try {
      await invoke('management_delete_score', { args: { documentId: score.documentId } });
      previews.forget(score.documentId);
      setScores((current) => current?.filter((item) => item.documentId !== score.documentId) ?? []);
    } catch (caught) {
      notify(`${t('deleteFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setBusy(score.documentId, false);
    }
  }

  async function openScore(score: ManagementScoreSummary) {
    if (browser) {
      window.location.assign(`/scores?${new URLSearchParams({ documentId: score.documentId })}`);
      return;
    }
    if (openingId) return;
    setOpeningId(score.documentId);
    try {
      await invoke('management_open_score', { args: { documentId: score.documentId } });
    } catch (caught) {
      notify(`${t('openScoreFailed')}: ${describe(caught)}`, 'error');
    } finally {
      setOpeningId(null);
    }
  }

  function chooseSort(metric: ScoreSortMetric) {
    setSort((current) =>
      current.metric === metric
        ? { metric, direction: current.direction === 'desc' ? 'asc' : 'desc' }
        : { metric, direction: 'desc' },
    );
  }

  function sortOption(metric: ScoreSortMetric) {
    const active = sort.metric === metric;
    const direction = active ? sort.direction : null;
    const label = t(metric === 'created' ? 'sortCreated' : 'sortModified');
    const title = t(
      direction
        ? directionTitleKey(metric, direction)
        : metric === 'created'
          ? 'sortByCreated'
          : 'sortByModified',
    );
    return {
      value: metric,
      label: (
        <span className="score-sort-label">
          <SortArrows direction={direction} />
          <span>{label}</span>
        </span>
      ),
      ariaLabel: title,
      title,
    };
  }

  const filtered = query.trim().length > 0;

  return (
    <section className="scores-page" aria-busy={loading}>
      <div className="scores-heading">
        <div className="page-intro">
          <h1>{t('myScores')}</h1>
          <p>{t(browser ? 'browseScoresDescription' : 'myScoresDescription')}</p>
        </div>
      </div>

      <div className="scores-toolbar">
        <label className="score-search">
          <Search aria-hidden="true" />
          <span className="visually-hidden">{t('searchScores')}</span>
          <input
            type="search"
            value={query}
            placeholder={t('searchScores')}
            onChange={(event) => setQuery(event.target.value)}
          />
          {query && (
            <button
              type="button"
              aria-label={t('clearSearch')}
              title={t('clearSearch')}
              onClick={() => setQuery('')}
            >
              <X aria-hidden="true" />
            </button>
          )}
        </label>
        <div className="score-view-controls">
          <Segmented
            ariaLabel={t('scoreSort')}
            value={sort.metric}
            options={[sortOption('created'), sortOption('updated')]}
            onChange={(value) => {
              if (value !== 'created' && value !== 'updated') return;
              chooseSort(value);
            }}
          />
          <Segmented
            ariaLabel={t('scoreLayout')}
            value={layout}
            compact
            options={[
              {
                value: 'grid',
                label: <LayoutGrid aria-hidden="true" />,
                ariaLabel: t('gridLayout'),
                title: t('gridLayout'),
              },
              {
                value: 'list',
                label: <List aria-hidden="true" />,
                ariaLabel: t('listLayout'),
                title: t('listLayout'),
              },
            ]}
            onChange={(value) => {
              if (value === 'grid' || value === 'list') setLayout(value);
            }}
          />
          {!browser && (
            <button
              className={`archived-filter${archived ? ' is-active' : ''}`}
              type="button"
              aria-pressed={archived}
              onClick={() => setArchived((current) => !current)}
            >
              <Archive aria-hidden="true" />
              <span>{t('archived')}</span>
            </button>
          )}
        </div>
      </div>

      {(!native && !browser) || !runtimeRunning ? (
        <div className="scores-state">
          <RefreshCw aria-hidden="true" />
          <h2>{t('libraryServiceRequired')}</h2>
          <p>{t('libraryServiceRequiredHint')}</p>
          {native && (
            <button
              className="score-primary-action"
              type="button"
              disabled={startingService}
              onClick={onStartService}
            >
              {startingService && <LoaderCircle className="spinner" aria-hidden="true" />}
              {t('startService')}
            </button>
          )}
        </div>
      ) : loading || (scores === null && !error) ? (
        showLoading ? (
          <div className="scores-state" role="status">
            <LoaderCircle className="spinner" aria-hidden="true" />
            <h2>{t('loadingScores')}</h2>
          </div>
        ) : null
      ) : error ? (
        <div className="scores-state is-error">
          <FileMusic aria-hidden="true" />
          <h2>{t('scoresLoadFailed')}</h2>
          <p title={error}>{error}</p>
          <button
            className="score-primary-action"
            type="button"
            onClick={() => setReloadToken((value) => value + 1)}
          >
            <RefreshCw aria-hidden="true" />
            {t('retry')}
          </button>
        </div>
      ) : visibleScores.length === 0 ? (
        <div className="scores-state">
          <FileMusic aria-hidden="true" />
          <h2>{t(archived ? 'noArchivedScores' : filtered ? 'noMatchingScores' : 'noScores')}</h2>
          <p>
            {t(
              archived
                ? 'noArchivedScoresHint'
                : filtered
                  ? 'noMatchingScoresHint'
                  : 'noScoresHint',
            )}
          </p>
        </div>
      ) : (
        <div className={`score-collection is-${layout}`}>
          {visibleScores.map((score) => (
            <ScoreCard
              key={score.documentId}
              score={score}
              navigableOnly={browser}
              archived={archived}
              timeMetric={sort.metric}
              preview={previews.view(score)}
              busy={busyIds.has(score.documentId) || openingId === score.documentId}
              confirmingDelete={confirmingDelete === score.documentId}
              onVisible={previews.request}
              onOpen={(item) => void openScore(item)}
              onArchive={(item) => void archiveScore(item)}
              onRestore={(item) => void restoreScore(item)}
              onDelete={(item) => void deleteScore(item)}
              onRetryPreview={previews.retry}
            />
          ))}
        </div>
      )}
    </section>
  );
}

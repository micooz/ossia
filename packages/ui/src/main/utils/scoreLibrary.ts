import type { ManagementScoreSummary } from '../../generated/protocol.ts';

export type ScoreLayout = 'grid' | 'list';
export type ScoreSortMetric = 'created' | 'updated';
export type ScoreSortDirection = 'asc' | 'desc';

export interface ScoreSort {
  metric: ScoreSortMetric;
  direction: ScoreSortDirection;
}

export type FriendlyScoreTime =
  | { kind: 'today'; time: string }
  | { kind: 'yesterday' }
  | { kind: 'day-before-yesterday' }
  | { kind: 'this-week' }
  | { kind: 'date'; date: string };

export const SCORE_LAYOUT_KEY = 'ossia:management:scores-layout';
export const SCORE_SORT_KEY = 'ossia:management:scores-sort';
export const DEFAULT_SCORE_LAYOUT: ScoreLayout = 'grid';
export const DEFAULT_SCORE_SORT: ScoreSort = { metric: 'updated', direction: 'desc' };

export function parseScoreLayout(value: string | null): ScoreLayout {
  return value === 'grid' || value === 'list' ? value : DEFAULT_SCORE_LAYOUT;
}

export function parseScoreSort(value: string | null): ScoreSort {
  if (value === 'created-asc' || value === 'created-desc') {
    return { metric: 'created', direction: value.endsWith('asc') ? 'asc' : 'desc' };
  }
  if (value === 'updated-asc' || value === 'updated-desc') {
    return { metric: 'updated', direction: value.endsWith('asc') ? 'asc' : 'desc' };
  }
  return DEFAULT_SCORE_SORT;
}

export function serializeScoreSort(sort: ScoreSort) {
  return `${sort.metric}-${sort.direction}`;
}

function normalized(value: string) {
  return value.normalize('NFKC').toLocaleLowerCase();
}

export function filterAndSortScores(
  scores: ManagementScoreSummary[],
  query: string,
  sort: ScoreSort,
  locale: string,
) {
  const needle = normalized(query.trim());
  const filtered = needle
    ? scores.filter((score) => normalized(score.title).includes(needle))
    : scores.slice();
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: 'base' });
  const multiplier = sort.direction === 'asc' ? 1 : -1;
  return filtered.sort((left, right) => {
    const leftTime = Date.parse(sort.metric === 'created' ? left.createdAt : left.updatedAt);
    const rightTime = Date.parse(sort.metric === 'created' ? right.createdAt : right.updatedAt);
    if (leftTime !== rightTime) return (leftTime - rightTime) * multiplier;
    return (
      collator.compare(left.title, right.title) || left.documentId.localeCompare(right.documentId)
    );
  });
}

function localDayNumber(date: Date) {
  return Math.floor(
    Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / (24 * 60 * 60 * 1000),
  );
}

export function friendlyScoreTime(value: string, now: Date, locale: string): FriendlyScoreTime {
  const date = new Date(value);
  const day = localDayNumber(date);
  const today = localDayNumber(now);
  const difference = today - day;
  if (difference === 0) {
    return {
      kind: 'today',
      time: new Intl.DateTimeFormat(locale, {
        hour: '2-digit',
        minute: '2-digit',
        hourCycle: 'h23',
      }).format(date),
    };
  }
  if (difference === 1) return { kind: 'yesterday' };
  if (difference === 2) return { kind: 'day-before-yesterday' };
  const mondayOffset = (now.getDay() + 6) % 7;
  const startOfWeek = today - mondayOffset;
  if (difference > 2 && day >= startOfWeek) return { kind: 'this-week' };
  return {
    kind: 'date',
    date: new Intl.DateTimeFormat(locale, { month: 'short', day: 'numeric' }).format(date),
  };
}

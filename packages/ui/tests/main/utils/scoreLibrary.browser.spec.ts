import { expect, test } from '@playwright/test';

import type { ManagementScoreSummary } from '../../../src/generated/protocol.ts';
import {
  DEFAULT_SCORE_LAYOUT,
  DEFAULT_SCORE_SORT,
  filterAndSortScores,
  friendlyScoreTime,
  parseScoreLayout,
  parseScoreSort,
  serializeScoreSort,
} from '../../../src/main/utils/scoreLibrary.ts';

function score(
  documentId: string,
  title: string,
  createdAt: Date,
  updatedAt: Date,
): ManagementScoreSummary {
  return {
    documentId,
    title,
    createdAt: createdAt.toISOString(),
    updatedAt: updatedAt.toISOString(),
    archivedAt: null,
  };
}

function localDate(year: number, month: number, day: number, hour = 12, minute = 0) {
  return new Date(year, month - 1, day, hour, minute);
}

test('score search and sort use localized titles and stable timestamp ordering', () => {
  const scores = [
    score(
      '00000000-0000-4000-8000-000000000001',
      'Etude',
      localDate(2026, 1, 2),
      localDate(2026, 3, 4),
    ),
    score(
      '00000000-0000-4000-8000-000000000002',
      '前奏曲',
      localDate(2026, 2, 3),
      localDate(2026, 2, 20),
    ),
    score(
      '00000000-0000-4000-8000-000000000003',
      'étude',
      localDate(2026, 1, 2),
      localDate(2026, 4, 1),
    ),
  ];

  expect(
    filterAndSortScores(scores, 'etude', { metric: 'created', direction: 'asc' }, 'en-US').map(
      (item) => item.title,
    ),
  ).toEqual(['Etude']);
  expect(
    filterAndSortScores(scores, 'ÉTUDE', { metric: 'created', direction: 'asc' }, 'en-US').map(
      (item) => item.title,
    ),
  ).toEqual(['étude']);
  expect(
    filterAndSortScores(scores, '', { metric: 'updated', direction: 'desc' }, 'zh-CN').map(
      (item) => item.title,
    ),
  ).toEqual(['étude', 'Etude', '前奏曲']);
  expect(
    filterAndSortScores(scores, '', { metric: 'created', direction: 'asc' }, 'zh-CN').map(
      (item) => item.documentId,
    ),
  ).toEqual([
    '00000000-0000-4000-8000-000000000001',
    '00000000-0000-4000-8000-000000000003',
    '00000000-0000-4000-8000-000000000002',
  ]);
});

test('friendly score time follows today, yesterday, this week and localized dates', () => {
  const now = localDate(2026, 11, 15, 14, 30);
  expect(friendlyScoreTime(localDate(2026, 11, 15, 9, 5).toISOString(), now, 'en-US')).toEqual({
    kind: 'today',
    time: '09:05',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 14, 23, 59).toISOString(), now, 'en-US')).toEqual({
    kind: 'yesterday',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 13, 8).toISOString(), now, 'en-US')).toEqual({
    kind: 'day-before-yesterday',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 10).toISOString(), now, 'en-US')).toEqual({
    kind: 'this-week',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 8).toISOString(), now, 'en-US')).toEqual({
    kind: 'date',
    date: 'Nov 8',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 8).toISOString(), now, 'zh-CN')).toEqual({
    kind: 'date',
    date: '11月8日',
  });
  expect(friendlyScoreTime(localDate(2026, 11, 16).toISOString(), now, 'en-US')).toEqual({
    kind: 'date',
    date: 'Nov 16',
  });
});

test('stored score layout and sort values validate to defaults', () => {
  expect(parseScoreLayout('list')).toBe('list');
  expect(parseScoreLayout('invalid')).toBe(DEFAULT_SCORE_LAYOUT);
  expect(parseScoreSort('created-asc')).toEqual({ metric: 'created', direction: 'asc' });
  expect(parseScoreSort('bad')).toEqual(DEFAULT_SCORE_SORT);
  expect(serializeScoreSort({ metric: 'updated', direction: 'asc' })).toBe('updated-asc');
});

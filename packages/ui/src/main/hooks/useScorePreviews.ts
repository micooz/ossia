import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useRef, useState } from 'react';

import { describe } from '../../errors.ts';
import type {
  ManagementScorePreview,
  ManagementScorePreviewArgs,
  ManagementScoreSummary,
} from '../../generated/protocol.ts';

const PREVIEW_CONCURRENCY = 4;
const PREVIEW_CACHE_LIMIT = 200;
const PREVIEW_POLL_MS = 750;
const cachedViews = new Map<string, ScorePreviewView>();

export interface ScorePreviewView {
  state: 'idle' | 'loading' | 'pending' | 'ready' | 'failed' | 'missing';
  dataUrl?: string;
  error?: string;
}

interface PreviewTask {
  key: string;
  documentId: string;
  retry: boolean;
}

function rememberView(key: string, view: ScorePreviewView) {
  cachedViews.delete(key);
  cachedViews.set(key, view);
  while (cachedViews.size > PREVIEW_CACHE_LIMIT) {
    const oldest = cachedViews.keys().next().value;
    if (oldest === undefined) break;
    cachedViews.delete(oldest);
  }
}

export function scorePreviewKey(score: ManagementScoreSummary) {
  return `${score.documentId}:${score.updatedAt}`;
}

export function useScorePreviews(browser = false) {
  const [views, setViews] = useState<Record<string, ScorePreviewView>>(() =>
    Object.fromEntries(cachedViews),
  );
  const requested = useRef(new Map<string, ManagementScoreSummary>());
  const queue = useRef<PreviewTask[]>([]);
  const active = useRef(0);
  const live = useRef(true);
  const timers = useRef(new Map<number, string>());

  const update = useCallback((key: string, view: ScorePreviewView) => {
    rememberView(key, view);
    if (live.current) setViews((current) => ({ ...current, [key]: view }));
  }, []);

  function clearTimer(key: string) {
    for (const [timer, timerKey] of timers.current) {
      if (timerKey !== key) continue;
      window.clearTimeout(timer);
      timers.current.delete(timer);
    }
  }

  function finish(task: PreviewTask, view: ScorePreviewView) {
    update(task.key, view);
    active.current = Math.max(0, active.current - 1);
    pump();
  }

  function schedulePoll(task: PreviewTask) {
    const timer = window.setTimeout(() => {
      timers.current.delete(timer);
      if (!live.current || !requested.current.has(task.key)) return;
      void runPreview({ ...task, retry: false });
    }, PREVIEW_POLL_MS);
    timers.current.set(timer, task.key);
  }

  function applyOutcome(task: PreviewTask, outcome: ManagementScorePreview) {
    if (!live.current || !requested.current.has(task.key)) return;
    if (outcome.state === 'ready') {
      clearTimer(task.key);
      finish(task, { state: 'ready', dataUrl: outcome.dataUrl });
    } else if (outcome.state === 'pending') {
      update(task.key, { state: 'pending' });
      schedulePoll(task);
    } else if (outcome.state === 'failed') {
      clearTimer(task.key);
      finish(task, { state: 'failed', error: describe(outcome.error) });
    } else {
      clearTimer(task.key);
      finish(task, { state: 'missing' });
    }
  }

  async function runPreview(task: PreviewTask) {
    if (!live.current || !requested.current.has(task.key)) return;
    try {
      const args = {
        documentId: task.documentId,
        retry: task.retry,
      } satisfies ManagementScorePreviewArgs;
      const outcome = browser
        ? await fetch(`/api/v1/library/scores/${encodeURIComponent(task.documentId)}/preview`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ retry: task.retry }),
          }).then(async (response) => {
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            return response.json() as Promise<ManagementScorePreview>;
          })
        : await invoke<ManagementScorePreview>('management_score_preview', { args });
      applyOutcome(task, outcome);
    } catch (error) {
      clearTimer(task.key);
      finish(task, { state: 'failed', error: describe(error) });
    }
  }

  function pump() {
    while (live.current && active.current < PREVIEW_CONCURRENCY && queue.current.length > 0) {
      const task = queue.current.shift();
      if (!task || !requested.current.has(task.key)) continue;
      active.current += 1;
      void runPreview(task);
    }
  }

  const request = useCallback(
    (score: ManagementScoreSummary) => {
      const key = scorePreviewKey(score);
      const cached = cachedViews.get(key)?.state;
      if (cached && cached !== 'loading' && cached !== 'pending') return;
      if (requested.current.has(key)) return;
      requested.current.set(key, score);
      if (!cached) update(key, { state: 'loading' });
      queue.current.push({ key, documentId: score.documentId, retry: false });
      pump();
    },
    [update],
  );

  const retry = useCallback(
    (score: ManagementScoreSummary) => {
      const key = scorePreviewKey(score);
      clearTimer(key);
      cachedViews.delete(key);
      requested.current.set(key, score);
      update(key, { state: 'loading' });
      queue.current = queue.current.filter((task) => task.key !== key);
      queue.current.push({ key, documentId: score.documentId, retry: true });
      pump();
    },
    [update],
  );

  const forget = useCallback((documentId: string) => {
    for (const key of requested.current.keys()) {
      if (!key.startsWith(`${documentId}:`)) continue;
      clearTimer(key);
      cachedViews.delete(key);
      requested.current.delete(key);
      setViews((current) => {
        const next = { ...current };
        delete next[key];
        return next;
      });
    }
    queue.current = queue.current.filter((task) => !task.key.startsWith(`${documentId}:`));
  }, []);

  useEffect(() => {
    return () => {
      live.current = false;
      for (const timer of timers.current.keys()) window.clearTimeout(timer);
      timers.current.clear();
      queue.current = [];
    };
  }, []);

  return {
    request,
    retry,
    forget,
    view(score: ManagementScoreSummary): ScorePreviewView {
      return views[scorePreviewKey(score)] ?? { state: 'idle' };
    },
  };
}

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  PlaybackNoteGroup,
  RenderReference,
  ScorePlayback,
  ScoreSettings,
} from '../../generated/protocol.ts';
import { scoreAudio } from '../scoreAudio.ts';
import { ApiError } from '../utils/api.ts';
import type { Api } from '../utils/api.ts';
import { scheduleWindow } from '../utils/playbackSchedule.ts';
import {
  auditionDuration,
  adjacentAuditionGroup,
  auditionMembers,
  auditionSounds,
  combinedAuditionGroup,
  groupAt,
  measureSpanAt,
} from '../utils/playbackTimeline.ts';
import { loadScorePlayback, peekScorePlayback } from '../utils/renderCache.ts';

export type PlaybackPhase = 'idle' | 'loading' | 'playing' | 'paused' | 'unavailable';

interface PlaybackState {
  phase: PlaybackPhase;
  available: boolean;
  timeline: ScorePlayback | null;
  position: number;
  activeGroup: PlaybackNoteGroup | null;
  playheadGroup: PlaybackNoteGroup | null;
  playhead: boolean;
}

const EMPTY_STATE: PlaybackState = {
  phase: 'idle',
  available: false,
  timeline: null,
  position: 0,
  activeGroup: null,
  playheadGroup: null,
  playhead: false,
};

const LOOKAHEAD_SECONDS = 0.18;
const START_LEAD_SECONDS = 0.03;

/** Own score playback timing, audio scheduling, audition state and the playhead. */
export function useScorePlayback(
  api: Api,
  latestRender: RenderReference | null,
  viewKey: string,
  enabled: boolean,
  onError: (error: unknown) => void,
  settings: ScoreSettings | null,
  selectedMeasure?: number,
) {
  const [state, setState] = useState<PlaybackState>(EMPTY_STATE);
  const [session, setSession] = useState<{ viewKey: string; render: RenderReference } | null>(null);
  const [unavailableReason, setUnavailableReason] = useState<string | null>(null);
  // A pause keeps the same render so the playhead and timeline stay aligned when resuming.
  const render =
    enabled &&
    session?.viewKey === viewKey &&
    session.render.manifest.sourceHash === latestRender?.manifest.sourceHash
      ? session.render
      : latestRender;
  const timelineRef = useRef<ScorePlayback | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const phaseRef = useRef<PlaybackPhase>('idle');
  const positionRef = useRef(0);
  // Audio scheduling starts slightly early; the playhead must never paint before the selected note.
  const positionFloorRef = useRef(0);
  const anchorRef = useRef(0);
  const scheduledIndexRef = useRef(0);
  const auditionRef = useRef<number | null>(null);
  const auditionGroupRef = useRef<PlaybackNoteGroup | null>(null);
  const playheadGroupRef = useRef<PlaybackNoteGroup | null>(null);
  const resumeReadyRef = useRef(false);
  const timerRef = useRef<number | null>(null);
  const frameRef = useRef<number | null>(null);
  const starting = useRef<Promise<void> | null>(null);
  const generation = useRef(0);
  const seekRef = useRef<Promise<void> | null>(null);
  const seekTargetRef = useRef<number | null>(null);

  const setPhase = useCallback((phase: PlaybackPhase) => {
    phaseRef.current = phase;
    setState((current) => ({ ...current, phase }));
  }, []);

  const publishPosition = useCallback((position: number) => {
    positionRef.current = position;
    const timeline = timelineRef.current;
    if (!timeline) {
      setState((current) => ({ ...current, position, playhead: false }));
      return;
    }
    const playheadGroup = groupAt(timeline, position) ?? playheadGroupRef.current;
    playheadGroupRef.current = playheadGroup;
    setState((current) => ({
      ...current,
      position,
      playheadGroup,
      playhead: true,
    }));
  }, []);

  const clearScheduler = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    if (frameRef.current !== null) {
      window.cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
  }, []);

  const finish = useCallback(() => {
    clearScheduler();
    scoreAudio.stopAll();
    playheadGroupRef.current = null;
    positionRef.current = 0;
    positionFloorRef.current = 0;
    scheduledIndexRef.current = 0;
    seekRef.current = null;
    seekTargetRef.current = null;
    resumeReadyRef.current = false;
    setSession(null);
    setState((current) => ({
      ...current,
      phase: 'idle',
      position: 0,
      playheadGroup: null,
      playhead: false,
    }));
    phaseRef.current = 'idle';
  }, [clearScheduler]);

  const schedule = useCallback(() => {
    const timeline = timelineRef.current;
    if (!timeline || phaseRef.current !== 'playing') return;
    const now = scoreAudio.now();
    const position = now - anchorRef.current;
    if (position >= timeline.durationSeconds + 0.02) {
      finish();
      return;
    }
    const horizon = position + LOOKAHEAD_SECONDS;
    const batch = scheduleWindow(timeline.notes, scheduledIndexRef.current, position, horizon);
    for (const note of batch.notes) {
      void scoreAudio
        .play(
          [note.pitch],
          anchorRef.current + note.startSeconds,
          note.ornamentAnchor ? note.durationSeconds : Math.max(0.05, note.durationSeconds),
          settingsRef.current?.instruments[note.partId] ?? note.instrument,
          !!note.ornamentAnchor,
        )
        .catch(onError);
    }
    scheduledIndexRef.current = batch.nextIndex;
    timerRef.current = window.setTimeout(schedule, 60);
  }, [finish, onError]);

  const tick = useCallback(() => {
    if (phaseRef.current !== 'playing') return;
    const position = Math.max(positionFloorRef.current, scoreAudio.now() - anchorRef.current);
    publishPosition(position);
    frameRef.current = window.requestAnimationFrame(tick);
  }, [publishPosition]);

  const beginAt = useCallback(
    async (offset: number, keepPlaying = true) => {
      const timeline = timelineRef.current;
      if (!timeline || !enabled) return;
      const run = generation.current;
      const instruments = JSON.stringify(settingsRef.current?.instruments ?? {});
      clearScheduler();
      scoreAudio.stopAll();
      await scoreAudio.resume();
      if (run !== generation.current) return;
      if (instruments !== JSON.stringify(settingsRef.current?.instruments ?? {})) {
        setPhase('paused');
        return;
      }
      const safeOffset = Math.min(
        Math.max(0, Number.isFinite(offset) ? offset : 0),
        timeline.durationSeconds,
      );
      anchorRef.current = scoreAudio.now() + START_LEAD_SECONDS - safeOffset;
      positionFloorRef.current = safeOffset;
      const index = timeline.notes.findIndex((note) => note.startSeconds >= safeOffset - 0.0001);
      scheduledIndexRef.current = index < 0 ? timeline.notes.length : index;
      resumeReadyRef.current = false;
      publishPosition(safeOffset);
      if (!keepPlaying) return;
      setPhase('playing');
      schedule();
      tick();
    },
    [clearScheduler, enabled, publishPosition, schedule, setPhase, tick],
  );

  const play = useCallback(async () => {
    if (starting.current) return starting.current;
    const timeline = timelineRef.current;
    if (!timeline || !enabled || !render) return;
    const run = generation.current;
    setSession({ viewKey, render });
    const playing = (async () => {
      try {
        const wasPaused = phaseRef.current === 'paused';
        const canResume = resumeReadyRef.current;
        const selectedSpan =
          selectedMeasure === undefined
            ? undefined
            : timeline.systems.find(
                (span) =>
                  span.measureStart <= selectedMeasure && span.measureEnd >= selectedMeasure,
              );
        const fallbackOffset = wasPaused
          ? positionRef.current
          : (auditionRef.current ?? selectedSpan?.startSeconds ?? 0);
        const instruments = JSON.stringify(settingsRef.current?.instruments ?? {});
        setPhase('loading');
        await scoreAudio.load(
          timeline.notes.map(
            (note) => settingsRef.current?.instruments[note.partId] ?? note.instrument,
          ),
        );
        if (run !== generation.current) return;
        if (instruments !== JSON.stringify(settingsRef.current?.instruments ?? {})) {
          publishPosition(fallbackOffset);
          setPhase('paused');
          return;
        }
        if (wasPaused && canResume) {
          await scoreAudio.resume();
          if (run !== generation.current) return;
          setPhase('playing');
          schedule();
          tick();
        } else {
          await beginAt(fallbackOffset);
        }
      } catch (error) {
        if (run !== generation.current) return;
        setSession(null);
        setPhase('idle');
        onError(error);
      }
    })();
    const tracked = playing.finally(() => {
      if (starting.current === tracked) starting.current = null;
    });
    starting.current = tracked;
    return tracked;
  }, [
    beginAt,
    enabled,
    onError,
    publishPosition,
    render,
    schedule,
    selectedMeasure,
    setPhase,
    tick,
    viewKey,
  ]);

  const pause = useCallback(async () => {
    if (phaseRef.current !== 'playing') return;
    const position = Math.max(positionFloorRef.current, scoreAudio.now() - anchorRef.current);
    clearScheduler();
    publishPosition(position);
    resumeReadyRef.current = true;
    setPhase('paused');
    await scoreAudio.suspend().catch(onError);
  }, [clearScheduler, onError, publishPosition, setPhase]);

  const instrumentSettingsKey = JSON.stringify(settings?.instruments ?? {});
  const previousSettingsKey = useRef(instrumentSettingsKey);
  useEffect(() => {
    if (previousSettingsKey.current === instrumentSettingsKey) return;
    previousSettingsKey.current = instrumentSettingsKey;
    if (phaseRef.current === 'playing') {
      void pause();
      scoreAudio.stopAll();
      resumeReadyRef.current = false;
    } else {
      scoreAudio.stopAll();
      resumeReadyRef.current = false;
    }
  }, [instrumentSettingsKey, pause]);

  const clearAuditionHighlight = useCallback(() => {
    setState((current) => (current.activeGroup ? { ...current, activeGroup: null } : current));
  }, []);

  const cueMeasure = useCallback(
    (page: number, measure: number) => {
      if (!enabled || phaseRef.current === 'loading') return;
      const span = timelineRef.current?.systems.find(
        (candidate) =>
          candidate.page === page &&
          candidate.measureStart <= measure &&
          candidate.measureEnd >= measure,
      );
      if (!span) return;
      auditionRef.current = span.startSeconds;
      auditionGroupRef.current = null;
      playheadGroupRef.current = null;
      clearAuditionHighlight();
      if (phaseRef.current === 'playing') {
        void beginAt(span.startSeconds).catch(onError);
      } else {
        resumeReadyRef.current = false;
        scoreAudio.stopAll();
        publishPosition(span.startSeconds);
      }
    },
    [beginAt, clearAuditionHighlight, enabled, onError, publishPosition],
  );

  const toggle = useCallback(async () => {
    if (!timelineRef.current || !enabled) return;
    if (phaseRef.current === 'playing') {
      await pause();
      return;
    }
    if (phaseRef.current === 'loading') return;
    await play();
  }, [enabled, pause, play]);

  const audition = useCallback(
    async (
      group: PlaybackNoteGroup,
      options?: { simultaneous?: boolean; highlightElementId?: string },
    ) => {
      if (!enabled) return;
      const run = generation.current;
      const members = auditionMembers(timelineRef.current, group, options?.simultaneous ?? false);
      const combined = combinedAuditionGroup(members, group);
      const activeGroup =
        options?.highlightElementId && !options.simultaneous
          ? { ...combined, elementIds: [options.highlightElementId] }
          : combined;
      auditionGroupRef.current = group;
      setState((current) => ({ ...current, activeGroup }));
      const start = group.startSeconds;
      if (start !== null) {
        auditionRef.current = start;
        if (phaseRef.current === 'playing') {
          await beginAt(start);
          return;
        }
        resumeReadyRef.current = false;
        scoreAudio.stopAll();
        positionRef.current = start;
      } else if (phaseRef.current === 'paused') {
        resumeReadyRef.current = false;
        scoreAudio.stopAll();
      }
      const sounds = auditionSounds(members);
      if (sounds.length) {
        try {
          const instruments = sounds.map((member) => {
            const note = timelineRef.current?.notes.find((candidate) => {
              const start = member.startSeconds;
              if (candidate.partId !== member.partId || start === null) return false;
              if (candidate.ornamentAnchor) {
                return (
                  member.elementIds.includes(candidate.ornamentAnchor) &&
                  candidate.startSeconds >= start - 0.001 &&
                  candidate.startSeconds <= start + (member.durationSeconds ?? 0) + 0.001
                );
              }
              return (
                member.pitches.includes(candidate.pitch) &&
                Math.abs(candidate.startSeconds - start) < 0.001
              );
            });
            return (
              (member.partId && settingsRef.current?.instruments[member.partId]) ||
              note?.instrument ||
              'piano'
            );
          });
          await scoreAudio.load(instruments);
          if (run !== generation.current) return;
          await scoreAudio.resume();
          if (run !== generation.current) return;
          const now = scoreAudio.now();
          await Promise.all(
            sounds.map((member, index) =>
              scoreAudio.play(member.pitches, now, auditionDuration(member), instruments[index]!),
            ),
          );
        } catch (error) {
          if (run === generation.current) onError(error);
        }
      }
    },
    [beginAt, enabled, onError],
  );

  const auditionAdjacent = useCallback(
    (direction: 'previous' | 'next', options?: { simultaneous?: boolean }) => {
      const timeline = timelineRef.current;
      if (!enabled || !timeline || !timeline.groups.length) return false;
      const simultaneous = options?.simultaneous ?? false;
      const next = adjacentAuditionGroup(
        timeline,
        auditionGroupRef.current,
        direction,
        simultaneous,
      );
      if (next) void audition(next, { simultaneous });
      return true;
    },
    [audition, enabled],
  );

  const seekMeasure = useCallback(
    (direction: 'previous' | 'next') => {
      const timeline = timelineRef.current;
      if (!enabled || !timeline || phaseRef.current !== 'playing') return false;
      const requested =
        seekTargetRef.current ??
        Math.max(positionFloorRef.current, scoreAudio.now() - anchorRef.current);
      const current = measureSpanAt(timeline, requested);
      if (!current) return false;
      const index = timeline.systems.indexOf(current);
      const target = timeline.systems[index + (direction === 'next' ? 1 : -1)];
      if (!target) return true;
      seekTargetRef.current = target.startSeconds;
      const previous = seekRef.current ?? Promise.resolve();
      const run = generation.current;
      let tracked: Promise<void>;
      tracked = previous
        .catch(() => {})
        .then(() => {
          if (run === generation.current) return beginAt(target.startSeconds);
        })
        .catch(onError)
        .finally(() => {
          if (run !== generation.current) return;
          if (seekTargetRef.current === target.startSeconds) seekTargetRef.current = null;
          if (seekRef.current === tracked) seekRef.current = null;
        });
      seekRef.current = tracked;
      return true;
    },
    [beginAt, enabled, onError],
  );

  const stop = useCallback(() => {
    generation.current++;
    starting.current = null;
    clearScheduler();
    scoreAudio.stopAll();
    timelineRef.current = null;
    auditionRef.current = null;
    auditionGroupRef.current = null;
    playheadGroupRef.current = null;
    positionRef.current = 0;
    positionFloorRef.current = 0;
    scheduledIndexRef.current = 0;
    seekRef.current = null;
    seekTargetRef.current = null;
    resumeReadyRef.current = false;
    phaseRef.current = 'idle';
    setSession(null);
    setState(EMPTY_STATE);
  }, [clearScheduler]);

  useEffect(() => () => stop(), [stop]);

  const renderKey = render?.renderKey ?? null;
  useEffect(() => {
    setUnavailableReason(null);
    if (!enabled || !renderKey) {
      stop();
      setState((current) => ({ ...current, available: false, phase: 'unavailable' }));
      return;
    }
    let live = true;
    stop();
    const cached = peekScorePlayback(renderKey);
    if (cached) {
      timelineRef.current = cached;
      setState({
        phase: 'idle',
        available: cached.notes.length > 0,
        timeline: cached,
        position: 0,
        activeGroup: null,
        playheadGroup: null,
        playhead: false,
      });
      return;
    }
    setState((current) => ({ ...current, phase: 'loading', available: false }));
    loadScorePlayback(api, renderKey)
      .then((timeline) => {
        if (!live) return;
        timelineRef.current = timeline;
        setState({
          phase: 'idle',
          available: timeline.notes.length > 0,
          timeline,
          position: 0,
          activeGroup: null,
          playheadGroup: null,
          playhead: false,
        });
      })
      .catch((error) => {
        if (!live) return;
        setUnavailableReason(error instanceof Error ? error.message : null);
        if (!(error instanceof ApiError) || error.code !== 'unsupported-source') {
          onError(error);
        }
        setState({
          ...EMPTY_STATE,
          phase: 'unavailable',
        });
      });
    return () => {
      live = false;
    };
  }, [api, enabled, onError, renderKey, stop, viewKey]);

  const activeElementIds = useMemo(
    () => new Set(state.activeGroup?.elementIds ?? []),
    [state.activeGroup],
  );
  const playheadElementIds = useMemo(
    () => new Set(state.playheadGroup?.elementIds ?? []),
    [state.playheadGroup],
  );

  return {
    render,
    unavailableReason,
    ...state,
    toggle,
    cueMeasure,
    audition,
    auditionAdjacent,
    seekMeasure,
    clearAuditionHighlight,
    stop,
    canToggle: state.available && enabled,
    activeElementIds,
    playheadElementIds,
  };
}

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';

import type { UpdateSnapshot } from '../generated/protocol.ts';

const UPDATE_EVENT = 'ossia://update';
const POLL_INTERVAL = 400;

export function useUpdate(native: boolean, onError: (message: string) => void) {
  const [snapshot, setSnapshot] = useState<UpdateSnapshot | null>(null);
  const poll = useRef<number | undefined>(undefined);
  const revision = useRef(0);
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;

  const stopPolling = useCallback(() => {
    window.clearTimeout(poll.current);
    poll.current = undefined;
  }, []);

  const watch = useCallback(
    (current: UpdateSnapshot | null) => {
      stopPolling();
      if (
        current?.state !== 'checking' &&
        current?.state !== 'downloading' &&
        current?.state !== 'installing'
      ) {
        return;
      }
      poll.current = window.setTimeout(() => {
        const requested = revision.current;
        void invoke<UpdateSnapshot>('update_state')
          .then((next) => {
            if (requested !== revision.current) return;
            setSnapshot(next);
            watch(next);
          })
          .catch((error) => onErrorRef.current(String(error)));
      }, POLL_INTERVAL);
    },
    [stopPolling],
  );

  const refresh = useCallback(async () => {
    if (!native) return;
    try {
      const requested = revision.current;
      const next = await invoke<UpdateSnapshot>('update_state');
      if (requested !== revision.current) return;
      setSnapshot(next);
      watch(next);
    } catch (error) {
      onErrorRef.current(String(error));
    }
  }, [native, watch]);

  useEffect(() => {
    if (!native) return;
    let live = true;
    let dispose: (() => void) | undefined;
    void listen<UpdateSnapshot>(UPDATE_EVENT, (event) => {
      if (!live) return;
      revision.current += 1;
      setSnapshot(event.payload);
      watch(event.payload);
    }).then((unlisten) => {
      if (live) dispose = unlisten;
      else unlisten();
    });
    void refresh();
    return () => {
      live = false;
      dispose?.();
      stopPolling();
    };
  }, [native, refresh, stopPolling, watch]);

  const run = useCallback(
    async (command: 'update_check' | 'update_apply' | 'release_notes' | 'update_cancel') => {
      try {
        const requested = ++revision.current;
        const next = await invoke<UpdateSnapshot>(command);
        if (requested === revision.current) {
          setSnapshot(next);
          watch(next);
        }
        return next;
      } catch (error) {
        onErrorRef.current(String(error));
        return null;
      }
    },
    [watch],
  );

  return {
    snapshot,
    refresh,
    check: () => run('update_check'),
    apply: () => run('update_apply'),
    cancel: () => run('update_cancel'),
    notes: () => run('release_notes'),
  };
}

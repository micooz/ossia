import { useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';

import type { DocumentContext, SelectionTarget, Selection } from '../generated/protocol.ts';
import { selectionKey } from './api.ts';
import type { Api } from './api.ts';

type PendingSelection = {
  selection: Selection;
  instanceId: string;
  renderKey: string;
};

function matches(pending: PendingSelection, context: DocumentContext | null) {
  return (
    context?.instanceId === pending.instanceId &&
    context.document.documentId === pending.selection.documentId &&
    context.document.sourceHash === pending.selection.sourceHash &&
    context.render?.renderKey === pending.renderKey
  );
}

/** Paint selection immediately and serialize saves, coalescing subsequent clicks. */
export function useSelection(
  api: Api,
  context: DocumentContext | null,
  contextRef: RefObject<DocumentContext | null>,
  applyContext: (context: DocumentContext) => void,
  refresh: () => Promise<unknown>,
  onError: (error: unknown) => void,
) {
  const [optimistic, setOptimistic] = useState<PendingSelection | null>(null);
  const pending = useRef<PendingSelection | null>(null);
  const saving = useRef(false);
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  useEffect(() => {
    if (pending.current && !matches(pending.current, context)) {
      pending.current = null;
      setOptimistic(null);
    }
  }, [context]);

  async function save() {
    if (saving.current) return;
    saving.current = true;
    try {
      while (active.current && pending.current) {
        const next = pending.current;
        if (!matches(next, contextRef.current)) {
          pending.current = null;
          setOptimistic(null);
          break;
        }
        try {
          const result = await api.selection(next.selection.documentId, next.selection);
          if (!active.current) break;
          if (matches(next, contextRef.current)) applyContext(result);
          // An older acknowledgment must not erase clicks made while it was in flight.
          if (pending.current === next) {
            pending.current = null;
            setOptimistic(null);
          }
        } catch (error) {
          if (!active.current) break;
          if (pending.current && matches(next, contextRef.current)) {
            pending.current = null;
            setOptimistic(null);
            onError(error);
            await refresh().catch(onError);
          }
        }
      }
    } finally {
      saving.current = false;
    }
  }

  function choose(target: SelectionTarget) {
    const current = contextRef.current;
    if (!current?.render || current.render.manifest.sourceHash !== current.document.sourceHash)
      return;
    const selected =
      pending.current && matches(pending.current, current)
        ? pending.current.selection.targets
        : (current.selection?.targets ?? []);
    const key = selectionKey(target);
    const targets = selected.some((item) => selectionKey(item) === key)
      ? selected.filter((item) => selectionKey(item) !== key)
      : [...selected, target];
    const next: PendingSelection = {
      instanceId: current.instanceId,
      renderKey: current.render.renderKey,
      selection: {
        documentId: current.document.documentId,
        sourceHash: current.document.sourceHash,
        targets,
      },
    };
    pending.current = next;
    setOptimistic(next);
    void save();
  }

  return {
    choose,
    selected:
      optimistic && matches(optimistic, context)
        ? optimistic.selection.targets
        : (context?.selection?.targets ?? []),
  };
}

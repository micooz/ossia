import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';

import { useI18n } from '../i18n.tsx';

export type ToastKind = 'error' | 'success' | 'warning' | 'info';

type ToastItem = {
  id: number;
  kind: ToastKind;
  message: string;
};

const DEFAULT_DURATION_MS = 4500;
const ERROR_DURATION_MS = 8000;

export function useToasts() {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const nextId = useRef(1);
  const active = useRef<{ id: number; key: string; timer: number } | null>(null);

  const dismiss = useCallback((id: number) => {
    if (active.current?.id !== id) return;
    window.clearTimeout(active.current.timer);
    active.current = null;
    setToasts([]);
  }, []);

  const show = useCallback(
    (message: string, kind: ToastKind = 'info') => {
      if (!message) return;
      const key = `${kind}:${message}`;
      if (active.current?.key === key) return;
      if (active.current) window.clearTimeout(active.current.timer);
      const id = nextId.current++;
      const timer = window.setTimeout(
        () => dismiss(id),
        kind === 'error' ? ERROR_DURATION_MS : DEFAULT_DURATION_MS,
      );
      active.current = { id, key, timer };
      setToasts([{ id, kind, message }]);
    },
    [dismiss],
  );

  useEffect(
    () => () => {
      if (active.current) window.clearTimeout(active.current.timer);
      active.current = null;
    },
    [],
  );

  return { toasts, show, dismiss };
}

export function ToastRegion({
  toasts,
  onDismiss,
}: {
  toasts: ToastItem[];
  onDismiss: (id: number) => void;
}) {
  const { t } = useI18n();
  const icons = {
    error: CircleAlert,
    success: CircleCheck,
    warning: TriangleAlert,
    info: Info,
  } satisfies Record<ToastKind, typeof Info>;

  return (
    <div className="toast-region" aria-label={t('notifications')}>
      {toasts.map((toast) => {
        const Icon = icons[toast.kind];
        return (
          <div
            className={`toast is-${toast.kind}`}
            key={toast.id}
            role={toast.kind === 'error' ? 'alert' : 'status'}
            aria-atomic="true"
          >
            <Icon aria-hidden="true" />
            <span>{toast.message}</span>
            <button
              className="toast-close"
              type="button"
              aria-label={t('dismissNotification')}
              onClick={() => onDismiss(toast.id)}
            >
              <X aria-hidden="true" />
            </button>
          </div>
        );
      })}
    </div>
  );
}

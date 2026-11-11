import type { ExchangeResponse, ViewSnapshot } from './generated/protocol.ts';
import type { UiLanguage } from './generated/protocol.ts';
import { translate } from './i18n.tsx';

export interface Session {
  token: string;
  viewId: string;
  language: UiLanguage;
}
const storageKey = (viewId: string) => `ossia:view:${viewId}`;
let boot: Promise<{ session: Session; view?: ViewSnapshot }> | undefined;

export function resolveSession(fallbackLanguage: UiLanguage) {
  return (boot ??= exchange(fallbackLanguage));
}
async function exchange(
  fallbackLanguage: UiLanguage,
): Promise<{ session: Session; view?: ViewSnapshot }> {
  const fragment = new URLSearchParams(location.hash.slice(1));
  const ticket = fragment.get('ticket');
  if (ticket) {
    history.replaceState(null, '', location.pathname + location.search);
    const response = await fetch('/api/v1/sessions/exchange', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ticket }),
    });
    if (!response.ok) throw new Error(translate(fallbackLanguage, 'endpointExpired'));
    const result: ExchangeResponse = await response.json();
    const session = {
      token: result.token,
      viewId: result.view.viewId,
      language: result.language ?? fallbackLanguage,
    };
    sessionStorage.setItem(storageKey(session.viewId), JSON.stringify(session));
    history.replaceState(null, '', `${location.pathname}${location.search}#view=${session.viewId}`);
    return { session, view: result.view };
  }
  const viewId = fragment.get('view');
  if (viewId) {
    const raw = sessionStorage.getItem(storageKey(viewId));
    if (raw) {
      const session: Session = JSON.parse(raw);
      if (session.viewId === viewId && typeof session.token === 'string') {
        return {
          session: {
            ...session,
            language: session.language ?? fallbackLanguage,
          },
        };
      }
    }
  }
  throw new Error(translate(fallbackLanguage, 'endpointMissing'));
}
export function forgetSession(session: Session) {
  sessionStorage.removeItem(storageKey(session.viewId));
}

import type { ExchangeResponse, ViewSnapshot } from './generated/protocol.ts';

export interface Session {
  token: string;
  viewId: string;
}
const storageKey = (viewId: string) => `ossia:view:${viewId}`;
let boot: Promise<{ session: Session; view?: ViewSnapshot }> | undefined;

export function resolveSession() {
  return (boot ??= exchange());
}
async function exchange(): Promise<{ session: Session; view?: ViewSnapshot }> {
  const fragment = new URLSearchParams(location.hash.slice(1));
  const ticket = fragment.get('ticket');
  if (ticket) {
    history.replaceState(null, '', location.pathname + location.search);
    const response = await fetch('/api/v1/sessions/exchange', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ticket }),
    });
    if (!response.ok) throw new Error('打开链接已过期或已使用，请让 Agent 重新打开谱面。');
    const result: ExchangeResponse = await response.json();
    const session = { token: result.token, viewId: result.view.viewId };
    sessionStorage.setItem(storageKey(session.viewId), JSON.stringify(session));
    history.replaceState(null, '', `${location.pathname}${location.search}#view=${session.viewId}`);
    return { session, view: result.view };
  }
  const viewId = fragment.get('view');
  if (viewId) {
    const raw = sessionStorage.getItem(storageKey(viewId));
    if (raw) {
      const session: Session = JSON.parse(raw);
      if (session.viewId === viewId && typeof session.token === 'string') return { session };
    }
  }
  throw new Error('请让 Agent 使用 ossia_open 打开谱面。每个打开链接对应独立视图。');
}
export function forgetSession(session: Session) {
  sessionStorage.removeItem(storageKey(session.viewId));
}

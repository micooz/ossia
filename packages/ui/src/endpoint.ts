// Which API deployment to talk to, and the secret it expects.
//
// Two ways in, and no third: the Tauri shell resolves `{ baseUrl, token }` through the
// `get_endpoint` command (env -> <app_data>/config.json -> loopback dev), and the browser dev build
// reads VITE_API_BASE. The SPA never guesses a port, because two live deployments would otherwise
// mean the UI talks to the wrong document store.
export interface Endpoint {
  baseUrl: string;
  token: string;
  /** Human-readable origin, for the status line. */
  source: 'tauri' | 'env';
}

type TauriWindow = Window & { __OSSIA_ENDPOINT__?: { baseUrl: string; token: string } };

export async function resolveEndpoint(): Promise<Endpoint> {
  const injected = (window as TauriWindow).__OSSIA_ENDPOINT__;
  if (injected?.baseUrl)
    return { baseUrl: injected.baseUrl, token: injected.token ?? '', source: 'tauri' };

  if ('__TAURI_INTERNALS__' in window) {
    const { invoke } = await import('@tauri-apps/api/core');
    const r = await invoke<{ baseUrl: string; token: string }>('get_endpoint');
    return { baseUrl: r.baseUrl, token: r.token, source: 'tauri' };
  }

  const env = (import.meta.env.VITE_API_BASE as string | undefined) ?? 'http://127.0.0.1:3210';
  return {
    baseUrl: env.replace(/\/$/, ''),
    token: (import.meta.env.VITE_API_TOKEN as string | undefined) ?? '',
    source: 'env',
  };
}

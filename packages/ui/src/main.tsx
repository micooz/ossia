import { createRoot } from 'react-dom/client';

/** Tauri rejects commands with a plain string (not an Error), so `.message` would be undefined and
    the user would see "undefined" instead of the reason the endpoint could not be resolved. */
function describe(e: unknown): string {
  if (e instanceof Error) return e.message || e.name;
  if (typeof e === 'string') return e;
  try {
    return JSON.stringify(e) ?? String(e);
  } catch {
    return String(e);
  }
}
import { makeApi } from './api.ts';
import { App } from './App.tsx';
import { resolveEndpoint } from './endpoint.ts';

import './styles.css';

// Boot: resolve which API to use before anything renders, because every request needs the
// base URL and token. Failure is shown as text rather than an empty page.
const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('missing #root');
const root = createRoot(rootEl);

root.render(<p className="hint">正在连接本地服务…</p>);
resolveEndpoint()
  .then((ep) =>
    root.render(
      <App
        api={makeApi(ep)}
        source={ep.source === 'tauri' ? `本地服务 ${ep.baseUrl}` : `开发模式 ${ep.baseUrl}`}
      />,
    ),
  )
  .catch((e) =>
    root.render(
      <main className="app">
        <div className="panel">
          <h3>无法启动</h3>
          <p className="error">{describe(e)}</p>
          <p className="hint">
            桌面端需要由 Tauri 壳注入服务地址；在浏览器里请直接运行{' '}
            <code>pnpm --filter @ossia/backend start</code> 并设置 VITE_API_BASE。
          </p>
        </div>
      </main>,
    ),
  );

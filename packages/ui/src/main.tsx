import { createRoot } from 'react-dom/client';

import { describe } from './api.ts';
import { App } from './App.tsx';
import { resolveSession } from './endpoint.ts';

import './styles.css';

const element = document.getElementById('root');
if (!element) throw new Error('missing #root');
const root = createRoot(element);
root.render(
  <main className="start panel">
    <h1>Ossia</h1>
    <p>正在连接本地服务…</p>
  </main>,
);
resolveSession()
  .then(({ session }) => root.render(<App session={session} />))
  .catch((e) =>
    root.render(
      <main className="start panel">
        <h1>打开谱面</h1>
        <p role="alert">{describe(e)}</p>
      </main>,
    ),
  );

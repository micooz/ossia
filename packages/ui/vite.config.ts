import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import type { Connect, Plugin } from 'vite';

function scoresRoute(): Plugin {
  const rewrite = (url: string | undefined) => {
    if (!url) return undefined;
    const query = url.indexOf('?');
    const path = query === -1 ? url : url.slice(0, query);
    return path === '/scores' ? `/scores.html${query === -1 ? '' : url.slice(query)}` : undefined;
  };
  const middleware = (req: Connect.IncomingMessage, _res: unknown, next: Connect.NextFunction) => {
    req.url = rewrite(req.url) ?? req.url;
    next();
  };
  return {
    name: 'ossia-scores-route',
    configureServer(server) {
      server.middlewares.use(middleware);
    },
    configurePreviewServer(server) {
      server.middlewares.use(middleware);
    },
  };
}

export default defineConfig({
  base: './',
  plugins: [react(), scoresRoute()],
  build: {
    target: 'es2022',
    outDir: 'dist',
    sourcemap: true,
    rolldownOptions: { input: { main: 'main.html', scores: 'scores.html' } },
  },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: { '/api': { target: 'http://127.0.0.1:3212', changeOrigin: true } },
  },
});

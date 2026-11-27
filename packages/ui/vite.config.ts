import type { ServerResponse } from 'node:http';

import react from '@vitejs/plugin-react';
import { createLogger, defineConfig } from 'vite';
import type { Connect, Plugin } from 'vite';

import { createDevProxyLogging } from './dev-proxy.ts';

const devProxyLogging = createDevProxyLogging(createLogger());

function scoresRoute(): Plugin {
  const pathname = (url: string | undefined) => {
    if (!url) return undefined;
    const query = url.indexOf('?');
    return query === -1 ? url : url.slice(0, query);
  };
  const rewrite = (url: string | undefined) => {
    const path = pathname(url);
    if (!url || !path) return undefined;
    const query = url.indexOf('?');
    return path === '/scores' ? `/scores.html${query === -1 ? '' : url.slice(query)}` : undefined;
  };
  const middleware = (
    req: Connect.IncomingMessage,
    res: ServerResponse,
    next: Connect.NextFunction,
  ) => {
    if (pathname(req.url) === '/score.html') {
      res.statusCode = 404;
      res.end('Not Found');
      return;
    }
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
  customLogger: devProxyLogging.logger,
  plugins: [react(), scoresRoute()],
  build: {
    target: 'es2022',
    outDir: 'dist',
    sourcemap: false,
    rolldownOptions: { input: { index: 'index.html', main: 'main.html', scores: 'scores.html' } },
  },
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:3211',
        changeOrigin: true,
        configure: devProxyLogging.configure,
      },
    },
  },
});

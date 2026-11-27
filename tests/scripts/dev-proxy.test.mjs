import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';

import { test } from 'vite-plus/test';

import { createDevProxyLogging } from '../../packages/ui/dev-proxy.ts';

test('reports a backend outage once and reports a new outage after recovery', () => {
  const warnings = [];
  const errors = [];
  const base = {
    warn: (message) => warnings.push(message),
    error: (...args) => errors.push(args),
  };
  const { logger, configure } = createDevProxyLogging(base);
  const proxy = new EventEmitter();
  configure(proxy);
  const refused = Object.assign(new Error('connect ECONNREFUSED'), { code: 'ECONNREFUSED' });
  const options = { error: refused };

  logger.error('http proxy error: /api/v1/status', options);
  logger.error('http proxy error: /api/v1/documents/1/context', options);
  assert.equal(warnings.length, 1);
  assert.equal(errors.length, 0);

  const other = Object.assign(new Error('socket closed'), { code: 'ECONNRESET' });
  logger.error('http proxy error: /api/v1/status', { error: other });
  logger.error('unrelated error', options);
  assert.equal(errors.length, 2);

  proxy.emit('proxyRes');
  logger.error('http proxy error: /api/v1/status', options);
  assert.equal(warnings.length, 2);
});

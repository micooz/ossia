import type { Logger, ProxyOptions } from 'vite';

export function createDevProxyLogging(base: Logger) {
  let unavailable = false;
  const logger: Logger = {
    ...base,
    error(message, options) {
      const error = options?.error;
      if (
        message.includes('http proxy error:') &&
        error &&
        'code' in error &&
        error.code === 'ECONNREFUSED'
      ) {
        if (!unavailable) {
          unavailable = true;
          base.warn('Ossia backend is unavailable; waiting for it to start.', { timestamp: true });
        }
        return;
      }
      base.error(message, options);
    },
  };
  const configure: NonNullable<ProxyOptions['configure']> = (proxy) => {
    proxy.on('proxyRes', () => {
      unavailable = false;
    });
  };
  return { logger, configure };
}

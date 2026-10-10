// The SPA origins the API answers. Kept free of Node imports so both the route handlers and
// `src/middleware.ts` (which runs outside the Node route module) can share one list.
//
// Vite dev + preview, and the Tauri custom-protocol hosts: macOS/Linux `tauri://localhost`,
// Windows/Linux `http(s)://tauri.localhost`. Add the deployed web/API host here when the site ships.
export const ALLOWED_ORIGINS = new Set([
  'http://localhost:5173',
  'http://127.0.0.1:5173',
  'http://localhost:4173',
  'http://127.0.0.1:4173',
  'tauri://localhost',
  'http://tauri.localhost',
  'https://tauri.localhost',
]);

export const CORS_HEADERS: Record<string, string> = {
  'access-control-allow-headers': 'content-type, x-ossia-token',
  'access-control-allow-methods': 'GET, POST, OPTIONS',
  'access-control-max-age': '600',
};

/** Response headers for `origin`, or just `Vary` when the origin is not allowed. */
export function corsFor(origin: string | null): Record<string, string> {
  const h: Record<string, string> = { vary: 'Origin' };
  if (origin && ALLOWED_ORIGINS.has(origin)) {
    h['access-control-allow-origin'] = origin;
    Object.assign(h, CORS_HEADERS);
  }
  return h;
}

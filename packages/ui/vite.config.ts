import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// The SPA is one static build shared by the macOS/Windows shell and the mobile shells, so it must
// not depend on where it is served from: relative base + no dev-server proxy assumptions.
export default defineConfig({
  base: './',
  plugins: [react()],
  build: { target: 'es2022', outDir: 'dist', sourcemap: true },
  // 127.0.0.1, not the default localhost: Tauri's devUrl is `http://127.0.0.1:5173`, and the shell
  // (`tauri dev`) starts this server through `beforeDevCommand`, so the host has to match exactly.
  server: { host: '127.0.0.1', port: 5173, strictPort: true },
});

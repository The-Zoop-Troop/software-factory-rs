import { defineConfig } from 'vite';

// CSR only: the console binary serves dist/ at `/` with an SPA fallback.
export default defineConfig({
  build: { target: 'es2022', outDir: 'dist', emptyOutDir: true, sourcemap: false },
  server: { proxy: { '/rigs': 'http://127.0.0.1:7700', '/whoami': 'http://127.0.0.1:7700', '/events': 'http://127.0.0.1:7700', '/.well-known': 'http://127.0.0.1:7700' } },
});

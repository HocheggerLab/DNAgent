// Default Vite settings; in e2e mode only, proxy desktop commands to the Rust test server.
import { defineConfig } from 'vite';

export default defineConfig(({ mode }) => ({
  clearScreen: false,
  server: mode === 'e2e' ? {
    proxy: { '/__dnagent': { target: `http://127.0.0.1:${process.env.DNAGENT_E2E_PORT ?? '1431'}`, rewrite: (path: string) => path.replace(/^\/__dnagent/, '') } },
  } : {},
}));

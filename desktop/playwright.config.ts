// Chromium-only harness for app logic and layout. Production uses WKWebView/WebKitGTK;
// see e2e/README.md. Port 1421 avoids the Tauri dev server on 1420.
import { defineConfig, devices } from '@playwright/test';

const port = 1421;
const serverPort = 1431;

export default defineConfig({
  testDir: './e2e',
  testMatch: 'scenarios.spec.ts',
  outputDir: './e2e/artifacts/test-results',
  globalSetup: './e2e/global-setup.ts',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    headless: true,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } } }],
  webServer: [
    {
      // The real Rust desktop session behind HTTP; relative fixture paths resolve from the repo root.
      command: `cargo run -q -p dnagent-desktop-api --example e2e_server -- --port ${serverPort}`,
      cwd: '..',
      url: `http://127.0.0.1:${serverPort}/health`,
      // The built-in enzyme set, like the CLI oracle: a locally installed REBASE must not change results.
      env: { DNAGENT_ENZYMES: 'builtin', DNAGENT_FEATURE_DB: 'desktop/e2e/artifacts/feature-library.sqlite' },
      reuseExistingServer: false,
      timeout: 300_000,
    },
    {
      command: `npx vite --mode e2e --host 127.0.0.1 --port ${port} --strictPort`,
      url: `http://127.0.0.1:${port}`,
      env: { DNAGENT_E2E_PORT: String(serverPort) },
      reuseExistingServer: false,
      timeout: 30_000,
    },
  ],
});

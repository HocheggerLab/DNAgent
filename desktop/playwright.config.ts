// Chromium-only harness for app logic and layout. Production uses WKWebView/WebKitGTK;
// see e2e/README.md. Port 1421 avoids the Tauri dev server on 1420.
import { defineConfig, devices } from '@playwright/test';

const port = 1421;

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
  webServer: {
    command: `npx vite --mode e2e --host 127.0.0.1 --port ${port} --strictPort`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});

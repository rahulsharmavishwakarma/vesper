import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './apps/desktop/tests',
  testMatch: '**/*.spec.mjs',
  timeout: 120_000,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    browserName: 'chromium',
    baseURL: 'http://127.0.0.1:1420',
    viewport: { width: 1280, height: 900 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'npm run dev --workspace @vesper/desktop -- --host 127.0.0.1',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: false,
    timeout: 30_000,
  },
});

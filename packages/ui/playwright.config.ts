import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:5173', channel: 'chrome', trace: 'retain-on-failure' },
  webServer: {
    command: 'pnpm dev',
    url: 'http://127.0.0.1:5173',
    reuseExistingServer: !process.env.CI,
  },
  projects: [
    { name: 'browser', outputDir: 'test-results/browser', testMatch: '**/browser.spec.ts' },
    {
      name: 'native',
      outputDir: 'test-results/native',
      testMatch: '**/native.spec.ts',
      timeout: 180_000,
    },
  ],
});

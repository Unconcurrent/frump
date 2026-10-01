import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.ts',
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:4317', viewport: { width: 1440, height: 960 }, trace: 'retain-on-failure' },
  webServer: { command: 'node tests/server.mjs', url: 'http://127.0.0.1:4317', reuseExistingServer: false, timeout: 120_000 },
});

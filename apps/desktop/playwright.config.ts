import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 120_000,
  outputDir: './artifacts/playwright',
  preserveOutput: 'always',
  use: {
    baseURL: 'http://127.0.0.1:4173',
    headless: true,
    video: 'on',
    trace: 'on'
  },
  webServer: {
    command:
      'pnpm --filter desktop build && pnpm --filter desktop preview --host 127.0.0.1 --port 4173',
    port: 4173,
    reuseExistingServer: true,
    timeout: 120_000
  }
});

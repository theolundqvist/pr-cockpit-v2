import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  testMatch: ['tests/**/*.spec.ts', 'playwright/**/*.spec.ts'],
  timeout: 120_000,
  outputDir: './artifacts/playwright',
  preserveOutput: 'always',
  use: {
    baseURL: 'http://127.0.0.1:4173',
    browserName: process.env.PLAYWRIGHT_BROWSER ?? 'webkit',
    headless: true,
    video: 'on',
    trace: 'on'
  },
  webServer: {
    command:
      'pnpm --filter desktop build && mkdir -p build/assets/grammars .svelte-kit/output/client/assets/grammars && cp -R src/assets/grammars/. build/assets/grammars/ && cp -R src/assets/grammars/. .svelte-kit/output/client/assets/grammars/ && pnpm --filter desktop preview --host 127.0.0.1 --port 4173',
    port: 4173,
    reuseExistingServer: true,
    timeout: 120_000
  }
});

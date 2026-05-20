import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test, type Page } from '@playwright/test';

const artifactRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  '..',
  'artifacts',
  'm6-demo'
);

async function pause(page: Page, ms: number): Promise<void> {
  await page.waitForTimeout(ms);
}

test('m6 demo walkthrough recording', async ({ browser }) => {
  await mkdir(artifactRoot, { recursive: true });

  const context = await browser.newContext({
    baseURL: 'http://127.0.0.1:4173',
    recordVideo: {
      dir: artifactRoot,
      size: { width: 1280, height: 720 }
    }
  });
  const page = await context.newPage();
  const video = page.video();
  expect(video).not.toBeNull();

  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M6_STACKS_DEBUG__));
  await expect(page.getByText('Pull Request Inbox')).toBeVisible();
  await pause(page, 1600);

  await page.getByTestId('account-switcher-trigger').click();
  await page.getByTestId('account-option-all').click();
  await expect(page.getByTestId('inbox-account-badge').first()).toBeVisible();
  await pause(page, 1800);

  await page
    .getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ })
    .first()
    .click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(page.getByRole('button', { name: 'Files' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Checks' })).toBeVisible();
  await pause(page, 1600);

  const autoMergeButton = page.getByRole('button', { name: 'Enable auto-merge' });
  if (await autoMergeButton.count()) {
    await autoMergeButton.click();
    await expect(page.getByRole('dialog', { name: 'Enable auto-merge' })).toBeVisible();
  } else {
    await page
      .getByRole('button', { name: /^Merge/ })
      .first()
      .click();
    await expect(page.getByRole('dialog')).toBeVisible();
  }
  await pause(page, 1500);
  await page.keyboard.press('Escape');
  await pause(page, 900);

  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M6_STACKS_DEBUG__));
  await expect(page.getByTestId('stack-summary-stack-linear-repo2')).toBeVisible();
  await page.getByTestId('stack-summary-stack-linear-repo2').click();
  await expect(page.getByTestId('stack-tree')).toBeVisible();
  await expect(page.getByTestId('stack-tree').getByTestId('stack-row')).toHaveCount(3);
  await pause(page, 2100);

  await page.keyboard.press('Control+k');
  await expect(page.getByTestId('command-palette-modal')).toBeVisible();
  await page.getByTestId('command-palette-search').fill('open inbox');
  await pause(page, 1500);
  await page.keyboard.press('Escape');
  await pause(page, 800);

  await page.goto('/');
  await expect(page.getByText('Pull Request Inbox')).toBeVisible();
  await pause(page, 1800);

  await context.close();
  if (video) {
    await video.saveAs(path.join(artifactRoot, 'demo.webm'));
  }
});

import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

test('offline fixture inbox to diff smoke flow', async ({ page }) => {
  const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm1-smoke');
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/');
  await expect(page.getByText('Pull Request Inbox')).toBeVisible();
  await page.screenshot({ path: path.join(artifactRoot, '01-inbox.png'), fullPage: true });

  const firstRow = page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ });
  await expect(firstRow).toBeVisible();
  await firstRow.click();

  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(
    page.getByRole('heading', { name: /Active Fixture PR Falcon Diff Stress/ })
  ).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '02-pr-detail-conversation.png'),
    fullPage: true
  });

  const sectionTabs = page.getByRole('navigation', { name: 'Pull request sections' });
  await sectionTabs.getByRole('button', { name: 'Files' }).click();
  const scroller = page.locator('.diff-scroll');
  await expect(scroller).toBeVisible();
  await page.screenshot({ path: path.join(artifactRoot, '03-diff-unified.png'), fullPage: true });

  await page.getByRole('button', { name: 'Side by side' }).click();
  await expect(page.locator('.diff-line-row.side-by-side').first()).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '04-diff-side-by-side.png'),
    fullPage: true
  });

  await scroller.evaluate((node) => {
    node.scrollTop = node.scrollHeight;
  });

  await expect(page.getByText('LINE_5000')).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '05-highlighted-line.png'),
    fullPage: true
  });

  // Keep the recording alive long enough for a ~30-second artifact.
  await page.waitForTimeout(30_000);
});

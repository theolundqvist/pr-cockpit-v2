import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

test.describe('m4 range-diff surface', () => {
  test('local-git mode renders markers and intra-line highlights', async ({ page }) => {
    const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm4-range-diff');
    await mkdir(artifactRoot, { recursive: true });

    await page.goto('/pr/pr_1/range-diff');
    await page.evaluate(() => window.__RANGE_DIFF_DEBUG__?.setMode('local'));
    await page.reload();

    await expect(page.getByTestId('range-diff-mode-badge')).toHaveText('Local git');
    await expect(page.getByTestId('range-diff-marker').first()).toHaveText('!');
    await page.getByRole('button', { name: 'Show intra-line diff' }).click();
    await expect(page.locator('[data-highlight-kind="Added"]').first()).toBeVisible();
    await expect(page.locator('[data-highlight-kind="Removed"]').first()).toBeVisible();

    await page.screenshot({
      path: path.join(artifactRoot, 'local-git.png'),
      fullPage: true
    });
  });

  test('rest mode renders the same range-diff structure', async ({ page }) => {
    const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm4-range-diff');
    await mkdir(artifactRoot, { recursive: true });

    await page.goto('/pr/pr_1/range-diff');
    await page.evaluate(() => window.__RANGE_DIFF_DEBUG__?.setMode('rest'));
    await page.reload();

    await expect(page.getByTestId('range-diff-mode-badge')).toHaveText('REST /compare');
    await expect(page.getByTestId('range-diff-marker').first()).toHaveText('!');
    await page.getByRole('button', { name: 'Show intra-line diff' }).click();
    await expect(page.locator('[data-highlight-kind="Added"]').first()).toBeVisible();
    await expect(page.locator('[data-highlight-kind="Removed"]').first()).toBeVisible();

    await page.screenshot({
      path: path.join(artifactRoot, 'rest-compare.png'),
      fullPage: true
    });
  });

  test('force-push banner links to latest push pair', async ({ page }) => {
    await page.goto('/');
    await page
      .getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/i })
      .first()
      .click();
    await expect(page.getByText('Force-pushed 2 times')).toBeVisible();
    await page.getByRole('link', { name: 'view range-diff' }).click();
    await expect(page).toHaveURL(/\/pr\/pr_1\/range-diff\?old=.*&new=.*/);
  });
});

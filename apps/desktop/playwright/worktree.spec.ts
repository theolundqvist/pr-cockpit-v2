import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

test('worktree mapping and roots panel render', async ({ page }) => {
  const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'worktree');
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Worktree roots' })).toBeVisible();
  await expect(page.getByText('Absolute paths only; one root per line.')).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '01-inbox-worktree-roots.png'),
    fullPage: true
  });

  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(page.getByText(/PR #1 · confidence/)).toBeVisible();
  await expect(page.getByText(/feature\/pr-1 • clean • ↑1\/↓0/)).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '02-pr-worktree-mapping.png'),
    fullPage: true
  });
});

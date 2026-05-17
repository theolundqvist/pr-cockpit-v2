import { expect, test } from '@playwright/test';

test('offline fixture inbox to diff smoke flow', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('Pull Request Inbox')).toBeVisible();

  const firstRow = page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ });
  await expect(firstRow).toBeVisible();
  await firstRow.click();

  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  const sectionTabs = page.getByRole('navigation', { name: 'Pull request sections' });
  await sectionTabs.getByRole('button', { name: 'Files' }).click();
  const scroller = page.locator('.diff-scroll');
  await expect(scroller).toBeVisible();
  await page.getByRole('button', { name: 'Side by side' }).click();

  await scroller.evaluate((node) => {
    node.scrollTop = node.scrollHeight;
  });

  await expect(page.getByText('LINE_5000')).toBeVisible();
});

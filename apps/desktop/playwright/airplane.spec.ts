import { expect, test } from '@playwright/test';

test('airplane mode queues and drains mutations without data loss', async ({ page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      consoleErrors.push(message.text());
    }
  });

  await page.goto('/');
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(
    page.getByRole('heading', { name: /Active Fixture PR Falcon Diff Stress/ })
  ).toBeVisible();

  await page.evaluate(async () => {
    await window.__M2_DEBUG__?.setOffline();
  });
  await expect(page.getByText('Offline — queued:')).toBeVisible();

  const textarea = page.getByTestId('composer-textarea');
  const addCommentButton = page.getByRole('button', { name: 'Add comment' });
  for (let index = 0; index < 10; index += 1) {
    await textarea.fill(`Offline comment ${index + 1}`);
    await addCommentButton.click();
  }

  const labelInput = page.getByPlaceholder('new label');
  const addLabelButton = labelInput.locator('..').getByRole('button', { name: 'Add' });
  for (const label of ['offline-a', 'offline-b', 'offline-c']) {
    await labelInput.fill(label);
    await addLabelButton.click();
  }

  const resolveButtons = page.getByRole('button', { name: 'Resolve' });
  await resolveButtons.nth(0).click();
  await resolveButtons.nth(1).click();

  await expect
    .poll(async () => page.locator('.pending-affordance').count(), { timeout: 10_000 })
    .toBeGreaterThan(0);

  await page.evaluate(async () => {
    await window.__M2_DEBUG__?.setOnline();
  });
  await expect(page.getByText('Offline — queued:')).toHaveCount(0);
  await expect(page.locator('.pending-affordance')).toHaveCount(0, { timeout: 10_000 });

  await page.getByRole('button', { name: /Sync errors/ }).click();
  await expect(page.getByText('No failed mutations.')).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

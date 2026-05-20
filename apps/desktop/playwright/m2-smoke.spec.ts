import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

test('m2 optimistic writes smoke flow', async ({ page }) => {
  const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm2-smoke');
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/');
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(
    page.getByRole('heading', { name: /Active Fixture PR Falcon Diff Stress/ })
  ).toBeVisible();
  await page.screenshot({ path: path.join(artifactRoot, '01-open-pr.png'), fullPage: true });

  const composer = page.getByTestId('composer-textarea');
  await composer.fill('Manual smoke optimistic comment');
  await page.getByRole('button', { name: 'Add comment' }).click();
  await page.screenshot({ path: path.join(artifactRoot, '02-comment-submit.png'), fullPage: true });

  const labelInput = page.getByPlaceholder('new label');
  const existingLabelButtons = page.locator('aside .Box:has-text("Labels") button.Label');
  if ((await existingLabelButtons.count()) > 0) {
    await existingLabelButtons.first().click();
  }
  await labelInput.fill('m2-smoke-a');
  await labelInput.locator('..').getByRole('button', { name: 'Add' }).click();

  const assigneeInput = page.getByPlaceholder('comma separated logins');
  await assigneeInput.fill('fixture-user');
  await assigneeInput.locator('..').getByRole('button', { name: 'Set' }).click();

  const reviewerInput = page.getByPlaceholder('reviewer login');
  await reviewerInput.fill('fixture-reviewer');
  await reviewerInput.locator('..').getByRole('button', { name: 'Request' }).click();

  await page.getByRole('button', { name: 'Resolve' }).first().click();

  await page.getByRole('button', { name: 'Files' }).click();
  const viewedCheckbox = page.locator('.pr-main input[type="checkbox"]').first();
  await viewedCheckbox.check();
  await page.screenshot({
    path: path.join(artifactRoot, '03-metadata-thread-file-actions.png'),
    fullPage: true
  });

  await page.evaluate(async () => {
    await window.__M2_DEBUG__?.setOffline();
  });
  await expect(page.getByText('Offline — queued:')).toBeVisible();

  await page.getByRole('button', { name: 'Conversation' }).click();
  for (let index = 0; index < 3; index += 1) {
    await composer.fill(`Offline smoke comment ${index + 1}`);
    await page.getByRole('button', { name: 'Add comment' }).click();
  }
  await labelInput.fill('offline-smoke-a');
  await labelInput.locator('..').getByRole('button', { name: 'Add' }).click();
  await labelInput.fill('offline-smoke-b');
  await labelInput.locator('..').getByRole('button', { name: 'Add' }).click();
  await page.screenshot({
    path: path.join(artifactRoot, '04-offline-queued-actions.png'),
    fullPage: true
  });

  await page.evaluate(async () => {
    await window.__M2_DEBUG__?.setOnline();
  });
  await expect(page.getByText('Offline — queued:')).toHaveCount(0);
  await expect(page.locator('.pending-affordance')).toHaveCount(0, { timeout: 10_000 });
  await page.screenshot({
    path: path.join(artifactRoot, '05-reconnected-drained.png'),
    fullPage: true
  });

  await page.getByRole('button', { name: /Sync errors/ }).click();
  await expect(page.getByText('No failed mutations.')).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '06-sync-tray-empty.png'),
    fullPage: true
  });
});

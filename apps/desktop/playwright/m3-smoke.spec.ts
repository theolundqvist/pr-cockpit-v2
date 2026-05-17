import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

const notificationKinds = [
  'review_requested',
  'changes_requested',
  'approved',
  'mention',
  'ci_fail',
  'ci_recover',
  'merge_conflict',
  'mutation_failure'
] as const;

test('m3 smoke captures diff/worktree/notification flows', async ({ page }) => {
  const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm3-smoke');
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/');
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await page.getByRole('button', { name: 'Files' }).click();

  const lineNumbers = page
    .locator('.diff-line-row .diff-line-number')
    .filter({ hasText: /^[0-9]+$/ });
  await lineNumbers.nth(3).click();
  await lineNumbers.nth(7).click({ modifiers: ['Shift'] });
  const inlineAffordance = page.getByRole('button', { name: /Add inline review comment/ });
  await inlineAffordance.evaluate((button) => (button as HTMLButtonElement).click());

  const inlineComposer = page.getByPlaceholder('Leave a review comment');
  await expect(inlineComposer).toBeVisible();
  await page
    .getByTestId('composer-insert-suggestion')
    .first()
    .evaluate((button) => (button as HTMLButtonElement).click());
  await page
    .getByTestId('composer-tab-preview')
    .first()
    .evaluate((button) => (button as HTMLButtonElement).click());
  await expect(page.getByTestId('composer-preview').first()).toContainText('suggestion');
  await page
    .getByTestId('composer-tab-write')
    .first()
    .evaluate((button) => (button as HTMLButtonElement).click());
  await inlineComposer.fill('M3 smoke multi-line review comment');
  await page
    .getByRole('button', { name: 'Add review comment' })
    .evaluate((button) => (button as HTMLButtonElement).click());
  await page.screenshot({
    path: path.join(artifactRoot, '01-multiline-suggestion-compose.png'),
    fullPage: true
  });

  const viewedCheckbox = page.locator('.pr-main input[type="checkbox"]').first();
  await viewedCheckbox.check();
  await expect(viewedCheckbox).toBeChecked();
  await viewedCheckbox.uncheck();
  await expect(viewedCheckbox).not.toBeChecked();
  await page.screenshot({
    path: path.join(artifactRoot, '02-viewed-checkbox-toggle.png'),
    fullPage: true
  });

  await page.locator('.diff-scroll').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(
    page.locator(
      'img[alt="Current assets/test-pattern.png"], img[alt="Previous assets/test-pattern.png"]'
    )
  ).toHaveCount(2);
  await expect(page.getByText('Binary file')).toBeVisible();
  await expect(
    page.locator('.diff-file-row').filter({ hasText: 'src/renamed/old_name.txt' })
  ).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '03-diff-image-binary-rename.png'),
    fullPage: true
  });

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Worktree roots' })).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '04-worktree-roots-panel.png'),
    fullPage: true
  });

  await page.goto('/settings');
  await expect(page.getByRole('heading', { name: 'Notification settings' })).toBeVisible();
  for (const kind of notificationKinds) {
    await page.evaluate(
      async ({ eventKind }) => {
        await window.__NOTIF_DEBUG__?.simulateEvent('github.com:fixture-user', {
          kind: eventKind,
          title: `Synthetic ${eventKind}`,
          body: `Injected ${eventKind} event for smoke`,
          pr_id: 'pr_1',
          repo_id: 'repo_1',
          repo_full_name: 'acme/repo',
          actor_id: `actor-${eventKind}`,
          server_event_id: `smoke-${eventKind}`
        });
      },
      { eventKind: kind }
    );
  }
  await expect(page.getByText('Synthetic review_requested')).toBeVisible();
  await expect(page.getByText('Synthetic mutation_failure')).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '05-notification-trigger-simulations.png'),
    fullPage: true
  });
});

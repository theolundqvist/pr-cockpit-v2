import { expect, test } from '@playwright/test';

test('notification settings issues expected IPC payloads', async ({ page }) => {
  await page.goto('/settings');
  await expect(page.getByRole('heading', { name: 'Notification settings' })).toBeVisible();

  await page.evaluate(() => {
    window.__NOTIF_DEBUG__?.clearInvocations();
  });

  const reviewRequestedToggle = page
    .locator('label', { hasText: 'review_requested' })
    .getByRole('checkbox');
  await reviewRequestedToggle.uncheck();

  const focusToggle = page.getByRole('checkbox', { name: 'Suppress all OS notifications' });
  await focusToggle.check();

  const allowText = page.getByLabel('Allow list (owner/name, comma or whitespace separated)');
  await allowText.fill('acme/repo');
  const denyText = page.getByLabel('Deny list (owner/name, comma or whitespace separated)');
  await denyText.fill('acme/blocked');
  await page.getByRole('button', { name: 'Save repo filters' }).click();

  await page.getByRole('button', { name: 'Save quiet hours' }).click();

  const invocations = await page.evaluate(() => window.__NOTIF_DEBUG__?.invocations() ?? []);
  expect(invocations).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        command: 'set_notification_rule',
        payload: expect.objectContaining({
          kind: 'review_requested',
          enabled: false
        })
      }),
      expect.objectContaining({
        command: 'set_focus_mode',
        payload: expect.objectContaining({ on: true })
      }),
      expect.objectContaining({
        command: 'set_per_repo_filters',
        payload: expect.objectContaining({
          allow: ['acme/repo'],
          deny: ['acme/blocked']
        })
      }),
      expect.objectContaining({
        command: 'set_quiet_hours'
      })
    ])
  );

  await page.evaluate(async () => {
    await window.__NOTIF_DEBUG__?.simulateEvent('github.com:fixture-user', {
      kind: 'mention',
      title: 'Mentioned in a review thread',
      body: '@fixture-user please look',
      pr_id: 'pr_1',
      repo_id: 'repo_1',
      repo_full_name: 'acme/repo',
      actor_id: 'user_2',
      server_event_id: 'comment-1'
    });
  });

  await expect(page.getByText('Mentioned in a review thread')).toBeVisible();
});

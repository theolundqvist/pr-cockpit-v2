import { mkdir } from 'node:fs/promises';
import path from 'node:path';

import { expect, test } from '@playwright/test';

const PRIMARY_ACCOUNT_ID = 'github.com:fixture-user';
const SECONDARY_ACCOUNT_ID = 'github.enterprise.test:octo-enterprise';

test('m4 multi-account UI flow: switcher, badges, composer identity, and rate-limit meter', async ({
  page
}) => {
  const inboxArtifacts = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm4-multi-account');
  const bypassArtifacts = path.resolve(
    process.cwd(),
    '..',
    '..',
    'artifacts',
    'm4-foreground-bypass'
  );
  await mkdir(inboxArtifacts, { recursive: true });
  await mkdir(bypassArtifacts, { recursive: true });

  await page.goto('/');

  await page.getByTestId('account-switcher-trigger').click();
  await page.getByRole('button', { name: /@octo-enterprise/ }).click();
  await expect(page.getByTestId('posting-identity-indicator')).toContainText(
    'Posting as @octo-enterprise'
  );

  await page.getByTestId('account-switcher-trigger').click();
  await page.getByTestId('account-option-all').click();
  await expect(page.getByTestId('inbox-account-badge').first()).toBeVisible();
  await page.screenshot({
    path: path.join(inboxArtifacts, 'inbox-aggregated.png'),
    fullPage: true
  });

  await page
    .getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ })
    .first()
    .click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);

  await page.evaluate(() => {
    window.__M4_MULTI_ACCOUNT_DEBUG__?.clearMutationInvocations();
  });
  await page.getByTestId('composer-posting-identity-select').click();
  await page.screenshot({
    path: path.join(inboxArtifacts, 'composer-identity-dropdown-open.png'),
    fullPage: true
  });
  await page.selectOption('[data-testid="composer-posting-identity-select"]', SECONDARY_ACCOUNT_ID);
  await page.getByTestId('composer-textarea').fill('Posting from secondary identity');
  await page.getByRole('button', { name: 'Add comment' }).click();
  const invocations = await page.evaluate(
    () => window.__M4_MULTI_ACCOUNT_DEBUG__?.mutationInvocations() ?? []
  );
  expect(invocations.length).toBeGreaterThan(0);
  expect(invocations[0]?.payload?.posting_account_id).toBe(SECONDARY_ACCOUNT_ID);

  await page.goto('/');
  await page.getByTestId('account-switcher-trigger').click();
  await page.getByTestId('account-option-all').click();
  await page.evaluate(
    ({ primaryAccountId, secondaryAccountId }) => {
      window.__M4_MULTI_ACCOUNT_DEBUG__?.setRateLimit(primaryAccountId, 'graphql', 500, 5000);
      window.__M4_MULTI_ACCOUNT_DEBUG__?.setRateLimit(primaryAccountId, 'core', 900, 5000);
      window.__M4_MULTI_ACCOUNT_DEBUG__?.setRateLimit(secondaryAccountId, 'graphql', 4200, 5000);
      window.__M4_MULTI_ACCOUNT_DEBUG__?.setRateLimit(secondaryAccountId, 'core', 4200, 5000);
    },
    { primaryAccountId: PRIMARY_ACCOUNT_ID, secondaryAccountId: SECONDARY_ACCOUNT_ID }
  );
  await page.getByTestId('account-switcher-trigger').click();
  await page.getByTestId('account-option-all').click();
  await expect(page.getByTestId('meter-all-accounts')).toBeVisible();

  await page.evaluate(
    (primaryAccountId) =>
      window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitPressure(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await expect(page.getByTestId('meter-throttled-chip')).toBeVisible();
  await page.evaluate(
    (primaryAccountId) => window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitBypass(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await page.waitForTimeout(150);
  await page.evaluate(
    (primaryAccountId) => window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitBypass(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await expect(page.getByTestId('meter-bypass-chip')).toBeVisible({ timeout: 15_000 });

  await page.screenshot({
    path: path.join(bypassArtifacts, '01-low-budget-meter.png'),
    fullPage: true
  });
  await page.evaluate(
    (primaryAccountId) =>
      window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitPressure(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await expect(page.getByTestId('meter-throttled-chip')).toBeVisible();
  await page.screenshot({
    path: path.join(bypassArtifacts, '02-background-throttled.png'),
    fullPage: true
  });
  await page.getByRole('button', { name: /Focus:/ }).click();
  await page.evaluate(
    (primaryAccountId) => window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitBypass(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await page.waitForTimeout(150);
  await page.evaluate(
    (primaryAccountId) => window.__M4_MULTI_ACCOUNT_DEBUG__?.emitRateLimitBypass(primaryAccountId),
    PRIMARY_ACCOUNT_ID
  );
  await expect(page.getByTestId('meter-bypass-chip')).toBeVisible({ timeout: 15_000 });
  await page.screenshot({
    path: path.join(bypassArtifacts, '03-foreground-bypass.png'),
    fullPage: true
  });
});

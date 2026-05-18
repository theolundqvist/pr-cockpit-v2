import { mkdir } from 'node:fs/promises';
import path from 'node:path';

import { expect, test } from '@playwright/test';

test('m4 GHE readiness flow: add account, switch, inbox/detail, endpoint routing proof', async ({
  page
}) => {
  const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm4-ghe');
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/settings');
  await page.evaluate(() => {
    window.__AUTH_DEBUG__?.clearEndpointInvocations();
  });

  await page.getByTestId('add-account-button').click();
  await expect(page.getByTestId('add-account-modal')).toBeVisible();
  await page.getByTestId('ghe-host-link').click();
  await page.getByTestId('add-account-host-input').fill('github.enterprise.test');
  await page.getByTestId('add-account-token-input').fill('ghp_fake_ghe_token_value');
  await page.getByTestId('validate-save-account-button').click();
  await expect(page.getByTestId('add-account-modal')).toHaveCount(0);
  await page.screenshot({
    path: path.join(artifactRoot, '01-add-account.png'),
    fullPage: true
  });

  const endpointCalls = await page.evaluate(
    () => window.__AUTH_DEBUG__?.endpointInvocations() ?? []
  );
  expect(endpointCalls.length).toBeGreaterThan(0);
  const latestCall = endpointCalls[endpointCalls.length - 1];
  expect(latestCall.host).toBe('github.enterprise.test');
  expect(latestCall.api_url).toBe('https://github.enterprise.test/api/v3');
  expect(latestCall.graphql_url).toBe('https://github.enterprise.test/api/graphql');
  expect(latestCall.api_url.includes('api.github.com')).toBe(false);
  expect(latestCall.graphql_url.includes('api.github.com')).toBe(false);

  await page.goto('/');
  await page.getByTestId('account-switcher-trigger').click();
  await page.getByRole('button', { name: /@octo-enterprise/ }).click();
  await expect(page.getByTestId('account-switcher-trigger')).toContainText(
    'github.enterprise.test'
  );
  await expect(page.getByTestId('account-switcher-trigger')).toContainText('GHE');
  await expect(page.getByTestId('posting-identity-indicator')).toContainText(
    'Posting as @octo-enterprise'
  );
  await page.screenshot({
    path: path.join(artifactRoot, '02-ghe-inbox.png'),
    fullPage: true
  });

  await page
    .getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ })
    .first()
    .click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await expect(
    page.getByRole('heading', { name: 'Active Fixture PR Falcon Diff Stress' })
  ).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '03-ghe-pr-detail.png'),
    fullPage: true
  });
});

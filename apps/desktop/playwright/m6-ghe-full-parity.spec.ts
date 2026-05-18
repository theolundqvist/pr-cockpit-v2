import { copyFile, mkdir } from 'node:fs/promises';
import path from 'node:path';

import { expect, test } from '@playwright/test';

const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm6-ghe-full-parity');

test('m6 GHE full parity smoke path', async ({ page }) => {
  await mkdir(artifactRoot, { recursive: true });

  await page.goto('/settings');
  await page.evaluate(() => {
    window.__AUTH_DEBUG__?.clearEndpointInvocations();
    window.__M4_MULTI_ACCOUNT_DEBUG__?.clearMutationInvocations();
  });

  await page.getByRole('button', { name: 'Accounts' }).click();
  await page.getByTestId('add-account-button').click();
  await page.getByTestId('ghe-host-link').click();
  await page.getByTestId('add-account-host-input').fill('github.enterprise.test');
  await page.getByTestId('add-account-token-input').fill('ghp_fake_ghe_token_value');
  await page.getByTestId('validate-save-account-button').click();
  await expect
    .poll(
      async () => page.evaluate(() => window.__AUTH_DEBUG__?.endpointInvocations().length ?? 0),
      { timeout: 15_000 }
    )
    .toBeGreaterThan(0);
  if ((await page.getByTestId('add-account-modal').count()) > 0) {
    await page.keyboard.press('Escape');
  }
  await page.screenshot({
    path: path.join(artifactRoot, '01-add-account.png'),
    fullPage: true
  });

  const endpointCalls = await page.evaluate(
    () => window.__AUTH_DEBUG__?.endpointInvocations() ?? []
  );
  expect(endpointCalls.length).toBeGreaterThan(0);
  const latest = endpointCalls.at(-1);
  expect(latest?.host).toBe('github.enterprise.test');
  expect(latest?.api_url.includes('api.github.com')).toBe(false);
  expect(latest?.graphql_url.includes('api.github.com')).toBe(false);

  await page.goto('/');
  await page.getByTestId('account-switcher-trigger').click();
  await page.getByRole('button', { name: /@octo-enterprise/ }).click();
  await expect(page.getByTestId('account-switcher-trigger')).toContainText(
    'github.enterprise.test'
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
    page.getByRole('heading', { name: /Active Fixture PR Falcon Diff Stress/ })
  ).toBeVisible();
  await page.screenshot({
    path: path.join(artifactRoot, '03-pr-detail.png'),
    fullPage: true
  });

  await page.getByTestId('composer-textarea').fill('m6 ghe parity comment');
  await page.getByRole('button', { name: 'Add comment' }).click();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        (window.__M4_MULTI_ACCOUNT_DEBUG__?.mutationInvocations() ?? []).some(
          (entry) => entry.kind === 'add_comment'
        )
      )
    )
    .toBe(true);
  await page.screenshot({
    path: path.join(artifactRoot, '04-comment-roundtrip.png'),
    fullPage: true
  });

  await page.getByRole('button', { name: 'Apply suggestion' }).first().click();
  await page
    .getByRole('dialog', { name: 'Confirm suggestion apply' })
    .getByRole('button', { name: 'Confirm' })
    .click();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        (window.__M4_MULTI_ACCOUNT_DEBUG__?.mutationInvocations() ?? []).some(
          (entry) => entry.kind === 'apply_suggestion'
        )
      )
    )
    .toBe(true);
  await page.screenshot({
    path: path.join(artifactRoot, '05-suggestion-roundtrip.png'),
    fullPage: true
  });

  await page.getByRole('button', { name: 'Checks' }).click();
  await page
    .locator('li', { hasText: 'lint' })
    .first()
    .getByRole('button', { name: 'Rerun' })
    .click();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        (window.__M4_MULTI_ACCOUNT_DEBUG__?.mutationInvocations() ?? []).some(
          (entry) => entry.kind === 'rerun_check_run'
        )
      )
    )
    .toBe(true);
  await page.screenshot({
    path: path.join(artifactRoot, '06-rerun-roundtrip.png'),
    fullPage: true
  });

  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M6_STACKS_DEBUG__));
  await page.evaluate(async () => {
    await window.__M6_STACKS_DEBUG__?.setStacks(
      'github.enterprise.test:octo-enterprise',
      'repo_2',
      [
        {
          stack_id: 'stack-ghe-parity',
          account_id: 'github.enterprise.test:octo-enterprise',
          repo_id: 'repo_2',
          kind: 'linear',
          warning: null,
          root_pr_id: 'pr_4',
          head_pr_id: 'pr_6',
          detected_at: Math.floor(Date.now() / 1000),
          nodes: [
            {
              pr_id: 'pr_4',
              repo_id: 'repo_2',
              number: 4,
              title: 'GHE stack root',
              state: 'OPEN',
              base_ref: 'main',
              head_ref: 'feature/a',
              base_sha: 'base-4',
              head_sha: 'head-4',
              merge_state_status: 'CLEAN',
              review_state: 'approved',
              checks_state: 'pass',
              blocked_by: [],
              position: 0
            },
            {
              pr_id: 'pr_5',
              repo_id: 'repo_2',
              number: 5,
              title: 'GHE stack mid',
              state: 'OPEN',
              base_ref: 'feature/a',
              head_ref: 'feature/b',
              base_sha: 'base-5',
              head_sha: 'head-5',
              merge_state_status: 'CLEAN',
              review_state: 'pending',
              checks_state: 'pending',
              blocked_by: ['pr_4'],
              position: 1
            },
            {
              pr_id: 'pr_6',
              repo_id: 'repo_2',
              number: 6,
              title: 'GHE stack head',
              state: 'OPEN',
              base_ref: 'feature/b',
              head_ref: 'feature/c',
              base_sha: 'base-6',
              head_sha: 'head-6',
              merge_state_status: 'DIRTY',
              review_state: 'changes_requested',
              checks_state: 'fail',
              blocked_by: ['pr_5'],
              position: 2
            }
          ],
          edges: [
            { parent_pr_id: 'pr_4', child_pr_id: 'pr_5' },
            { parent_pr_id: 'pr_5', child_pr_id: 'pr_6' }
          ]
        }
      ]
    );
  });
  await expect(page.getByTestId('stack-summary-stack-ghe-parity')).toBeVisible();
  await page.getByTestId('stack-summary-stack-ghe-parity').click();
  await expect(page.getByTestId('stack-tree')).toBeVisible();
  await expect(page.getByTestId('stack-tree').getByTestId('stack-row')).toHaveCount(3);
  await page.screenshot({
    path: path.join(artifactRoot, '07-stack-tree.png'),
    fullPage: true
  });

  const videoPath = await page.video()?.path();
  if (videoPath) {
    await copyFile(videoPath, path.join(artifactRoot, 'm6-ghe-full-parity.webm'));
  }
});

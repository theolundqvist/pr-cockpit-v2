import { expect, test, type Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm6-stacks');

async function openInbox(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M6_STACKS_DEBUG__));
  await page.evaluate(() => {
    window.__M6_STACKS_DEBUG__?.clearInvocations();
  });
}

async function expandLinearStack(page: Page): Promise<void> {
  await page.getByTestId('stack-summary-stack-linear-repo2').click();
}

test.beforeAll(async () => {
  await mkdir(artifactRoot, { recursive: true });
});

test('linear stack renders and row navigation opens PR detail', async ({ page }) => {
  await openInbox(page);
  await expandLinearStack(page);

  const stackTree = page.getByTestId('stack-tree');
  await expect(stackTree.getByTestId('stack-row')).toHaveCount(3);
  await stackTree.getByRole('link', { name: /#7/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_7$/);

  await page.screenshot({
    path: path.join(artifactRoot, 'linear-stack.png'),
    fullPage: true
  });
});

test('dag stack renders warning banner', async ({ page }) => {
  await openInbox(page);
  await page.getByTestId('stack-summary-stack-dag-repo2').click();
  await expect(page.getByTestId('stack-dag-warning')).toBeVisible();
});

test('rebase stack happy path streams progress and completes', async ({ page }) => {
  await openInbox(page);
  await expandLinearStack(page);
  await page.getByTestId('stack-actions-toggle-stack-linear-repo2').click();
  await page.getByTestId('rebase-stack-button-stack-linear-repo2').click();
  await expect(page.getByTestId('stack-op-modal')).toBeVisible();

  const opId = await page.evaluate(() => {
    return window.__M6_STACKS_DEBUG__?.latestOpId() ?? null;
  });
  expect(opId).toBeTruthy();

  await page.evaluate(async (id) => {
    await window.__M6_STACKS_DEBUG__?.emitStackOperation({
      id,
      stack_id: 'stack-linear-repo2',
      account_id: 'github.com:fixture-user',
      op_kind: 'rebase',
      status: 'running',
      current_pr_id: 'pr_4',
      current_step: 2,
      total_steps: 3,
      worktree_path: '/tmp/mock-worktree',
      conflict_files: [],
      last_error: null,
      started_at: Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
      finished_at: null
    });
  }, opId);
  await expect(page.getByText(/Step 2\/3/)).toBeVisible();

  await page.evaluate(async (id) => {
    await window.__M6_STACKS_DEBUG__?.emitStackOperation({
      id,
      stack_id: 'stack-linear-repo2',
      account_id: 'github.com:fixture-user',
      op_kind: 'rebase',
      status: 'succeeded',
      current_pr_id: 'pr_1',
      current_step: 3,
      total_steps: 3,
      worktree_path: '/tmp/mock-worktree',
      conflict_files: [],
      last_error: null,
      started_at: Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
      finished_at: Math.floor(Date.now() / 1000)
    });
  }, opId);
  await expect(page.getByTestId('stack-op-modal')).toHaveCount(0);
});

test('rebase conflict panel supports abort action', async ({ page }) => {
  await openInbox(page);
  await expandLinearStack(page);
  await page.getByTestId('stack-actions-toggle-stack-linear-repo2').click();
  await page.getByTestId('rebase-stack-button-stack-linear-repo2').click();
  await expect(page.getByTestId('stack-op-modal')).toBeVisible();

  const opId = await page.evaluate(() => {
    return window.__M6_STACKS_DEBUG__?.latestOpId() ?? null;
  });
  expect(opId).toBeTruthy();

  await page.evaluate(async (id) => {
    await window.__M6_STACKS_DEBUG__?.emitStackOperation({
      id,
      stack_id: 'stack-linear-repo2',
      account_id: 'github.com:fixture-user',
      op_kind: 'rebase',
      status: 'paused_conflict',
      current_pr_id: 'pr_4',
      current_step: 2,
      total_steps: 3,
      worktree_path: '/tmp/mock-worktree',
      conflict_files: ['src/conflict.ts'],
      last_error: 'conflict',
      started_at: Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
      finished_at: null
    });
  }, opId);

  await expect(page.getByTestId('stack-op-conflict')).toBeVisible();
  await page.getByRole('button', { name: 'Abort' }).click();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        (window.__M6_STACKS_DEBUG__?.invocations() ?? []).some(
          (entry) => entry.command === 'abort_stack_op'
        )
      )
    )
    .toBe(true);
});

test('merge stack emits merge and retarget sequence', async ({ page }) => {
  await openInbox(page);
  await expandLinearStack(page);
  await page.getByTestId('stack-actions-toggle-stack-linear-repo2').click();
  await page.getByTestId('merge-stack-button-stack-linear-repo2').click();
  await page.getByRole('button', { name: 'Confirm' }).click();

  const commands = await page.evaluate(() =>
    (window.__M6_STACKS_DEBUG__?.invocations() ?? [])
      .map((entry) => entry.command)
      .filter((command) =>
        ['merge_mutation', 'update_pull_request_base', 'start_merge_stack'].includes(command)
      )
  );
  expect(commands[0]).toBe('start_merge_stack');
  expect(commands).toContain('merge_mutation');
  expect(commands).toContain('update_pull_request_base');
});

test('graphite toggle enables gt badge in stack actions', async ({ page }) => {
  await openInbox(page);
  await page.evaluate(() => {
    window.__M6_STACKS_DEBUG__?.setGraphiteStatus({
      detected_version: { raw: 'gt 1.0.0' },
      enabled: false
    });
  });
  await page.goto('/settings');
  await page.getByRole('button', { name: 'Accounts' }).click();
  await page.getByTestId('graphite-toggle').check();

  await page.goto('/');
  await expandLinearStack(page);
  await page.getByTestId('stack-actions-toggle-stack-linear-repo2').click();
  await expect(page.getByTestId('graphite-enabled-badge')).toBeVisible();
});

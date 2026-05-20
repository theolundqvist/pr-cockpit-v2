import { expect, test, type Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts');
const mergeSurfaceArtifacts = path.join(artifactRoot, 'm4-merge-surface');
const backoffArtifacts = path.join(artifactRoot, 'm4-mergeable-backoff');

async function openFixturePr(page: Page) {
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
}

async function resetMergeFixture(page: Page, partial: Record<string, unknown> = {}) {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M4_DEBUG__));
  const fixturePatch = {
    delete_branch_on_merge_default: false,
    ...partial
  };
  await page.evaluate((value) => {
    window.__M4_DEBUG__?.resetPrDetail();
    window.__M4_DEBUG__?.setPrDetail(value);
    window.__M4_DEBUG__?.clearMutationCalls();
    window.__M4_DEBUG__?.setAutoSettleMutations(true);
  }, fixturePatch);
}

test.beforeAll(async () => {
  await mkdir(mergeSurfaceArtifacts, { recursive: true });
  await mkdir(backoffArtifacts, { recursive: true });
});

test('branch protection variations render expected merge controls', async ({ page }) => {
  const scenarios = [
    {
      name: 'none',
      fixture: {
        branch_protection_summary_json: null,
        viewer_can_merge: true,
        viewer_can_enable_auto_merge: true,
        viewer_can_update_branch: true,
        merge_state_status: 'behind'
      },
      mergeEnabled: true,
      autoMergeEnabled: true,
      updateVisible: true
    },
    {
      name: 'soft',
      fixture: {
        branch_protection_summary_json: JSON.stringify({
          requires_approving_reviews: true,
          required_approving_review_count: 1,
          requires_status_checks: true,
          required_status_check_contexts: ['ci-linux'],
          requires_strict_status_checks: false,
          restricts_pushes: false,
          restricts_review_dismissals: false
        }),
        merge_commit_allowed: true,
        squash_merge_allowed: true,
        rebase_merge_allowed: false,
        viewer_can_merge: true,
        viewer_can_enable_auto_merge: true,
        viewer_can_update_branch: false
      },
      mergeEnabled: true,
      autoMergeEnabled: true,
      updateVisible: false
    },
    {
      name: 'hard',
      fixture: {
        branch_protection_summary_json: JSON.stringify({
          requires_approving_reviews: true,
          required_approving_review_count: 2,
          requires_status_checks: true,
          required_status_check_contexts: ['ci-linux', 'ci-macos'],
          requires_strict_status_checks: true,
          restricts_pushes: true,
          restricts_review_dismissals: true
        }),
        viewer_can_merge: false,
        viewer_can_enable_auto_merge: false,
        viewer_can_update_branch: false
      },
      mergeEnabled: false,
      autoMergeEnabled: false,
      updateVisible: false
    }
  ];

  for (const scenario of scenarios) {
    await resetMergeFixture(page, scenario.fixture);
    await openFixturePr(page);

    const mergeButton = page.getByRole('button', { name: /^Merge/ });
    const autoMergeButton = page.getByRole('button', { name: 'Enable auto-merge' });
    const updateBranchPrimary = page
      .locator('.btn.btn-sm.btn-primary')
      .filter({ hasText: 'Update branch' });
    const rebaseRadio = page.getByLabel('Rebase and merge');

    if (scenario.mergeEnabled) {
      await expect(mergeButton).toBeEnabled();
    } else {
      await expect(mergeButton).toBeDisabled();
    }
    if (scenario.autoMergeEnabled) {
      await expect(autoMergeButton).toBeEnabled();
    } else {
      await expect(autoMergeButton).toBeDisabled();
    }
    if (scenario.updateVisible) {
      await expect(updateBranchPrimary).toHaveCount(1);
    }
    if (scenario.name === 'soft') {
      await expect(rebaseRadio).toBeDisabled();
      await expect(page.getByText('Branch protection:')).toBeVisible();
    }

    await page.screenshot({
      path: path.join(mergeSurfaceArtifacts, `branch-protection-${scenario.name}.png`),
      fullPage: true
    });
  }
});

test('merge confirmation waits for reconcile and submits once', async ({ page }) => {
  await resetMergeFixture(page);
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });
  await openFixturePr(page);

  await page.getByRole('button', { name: /^Merge/ }).click();
  await expect(page.getByRole('dialog', { name: 'Confirm merge' })).toBeVisible();
  await page.getByRole('button', { name: 'Confirm' }).click();

  await expect(page.getByRole('button', { name: /Submitting/ })).toBeVisible();
  await expect(page.locator('.State.State--open')).toBeVisible();

  const call = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls[0] ?? null;
  });
  expect(call?.kind).toBe('merge');

  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, call?.mutation_id);

  await expect(page.getByText('Confirmed')).toBeVisible();
  await expect
    .poll(async () => {
      const calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
      return calls.filter((entry: { kind: string }) => entry.kind === 'merge').length;
    })
    .toBe(1);
});

test('merge and delete branch runs delete only after merge reconcile', async ({ page }) => {
  await resetMergeFixture(page);
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });
  await openFixturePr(page);

  await page.getByRole('checkbox', { name: 'Delete branch after merge' }).check();
  await page.getByRole('button', { name: 'Merge and delete branch' }).click();
  await page.getByRole('button', { name: 'Confirm' }).click();

  let calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
  expect(calls.map((entry: { kind: string }) => entry.kind)).toEqual(['merge']);

  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, calls[0]?.mutation_id);

  await expect
    .poll(async () => {
      const value = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
      return value.map((entry: { kind: string }) => entry.kind).join(',');
    })
    .toBe('merge,delete_head_ref');

  calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, calls[1]?.mutation_id);
});

test('auto-merge enable modal submits expected payload and flips panel', async ({ page }) => {
  await resetMergeFixture(page);
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });
  await openFixturePr(page);

  await page.getByRole('button', { name: 'Enable auto-merge' }).click();
  await expect(page.getByRole('dialog', { name: 'Enable auto-merge' })).toBeVisible();
  await page
    .getByRole('dialog', { name: 'Enable auto-merge' })
    .getByRole('radio', { name: 'Squash and merge' })
    .check();
  await page.getByLabel('Commit headline').fill('Squash title');
  await page.getByLabel('Commit body').fill('Squash body');
  await page
    .getByRole('dialog', { name: 'Enable auto-merge' })
    .getByRole('button', { name: 'Enable auto-merge' })
    .click();

  const call = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls.find((entry) => entry.kind === 'enable_auto_merge') ?? null;
  });
  expect(call?.payload.merge_method).toBe('SQUASH');
  expect(call?.payload.commit_headline).toBe('Squash title');
  expect(call?.payload.commit_body).toBe('Squash body');

  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, call?.mutation_id);

  await expect(page.getByText('Auto-merge enabled by')).toBeVisible();
});

test('merge queue controls enqueue, reorder, and dequeue', async ({ page }) => {
  await resetMergeFixture(page, {
    repo_has_merge_queue: true,
    merge_queue_entry_id: null
  });
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });
  await openFixturePr(page);

  await page.getByRole('button', { name: 'Add to merge queue' }).click();
  await page.getByRole('button', { name: 'Confirm' }).click();
  let calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
  expect(calls[0]?.kind).toBe('enqueue_merge_queue');
  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, calls[0]?.mutation_id);
  await expect(page.getByText('In queue — position')).toBeVisible();

  await page.getByRole('button', { name: 'Move to top' }).click();
  await page.getByRole('button', { name: 'Confirm' }).click();
  calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
  expect(calls[calls.length - 1]?.kind).toBe('reorder_merge_queue');
  await page.evaluate(
    async (mutationId) => {
      await window.__M4_DEBUG__?.settleMutation(mutationId);
    },
    calls[calls.length - 1]?.mutation_id
  );

  await page.getByRole('button', { name: 'Remove from queue' }).click();
  await page.getByRole('button', { name: 'Confirm' }).click();
  calls = await page.evaluate(() => window.__M4_DEBUG__?.mutationCalls() ?? []);
  expect(calls[calls.length - 1]?.kind).toBe('dequeue_merge_queue');
  await page.evaluate(
    async (mutationId) => {
      await window.__M4_DEBUG__?.settleMutation(mutationId);
    },
    calls[calls.length - 1]?.mutation_id
  );

  await expect(page.getByRole('button', { name: 'Add to merge queue' })).toBeVisible();
});

test('update branch transitions through updating and clean', async ({ page }) => {
  await resetMergeFixture(page, {
    merge_state_status: 'behind',
    viewer_can_update_branch: true
  });
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });
  await openFixturePr(page);

  await page.getByRole('button', { name: 'Update branch' }).first().click();
  await expect(page.getByRole('button', { name: /Updating/ })).toBeVisible();

  const call = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls.find((entry) => entry.kind === 'update_branch') ?? null;
  });
  expect(call?.kind).toBe('update_branch');

  await page.evaluate(async (mutationId) => {
    await window.__M4_DEBUG__?.settleMutation(mutationId);
  }, call?.mutation_id);

  await expect(page.getByRole('button', { name: /Updating/ })).toHaveCount(0);
});

test('mergeable null backoff emits expected sequence screenshots', async ({ page }) => {
  await resetMergeFixture(page, { mergeable_state: null });
  await openFixturePr(page);

  const schedule = [2, 5, 15, 45, 120, 300];
  for (const [index, seconds] of schedule.entries()) {
    await page.evaluate(
      async ({ seconds: value, attempt }) => {
        await window.__M4_DEBUG__?.emitMergeableBackoffTick(
          'github.com:fixture-user',
          'pr_1',
          attempt,
          value
        );
      },
      { seconds, attempt: index + 1 }
    );
    await expect(page.getByText(`next check in ${seconds}s`)).toBeVisible();
    await page.screenshot({
      path: path.join(
        backoffArtifacts,
        `tick-${String(index + 1).padStart(3, '0')}-${seconds}s.png`
      ),
      fullPage: true
    });
  }

  await page.evaluate(async () => {
    window.__M4_DEBUG__?.setPrDetail({ mergeable_state: 'clean' });
    await window.__M4_DEBUG__?.emitPrChanged('pr_1');
  });
  await expect(page.getByText('Computing mergeability…')).toHaveCount(0);
  await page.screenshot({
    path: path.join(backoffArtifacts, 'tick-007-resolved.png'),
    fullPage: true
  });
});

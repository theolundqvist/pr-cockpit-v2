import { expect, test, type Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm5-suggestion-apply');

async function openFixturePr(page: Page) {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M4_DEBUG__ && window.__M5_SUGGESTION_DEBUG__));
  await page.evaluate(async () => {
    window.__M4_DEBUG__?.resetPrDetail();
    window.__M4_DEBUG__?.setAutoSettleMutations(true);
    window.__M4_DEBUG__?.clearMutationCalls();
    window.__M5_SUGGESTION_DEBUG__?.resetSuggestions();
    await window.__M5_SUGGESTION_DEBUG__?.setWorktreeDirty(false);
  });
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
}

test.beforeAll(async () => {
  await mkdir(path.join(artifactRoot, 'batch'), { recursive: true });
});

test('single suggestion apply flow', async ({ page }) => {
  await openFixturePr(page);

  await page.getByRole('button', { name: 'Apply suggestion' }).first().click();
  await page
    .getByRole('dialog', { name: 'Confirm suggestion apply' })
    .getByRole('button', { name: 'Confirm' })
    .click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
        return calls.find((entry) => entry.kind === 'apply_suggestion') ?? null;
      })
    )
    .toBeTruthy();
  await page.screenshot({
    path: path.join(artifactRoot, 'single.png'),
    fullPage: true
  });
});

test('batch suggestion apply shows progress steps and push state', async ({ page }) => {
  await openFixturePr(page);
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
  });

  await page.getByRole('button', { name: 'Apply 3' }).click();
  await page.locator('input[type="checkbox"]').nth(2).uncheck();
  await page.getByRole('button', { name: 'Apply selected suggestions' }).click();
  await page
    .getByRole('dialog', { name: 'Confirm batch suggestion apply' })
    .getByRole('button', { name: 'Confirm' })
    .click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
        return calls.find((entry) => entry.kind === 'apply_suggestion_batch') ?? null;
      })
    )
    .toBeTruthy();

  const steps: Array<'opened' | 'assertions_ok' | 'patched' | 'committed' | 'pushed'> = [
    'opened',
    'assertions_ok',
    'patched',
    'committed',
    'pushed'
  ];
  for (const [index, step] of steps.entries()) {
    await page.evaluate(
      async ({ prId, name }) => {
        await window.__M5_SUGGESTION_DEBUG__?.emitWorktreeStep(prId, name);
      },
      { prId: 'pr_1', name: step }
    );
    await expect(
      page.getByRole('dialog', { name: 'Apply suggestions' }).getByText(step, { exact: true })
    ).toBeVisible();
    await page.screenshot({
      path: path.join(artifactRoot, 'batch', `${String(index + 1).padStart(2, '0')}.png`),
      fullPage: true
    });
  }

  const mutationId = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls.find((entry) => entry.kind === 'apply_suggestion_batch')?.mutation_id ?? null;
  });
  await page.evaluate(async (id) => {
    if (id) {
      await window.__M4_DEBUG__?.settleMutation(id);
    }
  }, mutationId);
});

test('dirty worktree blocks batch submission', async ({ page }) => {
  await openFixturePr(page);
  await page.evaluate(async () => {
    await window.__M5_SUGGESTION_DEBUG__?.setWorktreeDirty(true);
  });
  await page.getByRole('button', { name: 'Apply 3' }).click();
  await expect(page.getByText(/Worktree is dirty/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply selected suggestions' })).toBeDisabled();
  await page.screenshot({
    path: path.join(artifactRoot, 'dirty-blocked.png'),
    fullPage: true
  });
});

test('force-with-stash opt-in is sent in payload', async ({ page }) => {
  await openFixturePr(page);
  await page.evaluate(async () => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
    await window.__M5_SUGGESTION_DEBUG__?.setWorktreeDirty(true);
  });
  await page.getByRole('button', { name: 'Apply 3' }).click();
  await page.getByLabel('Force with stash (advanced)').check();
  await page.getByRole('button', { name: 'Apply selected suggestions' }).click();
  await page
    .getByRole('dialog', { name: 'Confirm batch suggestion apply' })
    .getByRole('button', { name: 'Confirm' })
    .click();
  const payload = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls.find((entry) => entry.kind === 'apply_suggestion_batch')?.payload ?? null;
  });
  expect(payload?.force_with_stash).toBe(true);
});

test('head mismatch conflict is surfaced', async ({ page }) => {
  await openFixturePr(page);
  await page.getByRole('button', { name: 'Apply 3' }).click();
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setPrDetail({
      head_sha: 'mismatched_head_sha_000000000000000000000000000000000'
    });
  });
  await page.getByRole('button', { name: 'Apply selected suggestions' }).click();
  await page
    .getByRole('dialog', { name: 'Confirm batch suggestion apply' })
    .getByRole('button', { name: 'Confirm' })
    .click();
  await expect(page.getByText(/conflict/i)).toBeVisible();
});

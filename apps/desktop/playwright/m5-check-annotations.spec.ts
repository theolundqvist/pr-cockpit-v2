import { expect, test, type Locator, type Page } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import path from 'node:path';

const artifactRoot = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm5-check-annotations');

async function openFixturePr(page: Page) {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M4_DEBUG__));
  await page.evaluate(() => {
    window.__M4_DEBUG__?.resetPrDetail();
    window.__M4_DEBUG__?.clearMutationCalls();
    window.__M4_DEBUG__?.setAutoSettleMutations(true);
  });
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
}

async function scrollDiffToAnnotation(page: Page, label: string): Promise<Locator> {
  const chip = page.locator('.check-annotation-chip', { hasText: label }).first();
  const scroll = page.locator('.diff-scroll');
  for (let attempt = 0; attempt < 220; attempt += 1) {
    if (await chip.isVisible()) {
      return chip;
    }
    await scroll.evaluate((element) => {
      element.scrollTop += Math.max(900, element.clientHeight);
    });
    await page.waitForTimeout(25);
  }
  await expect(chip).toBeVisible();
  return chip;
}

async function expandAnnotation(page: Page, label: string): Promise<Locator> {
  const panel = page.locator('.check-annotation-panel').first();
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const chip = await scrollDiffToAnnotation(page, label);
    await chip.dispatchEvent('click');
    if (!(await panel.isVisible())) {
      await chip.click({ force: true });
    }
    if (await panel.isVisible()) {
      return panel;
    }
    await page.waitForTimeout(80);
  }
  await expect(panel).toBeVisible();
  return panel;
}

test.beforeAll(async () => {
  await mkdir(artifactRoot, { recursive: true });
});

test('renders inline annotations and expansion details on diff lines', async ({ page }) => {
  await openFixturePr(page);
  await page.getByRole('button', { name: 'Files' }).click();
  await scrollDiffToAnnotation(page, 'Lint failure');

  const failureChip = page.locator('.check-annotation-chip.tone-failure', {
    hasText: 'Lint failure'
  });
  await expect(failureChip).toBeVisible();

  await scrollDiffToAnnotation(page, 'Deprecated call');
  const warningChip = page.locator('.check-annotation-chip.tone-warning', {
    hasText: 'Deprecated call'
  });
  await expect(warningChip).toBeVisible();

  await scrollDiffToAnnotation(page, 'Coverage note');
  const noticeChip = page.locator('.check-annotation-chip.tone-notice', {
    hasText: 'Coverage note'
  });
  await expect(noticeChip).toBeVisible();

  const failurePanel = await expandAnnotation(page, 'Lint failure');
  await expect(
    failurePanel.getByText('Unexpected any. Please provide a concrete type.')
  ).toBeVisible();
  await failurePanel.locator('summary', { hasText: 'Raw details' }).click({ force: true });
  await expect(failurePanel.locator('pre', { hasText: 'eslint(no-explicit-any)' })).toHaveCount(1);
  await expect(failurePanel.getByRole('button', { name: 'View raw log' })).toBeVisible();

  await page.screenshot({
    path: path.join(artifactRoot, 'diff-line.png'),
    fullPage: true
  });
});

test('streams failed-job log tail and exposes stale badge', async ({ page }) => {
  await openFixturePr(page);
  await page.getByRole('button', { name: 'Files' }).click();
  await scrollDiffToAnnotation(page, 'Coverage note');

  const outdatedChip = page
    .locator('.check-annotation-chip.tone-notice.is-outdated', { hasText: 'Coverage note' })
    .first();
  await expect(outdatedChip).toBeVisible();
  await expect(outdatedChip.getByText('outdated')).toBeVisible();

  await page.screenshot({
    path: path.join(artifactRoot, 'outdated.png'),
    fullPage: true
  });

  const failurePanel = await expandAnnotation(page, 'Lint failure');
  await failurePanel.getByRole('button', { name: 'View raw log' }).dispatchEvent('click');

  const logPanel = page.getByRole('region', { name: 'Check log tail' });
  await expect(logPanel).toBeVisible({ timeout: 15000 });
  await expect(
    logPanel.locator('.Label.Label--secondary', { hasText: 'run_suite_a_1' })
  ).toBeVisible();
  await expect(logPanel.locator('.log-view pre')).toContainText('log line 1 for run_suite_a_1');

  await page.screenshot({
    path: path.join(artifactRoot, 'log-tail.png'),
    fullPage: true
  });
});

test('rerun check-run and rerun-suite dispatch cautious mutations', async ({ page }) => {
  await openFixturePr(page);
  await page.evaluate(() => {
    window.__M4_DEBUG__?.setAutoSettleMutations(false);
    window.__M4_DEBUG__?.clearMutationCalls();
  });

  await page.getByRole('button', { name: 'Checks' }).click();

  const lintRow = page.locator('li', { hasText: 'lint' }).first();
  await lintRow.getByRole('button', { name: 'Rerun' }).click();
  await expect(page.getByText('queued')).toBeVisible();

  await page.getByRole('button', { name: 'Rerun suite' }).first().click();

  await expect
    .poll(async () =>
      page.evaluate(() => {
        const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
        return {
          run: calls.some((entry) => entry.kind === 'rerun_check_run'),
          suite: calls.some((entry) => entry.kind === 'rerun_check_suite')
        };
      })
    )
    .toEqual({ run: true, suite: true });

  const suiteMutationId = await page.evaluate(() => {
    const calls = window.__M4_DEBUG__?.mutationCalls() ?? [];
    return calls.find((entry) => entry.kind === 'rerun_check_suite')?.mutation_id ?? null;
  });
  await page.evaluate(
    async ({ mutationId }) => {
      if (mutationId) {
        await window.__M4_DEBUG__?.settleMutation(mutationId);
      }
      await window.__M4_DEBUG__?.emitPrChanged('pr_1');
    },
    { mutationId: suiteMutationId }
  );
  await expect
    .poll(async () =>
      page.evaluate(() => {
        const labels = Array.from(document.querySelectorAll('li .Label.Label--secondary'));
        return labels.filter((label) => label.textContent?.trim().toLowerCase() === 'queued')
          .length;
      })
    )
    .toBeGreaterThanOrEqual(2);

  await page.screenshot({
    path: path.join(artifactRoot, 'rerun.png'),
    fullPage: true
  });
});

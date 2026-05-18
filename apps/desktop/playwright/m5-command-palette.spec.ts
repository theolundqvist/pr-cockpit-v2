import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const paletteArtifacts = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm5-command-palette');
const keyboardArtifacts = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm5-keyboard-layer');

async function openFixturePr(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M4_MULTI_ACCOUNT_DEBUG__));
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
}

async function openPaletteAndMeasure(page: Page): Promise<number> {
  await page.waitForFunction(() => Boolean(window.__COMMANDS_DEBUG__));
  return page.evaluate(async () => {
    const trigger = () => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'k',
          ctrlKey: true,
          bubbles: true,
          cancelable: true
        })
      );
    };

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const start = performance.now();
      trigger();
      const deadline = performance.now() + 2_000;
      while (performance.now() < deadline) {
        const root = document.querySelector('[data-testid="command-palette-root"]');
        const input = document.querySelector('[data-testid="command-palette-search"]');
        if (
          root instanceof HTMLDivElement &&
          root.classList.contains('is-open') &&
          input instanceof HTMLInputElement &&
          document.activeElement === input
        ) {
          return performance.now() - start;
        }
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
    }

    throw new Error('palette did not open in time');
  });
}

async function filterAndMeasure(page: Page, query: string): Promise<number> {
  return page.evaluate(async (needle) => {
    const input = document.querySelector('[data-testid="command-palette-search"]');
    if (!(input instanceof HTMLInputElement)) {
      throw new Error('missing command palette input');
    }
    const start = performance.now();
    input.value = needle;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    const deadline = performance.now() + 5_000;
    while (performance.now() < deadline) {
      const hit = document.querySelector('[data-testid="command-palette-item-inbox.open"]');
      if (hit) {
        return performance.now() - start;
      }
      await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
    }
    throw new Error('palette filtering timed out');
  }, query);
}

async function runCommand(page: Page, query: string, expectedLabel: string): Promise<void> {
  const search = page.getByTestId('command-palette-search');
  await search.fill(query);
  const row = page.locator('.command-row', { hasText: expectedLabel }).first();
  await expect(row).toBeVisible();
  await row.click();
}

test.beforeAll(async () => {
  await mkdir(paletteArtifacts, { recursive: true });
  await mkdir(keyboardArtifacts, { recursive: true });
});

test('open/filter/run command palette with perf budgets', async ({ page }) => {
  await page.goto('/');
  const openDurationMs = await openPaletteAndMeasure(page);
  await expect(page.getByTestId('command-palette-modal')).toBeVisible();
  await expect(page.getByTestId('command-palette-search')).toBeFocused();
  expect(openDurationMs).toBeLessThan(75);

  const resultDurationMs = await filterAndMeasure(page, 'open inbox');
  expect(resultDurationMs).toBeLessThan(150);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL('/');
  await page.screenshot({
    path: path.join(paletteArtifacts, 'open-filter-run.png'),
    fullPage: true
  });
});

test('sequence shortcuts and input-focus suppression', async ({ page }) => {
  await page.goto('/');
  await page.keyboard.press('g');
  await page.keyboard.press('i');
  await expect(page).toHaveURL('/');

  await page.keyboard.press('g');
  await page.waitForTimeout(600);
  await page.keyboard.press('i');
  await expect(page).toHaveURL('/');

  await openFixturePr(page);
  const textarea = page.getByTestId('composer-textarea');
  await textarea.click();
  await textarea.fill('typing lock');
  await page.keyboard.press('r');
  await expect(textarea).toBeFocused();
  await expect(page.getByTestId('command-palette-root')).not.toHaveClass(/is-open/);
});

test('palette commands trigger suggestion, thread, file, github, and saved-reply flows', async ({
  page
}) => {
  await openFixturePr(page);
  await page.waitForFunction(() => Boolean(window.__COMMANDS_DEBUG__));

  await openPaletteAndMeasure(page);
  await page.evaluate(() => {
    window.__M4_MULTI_ACCOUNT_DEBUG__?.clearMutationInvocations();
  });
  await page.getByTestId('command-palette-search').fill('apply suggestion');
  const applySuggestionButton = page
    .locator('.command-row', { hasText: 'Apply suggestion' })
    .first();
  await expect(applySuggestionButton).toBeVisible();
  await applySuggestionButton.click();
  await page.waitForFunction(() => {
    const applyMutationSeen = window.__M4_MULTI_ACCOUNT_DEBUG__
      ?.mutationInvocations()
      .some(
        (entry) => entry.kind === 'apply_suggestion' || entry.kind === 'apply_suggestion_batch'
      );
    const batchModal = document.querySelector('[data-testid="suggestion-batch-modal"]');
    const batchModalOpen =
      batchModal instanceof HTMLElement && batchModal.getAttribute('aria-hidden') !== 'true';
    return Boolean(applyMutationSeen || batchModalOpen);
  });
  const batchModal = page.getByTestId('suggestion-batch-modal');
  if (!(await batchModal.isVisible().catch(() => false))) {
    await batchModal.waitFor({ state: 'visible', timeout: 2_000 }).catch(() => null);
  }
  if (await batchModal.isVisible().catch(() => false)) {
    await batchModal.getByRole('button', { name: 'Close' }).click();
    await expect(batchModal).toBeHidden();
  }

  await page.evaluate(() => {
    window.__M4_MULTI_ACCOUNT_DEBUG__?.clearMutationInvocations();
  });
  await page.locator('[data-command-thread-id]').first().focus();
  await openPaletteAndMeasure(page);
  await runCommand(page, 'resolve thread', 'Resolve thread');
  await page.waitForFunction(() =>
    window.__M4_MULTI_ACCOUNT_DEBUG__
      ?.mutationInvocations()
      .some((entry) => entry.kind === 'resolve_thread')
  );

  await page.getByRole('button', { name: 'Files' }).click();
  await page.locator('[data-command-file-path]').first().focus();
  await page.evaluate(() => {
    window.__M4_MULTI_ACCOUNT_DEBUG__?.clearMutationInvocations();
  });
  await openPaletteAndMeasure(page);
  await runCommand(page, 'mark file viewed', 'Mark file viewed');
  await page.waitForFunction(() =>
    window.__M4_MULTI_ACCOUNT_DEBUG__
      ?.mutationInvocations()
      .some((entry) => entry.kind === 'mark_file_viewed')
  );

  await page.evaluate(() => {
    window.__COMMANDS_DEBUG__?.clearExternalOpenInvocations();
  });
  await openPaletteAndMeasure(page);
  await runCommand(page, 'open in github', 'Open in github.com');
  await page.waitForFunction(() =>
    (window.__COMMANDS_DEBUG__?.externalOpenInvocations() ?? []).some((value) =>
      value.includes('/pull/')
    )
  );

  await page.getByRole('button', { name: 'Conversation' }).click();
  const textarea = page.getByTestId('composer-textarea');
  await textarea.click();
  await textarea.fill('');
  await openPaletteAndMeasure(page);
  await runCommand(page, 'Insert: Friendly follow-up', 'Insert: Friendly follow-up');
  await expect(textarea).toHaveValue(/Thanks for the update!/);

  await openPaletteAndMeasure(page);
  await runCommand(page, 'open inbox', 'Open inbox');
  await openPaletteAndMeasure(page);
  await runCommand(page, 'open inbox', 'Open inbox');
  await openPaletteAndMeasure(page);
  await expect(page.getByText('Recently used')).toBeVisible();
  await expect(page.locator('.sticky-section + button')).toContainText('Open inbox');
  await page.screenshot({
    path: path.join(paletteArtifacts, 'commands-flows.png'),
    fullPage: true
  });
});

test('keyboard-only PR cycle', async ({ page }) => {
  await page.goto('/');
  await page.evaluate(() => {
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'g', bubbles: true, cancelable: true })
    );
    window.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'p', bubbles: true, cancelable: true })
    );
  });
  await expect(page.getByTestId('command-palette-modal')).toBeVisible();
  await page.getByTestId('command-palette-search').fill('1');
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await page.screenshot({ path: path.join(keyboardArtifacts, '01-open-pr.png'), fullPage: true });

  await page.keyboard.press('j');
  await page.keyboard.press('Control+Shift+R');
  await page.waitForFunction(() =>
    window.__M4_MULTI_ACCOUNT_DEBUG__
      ?.mutationInvocations()
      .some((entry) => entry.kind === 'resolve_thread')
  );
  await page.screenshot({
    path: path.join(keyboardArtifacts, '02-resolve-thread.png'),
    fullPage: true
  });

  await page.keyboard.press('v');
  await page.waitForFunction(() =>
    window.__M4_MULTI_ACCOUNT_DEBUG__
      ?.mutationInvocations()
      .some((entry) => entry.kind === 'mark_file_viewed')
  );
  await page.screenshot({
    path: path.join(keyboardArtifacts, '03-mark-file-viewed.png'),
    fullPage: true
  });

  await page.evaluate(() => {
    window.__COMMANDS_DEBUG__?.clearExternalOpenInvocations();
  });
  await page.keyboard.press('Control+Shift+O');
  await page.waitForFunction(() =>
    (window.__COMMANDS_DEBUG__?.externalOpenInvocations() ?? []).some((value) =>
      value.includes('/pull/')
    )
  );
  await page.screenshot({
    path: path.join(keyboardArtifacts, '04-open-github.png'),
    fullPage: true
  });
});

import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function assertNoCriticalViolations(page: Page, scopeLabel: string): Promise<void> {
  const results = await new AxeBuilder({ page }).analyze();
  const critical = results.violations.filter((violation) => violation.impact === 'critical');
  expect(critical, `${scopeLabel} critical a11y violations`).toEqual([]);
}

test('M5 a11y sweep for inbox, PR detail, composer, palette, suggestion modal, and checks rail', async ({
  page
}) => {
  await page.goto('/');
  await assertNoCriticalViolations(page, 'inbox');

  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await assertNoCriticalViolations(page, 'pr detail');

  await page.getByTestId('composer-saved-replies-button').click();
  await expect(page.getByTestId('composer-saved-replies-dropdown')).toBeVisible();
  await assertNoCriticalViolations(page, 'composer with saved replies');

  await page.keyboard.press('Control+K');
  await expect(page.getByTestId('command-palette-modal')).toBeVisible();
  await assertNoCriticalViolations(page, 'command palette');
  await page.keyboard.press('Escape');

  await page.getByTestId('suggestion-batch-open').click();
  await expect(page.getByTestId('suggestion-batch-modal')).toBeVisible();
  await assertNoCriticalViolations(page, 'suggestion batch modal');
  await page.getByTestId('suggestion-batch-modal').getByRole('button', { name: 'Close' }).click();

  await page.getByRole('button', { name: 'Checks' }).click();
  await expect(page.locator('[data-command-check-suite-id]').first()).toBeVisible();
  await assertNoCriticalViolations(page, 'checks rail');
});

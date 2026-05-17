import { expect, test } from '@playwright/test';

test('pr detail keyboard + accessible naming smoke', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);

  const unnamedInteractive = await page.evaluate(() => {
    const interactive = Array.from(
      document.querySelectorAll<HTMLElement>(
        'button, a[href], input:not([type="hidden"]), select, textarea, summary'
      )
    );
    const failures: string[] = [];
    for (const element of interactive) {
      const style = window.getComputedStyle(element);
      const isVisible =
        style.display !== 'none' &&
        style.visibility !== 'hidden' &&
        element.getClientRects().length > 0;
      if (!isVisible) {
        continue;
      }
      const explicitAria = element.getAttribute('aria-label')?.trim() ?? '';
      const ownText = (element.textContent ?? '').replace(/\s+/g, ' ').trim();
      const placeholder = (element as HTMLInputElement).placeholder?.trim() ?? '';
      const labelledByIds = (element.getAttribute('aria-labelledby') ?? '')
        .split(/\s+/)
        .filter(Boolean);
      const labelledByText = labelledByIds
        .map((id) => document.getElementById(id)?.textContent ?? '')
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
      const htmlLabels =
        'labels' in element ? Array.from((element as HTMLInputElement).labels ?? []) : [];
      const htmlLabelText = htmlLabels
        .map((label) => label.textContent ?? '')
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
      const titleText = element.getAttribute('title')?.trim() ?? '';
      const accessibleText =
        explicitAria || ownText || labelledByText || htmlLabelText || placeholder || titleText;
      if (!accessibleText) {
        const descriptor =
          `${element.tagName.toLowerCase()}` +
          (element.id ? `#${element.id}` : '') +
          (element.className ? `.${element.className.toString().replace(/\s+/g, '.')}` : '');
        failures.push(descriptor);
      }
    }
    return failures;
  });
  expect(unnamedInteractive).toEqual([]);

  const tabOrder = await page.evaluate(() => {
    const tabbable = Array.from(
      document.querySelectorAll<HTMLElement>(
        'a[href], button, input:not([type="hidden"]), select, textarea, [tabindex]:not([tabindex="-1"])'
      )
    ).filter((element) => {
      const style = window.getComputedStyle(element);
      return (
        !element.hasAttribute('disabled') &&
        style.display !== 'none' &&
        style.visibility !== 'hidden' &&
        element.getClientRects().length > 0
      );
    });
    return tabbable.map((element) => {
      const ownText = (element.textContent ?? '').replace(/\s+/g, ' ').trim();
      const aria = element.getAttribute('aria-label')?.trim() ?? '';
      const placeholder = (element as HTMLInputElement).placeholder?.trim() ?? '';
      const labels =
        'labels' in element ? Array.from((element as HTMLInputElement).labels ?? []) : [];
      const labelText = labels
        .map((label) => label.textContent ?? '')
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
      const titleText = element.getAttribute('title')?.trim() ?? '';
      return [aria, ownText, labelText, placeholder, titleText]
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim();
    });
  });
  const indexOf = (needle: string): number => tabOrder.findIndex((value) => value.includes(needle));

  const titleIndex = indexOf('Edit title');
  const descriptionIndex = indexOf('Edit description');
  const timelineIndex = indexOf('Add reaction');
  const composerIndex = indexOf('Comment body');
  const rightRailIndex = indexOf('reviewer login');
  const mergeBoxIndex = indexOf('Enable auto-merge');

  expect(titleIndex).toBeGreaterThanOrEqual(0);
  expect(descriptionIndex).toBeGreaterThanOrEqual(0);
  expect(timelineIndex).toBeGreaterThanOrEqual(0);
  expect(composerIndex).toBeGreaterThanOrEqual(0);
  expect(rightRailIndex).toBeGreaterThanOrEqual(0);
  expect(mergeBoxIndex).toBeGreaterThanOrEqual(0);
  await expect(page.locator('aside.pr-rail')).toBeVisible();
  await expect(page.locator('aside.pr-rail .Box-header', { hasText: 'Reviewers' })).toBeVisible();

  expect(descriptionIndex).toBeGreaterThan(titleIndex);
  expect(composerIndex).toBeGreaterThan(descriptionIndex);
  expect(timelineIndex).toBeGreaterThan(composerIndex);
  expect(rightRailIndex).toBeGreaterThan(composerIndex);
  expect(mergeBoxIndex).toBeGreaterThan(composerIndex);

  const focusButton = page.getByRole('button', { name: /Focus:/ });
  await focusButton.focus();
  const focusVisibleStyle = await focusButton.evaluate((button) => {
    const computed = window.getComputedStyle(button);
    return {
      outlineStyle: computed.outlineStyle,
      outlineWidth: computed.outlineWidth,
      boxShadow: computed.boxShadow
    };
  });
  expect(focusVisibleStyle.outlineStyle).not.toBe('none');
  expect(
    focusVisibleStyle.outlineWidth !== '0px' || focusVisibleStyle.boxShadow !== 'none'
  ).toBeTruthy();
});

import { expect, test } from '@playwright/test';

test('diff polish renders text, image, binary, and rename rows', async ({ page }) => {
  const consoleErrors: string[] = [];
  const httpErrors: string[] = [];
  const allowedMissingAssetPaths = new Set([
    '/assets/grammars/tree-sitter-rust.wasm',
    '/fixture-user.png'
  ]);

  page.on('console', (message) => {
    if (message.type() === 'error') {
      const text = message.text();
      if (text.includes('Failed to load resource: the server responded with a status of 404')) {
        return;
      }
      consoleErrors.push(text);
    }
  });

  page.on('response', (response) => {
    if (response.status() < 400) {
      return;
    }
    const url = new URL(response.url());
    const isAllowedMissingGrammar =
      response.status() === 404 && allowedMissingAssetPaths.has(url.pathname);
    if (!isAllowedMissingGrammar) {
      httpErrors.push(`${response.status()} ${url.pathname}`);
    }
  });

  page.on('requestfailed', (request) => {
    const url = new URL(request.url());
    if (allowedMissingAssetPaths.has(url.pathname)) {
      return;
    }
    httpErrors.push(`failed ${request.failure()?.errorText ?? 'unknown'} ${url.pathname}`);
  });

  await page.goto('/');
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
  await page.getByRole('button', { name: 'Files' }).click();

  await expect(
    page
      .locator('.diff-file-row .text-mono')
      .filter({ hasText: 'src/generated/huge_fixture.rs' })
      .first()
  ).toBeVisible();
  await page.locator('.diff-scroll').evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(
    page.locator('.diff-file-row .text-mono').filter({ hasText: 'assets/test-pattern.png' }).first()
  ).toBeVisible();
  await expect(
    page.locator(
      'img[alt="Current assets/test-pattern.png"], img[alt="Previous assets/test-pattern.png"]'
    )
  ).toHaveCount(2);

  await expect(page.getByText('Binary file')).toBeVisible();
  await expect(
    page.locator('.diff-file-row .text-mono').filter({ hasText: 'assets/header.bin' }).first()
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open on GitHub' }).first()).toBeVisible();

  await expect(
    page.locator('.diff-file-row').filter({ hasText: 'src/renamed/old_name.txt' })
  ).toBeVisible();
  await expect(
    page.locator('.diff-file-row').filter({ hasText: 'src/renamed/new_name.txt' })
  ).toBeVisible();

  expect(consoleErrors).toEqual([]);
  expect(httpErrors).toEqual([]);
});

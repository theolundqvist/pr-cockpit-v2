import { mkdir } from 'node:fs/promises';
import path from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const savedRepliesArtifacts = path.resolve(
  process.cwd(),
  '..',
  '..',
  'artifacts',
  'm5-saved-replies'
);
const pasteImageArtifacts = path.resolve(process.cwd(), '..', '..', 'artifacts', 'm5-paste-image');
const secondaryAccountId = 'github.enterprise.test:octo-enterprise';

async function switchAccount(page: Page, accountId: string): Promise<void> {
  await page.getByTestId('account-switcher-trigger').click();
  await page.getByTestId(`account-option-${accountId}`).click();
}

async function openFixturePr(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.__M5_SAVED_REPLIES_DEBUG__));
  await page.evaluate(() => {
    window.__M5_SAVED_REPLIES_DEBUG__?.resetImageUploadQueue();
    window.__M5_SAVED_REPLIES_DEBUG__?.setSavedReplies('github.com:fixture-user', [
      {
        name: 'Friendly follow-up',
        body: 'Thanks for the update! Could you add one regression test for this path?'
      },
      {
        name: 'Needs clarification',
        body: 'Can you clarify the behavior change in the PR description?'
      }
    ]);
    window.__M5_SAVED_REPLIES_DEBUG__?.clearSavedReplies('github.enterprise.test:octo-enterprise');
  });
  await page.getByRole('link', { name: /Active Fixture PR Falcon Diff Stress/ }).click();
  await expect(page).toHaveURL(/\/pr\/pr_1$/);
}

async function dispatchImagePaste(page: Page): Promise<void> {
  await page.getByTestId('composer-textarea').evaluate((textarea) => {
    const file = new File([new Uint8Array([1, 2, 3, 4, 5])], 'pasted.png', { type: 'image/png' });
    const event = new Event('paste', { bubbles: true, cancelable: true });
    Object.defineProperty(event, 'clipboardData', {
      value: {
        items: [
          {
            type: 'image/png',
            getAsFile: () => file
          }
        ]
      }
    });
    textarea.dispatchEvent(event);
  });
}

test.beforeAll(async () => {
  await mkdir(savedRepliesArtifacts, { recursive: true });
  await mkdir(pasteImageArtifacts, { recursive: true });
});

test('saved replies settings CRUD, reorder, and per-account isolation', async ({ page }) => {
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.__M5_SAVED_REPLIES_DEBUG__));
  await page.evaluate(() => {
    window.__M5_SAVED_REPLIES_DEBUG__?.clearSavedReplies('github.com:fixture-user');
    window.__M5_SAVED_REPLIES_DEBUG__?.clearSavedReplies('github.enterprise.test:octo-enterprise');
  });

  await page.getByTestId('settings-tab-saved-replies').click();
  await expect(page.getByText('No saved replies yet for this account.')).toBeVisible();

  await page.getByRole('button', { name: 'New saved reply' }).click();
  await page.getByTestId('saved-reply-name-input').fill('First reply');
  await page.getByTestId('saved-reply-body-input').fill('First body');
  await page.getByTestId('saved-replies-modal').getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('First reply')).toBeVisible();

  await page.getByRole('button', { name: 'New saved reply' }).click();
  await page.getByTestId('saved-reply-name-input').fill('Second reply');
  await page.getByTestId('saved-reply-body-input').fill('Second body');
  await page.getByTestId('saved-replies-modal').getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Second reply')).toBeVisible();

  await page.getByRole('button', { name: 'Edit' }).first().click();
  await page.getByTestId('saved-reply-name-input').fill('First reply edited');
  await page.getByTestId('saved-replies-modal').getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('First reply edited')).toBeVisible();

  await page.dragAndDrop(
    '[data-testid="saved-replies-settings-list"] li:has-text("Second reply") .saved-reply-drag',
    '[data-testid="saved-replies-settings-list"] li:has-text("First reply edited") .saved-reply-drag'
  );
  await expect(
    page.locator('[data-testid="saved-replies-settings-list"] li').first()
  ).toContainText('Second reply');

  await page
    .locator('[data-testid="saved-replies-settings-list"] li:has-text("First reply edited")')
    .getByRole('button', { name: 'Delete' })
    .click();
  await expect(page.getByText('First reply edited')).not.toBeVisible();

  await switchAccount(page, secondaryAccountId);
  await page.getByTestId('settings-tab-saved-replies').click();
  await expect(page.getByText('No saved replies yet for this account.')).toBeVisible();

  await page.screenshot({
    path: path.join(savedRepliesArtifacts, 'settings.png'),
    fullPage: true
  });
});

test('composer saved replies dropdown, keyboard shortcut, and quick-switch palette insertion', async ({
  page
}) => {
  await openFixturePr(page);
  const textarea = page.getByTestId('composer-textarea');
  await textarea.click();
  await textarea.fill('Hello world');
  await textarea.evaluate((element) => {
    const field = element as HTMLTextAreaElement;
    field.setSelectionRange(6, 6);
  });

  await page.getByTestId('composer-saved-replies-button').click();
  await page.getByTestId('composer-saved-reply-item').first().click();
  await expect(textarea).toHaveValue(
    /Hello Thanks for the update! Could you add one regression test for this path\?world/
  );

  await page.screenshot({
    path: path.join(savedRepliesArtifacts, 'composer-dropdown.png'),
    fullPage: true
  });

  await textarea.click();
  await page.keyboard.press('Control+.');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect(textarea).toHaveValue(/Can you clarify the behavior change in the PR description\?/);

  await textarea.click();
  await page.keyboard.press('Control+Shift+.');
  await expect(page.getByTestId('saved-replies-palette-modal')).toBeVisible();
  await page.getByTestId('saved-replies-palette-search').fill('friendly');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('saved-replies-palette-modal')).not.toBeVisible();
  await expect(textarea).toHaveValue(/Thanks for the update!/);

  await page.screenshot({
    path: path.join(savedRepliesArtifacts, 'palette.png'),
    fullPage: true
  });
});

test('paste image upload success and failure retry/dismiss flow', async ({ page }) => {
  await openFixturePr(page);
  await page.evaluate(() => {
    window.__M5_SAVED_REPLIES_DEBUG__?.resetImageUploadQueue();
    window.__M5_SAVED_REPLIES_DEBUG__?.queueImageUploadSuccess(
      'https://user-images.githubusercontent.com/mock/paste-success.png',
      'pasted-image'
    );
  });

  await dispatchImagePaste(page);
  await expect(page.getByTestId('composer-textarea')).toHaveValue(
    /!\[Uploading image…\]\(pending-/
  );
  await expect(page.getByTestId('composer-textarea')).toHaveValue(
    /!\[pasted-image\]\(https:\/\/user-images\.githubusercontent\.com\/mock\/paste-success\.png\)/
  );

  await page.getByTestId('composer-tab-preview').click();
  await expect(page.getByTestId('composer-preview')).toContainText('pasted-image');
  await page.getByTestId('composer-tab-write').click();

  await page.screenshot({
    path: path.join(pasteImageArtifacts, 'paste-success.png'),
    fullPage: true
  });

  await page.evaluate(() => {
    window.__M5_SAVED_REPLIES_DEBUG__?.queueImageUploadFailure('upload failed');
    window.__M5_SAVED_REPLIES_DEBUG__?.queueImageUploadSuccess(
      'https://user-images.githubusercontent.com/mock/paste-retry.png',
      'retry-image'
    );
  });

  await dispatchImagePaste(page);
  await expect(page.getByTestId('composer-upload-errors')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Dismiss' })).toBeVisible();
  await page.getByRole('button', { name: 'Retry' }).click();
  await expect(page.getByTestId('composer-textarea')).toHaveValue(
    /!\[retry-image\]\(https:\/\/user-images\.githubusercontent\.com\/mock\/paste-retry\.png\)/
  );
  await expect(page.getByTestId('composer-upload-errors')).not.toBeVisible();

  await page.evaluate(() => {
    window.__M5_SAVED_REPLIES_DEBUG__?.queueImageUploadFailure('dismiss me');
  });
  await dispatchImagePaste(page);
  await expect(page.getByTestId('composer-upload-errors')).toBeVisible();
  await page.getByRole('button', { name: 'Dismiss' }).click();
  await expect(page.getByTestId('composer-upload-errors')).not.toBeVisible();

  await page.screenshot({
    path: path.join(pasteImageArtifacts, 'paste-failure.png'),
    fullPage: true
  });
});

import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

const { submitMutationMock } = vi.hoisted(() => ({
  submitMutationMock: vi.fn()
}));

vi.mock('$lib/ipc/client', async () => {
  const actual = await vi.importActual<object>('$lib/ipc/client');
  return {
    ...actual,
    listDrafts: vi.fn(async () => []),
    saveDraft: vi.fn(
      async (input: {
        id?: string;
        account_id: string;
        target_type: string;
        target_id: string;
        body: string;
      }) => ({
        id: input.id ?? 'draft-mock',
        account_id: input.account_id,
        target_type: input.target_type,
        target_id: input.target_id,
        body: input.body,
        created_at: 1,
        updated_at: 1
      })
    ),
    deleteDraft: vi.fn(async () => {}),
    renderPreview: vi.fn(async (body: string) => ({
      html: `<p>${body}</p>`,
      cache_hit: false,
      content_hash: 'mock',
      cache_key: 'mock',
      renderer_version: 'mock'
    })),
    submitMutation: submitMutationMock.mockImplementation(async (...args: unknown[]) => {
      void args;
      return {
        mutation_id: 'm-quick-switch',
        deduped: false,
        requires_confirmation: false,
        optimism_level: 'full',
        projected_changes: ['add_comment']
      };
    }),
    submitReviewComment: vi.fn(async () => ({
      mutation_id: 'm-review',
      deduped: false,
      requires_confirmation: false,
      optimism_level: 'full',
      projected_changes: ['add_review_comment']
    }))
  };
});

describe('Composer posting identity quick-switch', () => {
  afterEach(() => {
    submitMutationMock.mockClear();
  });

  it('threads selected posting identity into submit_mutation payload', async () => {
    const { activeAccountIdStore, accountsStore } = await import('$lib/state/cockpit');
    accountsStore.set([
      {
        host: 'github.com',
        login: 'fixture-user',
        token_kind: 'pat',
        scopes: ['repo'],
        created_at: 1,
        updated_at: 1,
        is_active: true
      },
      {
        host: 'github.enterprise.test',
        login: 'octo-enterprise',
        token_kind: 'oauth-device',
        scopes: ['repo'],
        created_at: 1,
        updated_at: 1,
        is_active: false
      }
    ]);
    activeAccountIdStore.set('github.com:fixture-user');

    const Composer = (await import('./Composer.svelte')).default;
    render(Composer, {
      accountId: 'github.com:fixture-user',
      targetType: 'pr_comment',
      targetId: 'pr_1',
      submitKind: 'add_comment',
      payloadBase: { pr_id: 'pr_1' },
      submitLabel: 'Add comment'
    });

    await fireEvent.change(screen.getByTestId('composer-posting-identity-select'), {
      target: { value: 'github.enterprise.test:octo-enterprise' }
    });
    await fireEvent.input(screen.getByTestId('composer-textarea'), {
      target: { value: 'Ship it from enterprise identity' }
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Add comment' }));

    expect(submitMutationMock).toHaveBeenCalledTimes(1);
    const firstCall = submitMutationMock.mock.calls[0];
    expect(firstCall).toBeDefined();
    const payloadJson = firstCall?.[2];
    expect(typeof payloadJson).toBe('string');
    const payload = JSON.parse(payloadJson as string) as Record<string, string>;
    expect(payload.posting_account_id).toBe('github.enterprise.test:octo-enterprise');
  });
});

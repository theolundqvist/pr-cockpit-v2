import { describe, expect, it, vi } from 'vitest';

vi.mock('$lib/ipc/client', async () => {
  const actual = await vi.importActual<object>('$lib/ipc/client');
  return {
    ...actual,
    renderPreview: vi.fn(async (body: string) => ({
      html: `<p>${body}</p>`,
      cache_hit: false,
      content_hash: 'mock',
      cache_key: 'mock',
      renderer_version: 'mock'
    })),
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
    submitMutation: vi.fn(async () => ({
      mutation_id: 'm-fast',
      deduped: false,
      requires_confirmation: false,
      optimism_level: 'full',
      projected_changes: ['add_comment', 'body']
    }))
  };
});

describe('Composer optimistic submit timing', () => {
  it('keeps optimistic projection render under 16ms', async () => {
    const { submitComposerMutation } = await import('$lib/components/composer-model');
    const result = await submitComposerMutation('github.com:fixture-user', 'add_comment', {
      pr_id: 'pr_1',
      target_id: 'pr_1',
      body: 'hello world'
    });
    expect(result.elapsedMs).toBeLessThan(16);
    expect(result.submission.projected_changes.length).toBeGreaterThan(0);
  });
});

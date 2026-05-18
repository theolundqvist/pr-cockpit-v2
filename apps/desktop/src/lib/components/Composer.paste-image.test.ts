import { fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

const { uploadImageMock } = vi.hoisted(() => ({
  uploadImageMock: vi.fn()
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
    submitMutation: vi.fn(async () => ({
      mutation_id: 'm-paste',
      deduped: false,
      requires_confirmation: false,
      optimism_level: 'full',
      projected_changes: ['add_comment']
    })),
    listSavedReplies: vi.fn(async () => []),
    uploadImageToGithubUserContent: uploadImageMock
  };
});

function imageClipboardData(file: File) {
  return {
    items: [
      {
        type: file.type,
        getAsFile: () => file
      }
    ]
  };
}

describe('Composer paste-image flow', () => {
  afterEach(() => {
    uploadImageMock.mockReset();
  });

  it('inserts upload placeholder then replaces with markdown image link', async () => {
    const resolverRef: { current?: (value: unknown) => void } = {};
    uploadImageMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          resolverRef.current = resolve;
        })
    );

    const Composer = (await import('./Composer.svelte')).default;
    render(Composer, {
      accountId: 'github.com:fixture-user',
      targetType: 'pr_comment',
      targetId: 'pr_1',
      submitKind: 'add_comment',
      payloadBase: { pr_id: 'pr_1' },
      submitLabel: 'Add comment'
    });

    const textarea = screen.getByTestId('composer-textarea') as HTMLTextAreaElement;
    textarea.focus();
    await fireEvent.input(textarea, { target: { value: 'Before ' } });
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);

    const file = new File([new Uint8Array([1, 2, 3, 4])], 'paste.png', { type: 'image/png' });
    await fireEvent.paste(textarea, { clipboardData: imageClipboardData(file) });

    await vi.waitFor(() => {
      expect(textarea.value).toContain('![Uploading image…](pending-');
    });
    expect(uploadImageMock).toHaveBeenCalledTimes(1);

    if (!resolverRef.current) {
      throw new Error('upload resolver not initialized');
    }
    resolverRef.current({
      url: 'https://user-images.githubusercontent.com/mock/paste-success.png',
      alt: 'pasted-image',
      content_hash: 'hash',
      size_bytes: 4
    });

    await vi.waitFor(() => {
      expect(textarea.value).toContain(
        '![pasted-image](https://user-images.githubusercontent.com/mock/paste-success.png)'
      );
      expect(textarea.value).not.toContain('pending-');
    });
  });

  it('shows retry + dismiss chip when upload fails', async () => {
    uploadImageMock.mockRejectedValueOnce(new Error('upload failed')).mockResolvedValueOnce({
      url: 'https://user-images.githubusercontent.com/mock/retried.png',
      alt: 'retry-image',
      content_hash: 'hash-2',
      size_bytes: 4
    });

    const Composer = (await import('./Composer.svelte')).default;
    render(Composer, {
      accountId: 'github.com:fixture-user',
      targetType: 'pr_comment',
      targetId: 'pr_1',
      submitKind: 'add_comment',
      payloadBase: { pr_id: 'pr_1' },
      submitLabel: 'Add comment'
    });

    const textarea = screen.getByTestId('composer-textarea') as HTMLTextAreaElement;
    textarea.focus();
    const file = new File([new Uint8Array([9, 8, 7, 6])], 'retry.png', { type: 'image/png' });
    await fireEvent.paste(textarea, { clipboardData: imageClipboardData(file) });

    await vi.waitFor(() => {
      expect(screen.getByTestId('composer-upload-errors')).toBeTruthy();
      expect(screen.getByText(/upload failed/i)).toBeTruthy();
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Retry' }));

    await vi.waitFor(() => {
      expect(textarea.value).toContain(
        '![retry-image](https://user-images.githubusercontent.com/mock/retried.png)'
      );
      expect(screen.queryByTestId('composer-upload-errors')).toBeNull();
    });
  });
});

<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from 'svelte';

  import { activeAccountIdStore, accountsStore } from '$lib/state/cockpit';
  import {
    deleteDraft,
    listDrafts,
    listSavedReplies,
    saveDraft,
    toAccountId,
    uploadImageToGithubUserContent
  } from '$lib/ipc/client';
  import type { MutationKind, SavedReply, SubmittedMutation } from '$lib/ipc/bindings';
  import {
    insertSuggestionBlock,
    insertTextAtSelection,
    renderComposerPreview,
    submitComposerMutation
  } from '$lib/components/composer-model';
  import {
    registerComposerFocusTarget,
    setFocusedComposerId
  } from '$lib/components/saved-replies/composerFocusRegistry';
  import { openSavedRepliesPalette } from '$lib/components/saved-replies/palette-state';

  export let accountId: string;
  export let targetType = 'comment';
  export let targetId: string;
  export let repo: string | null = null;
  export let submitKind: MutationKind = 'add_comment';
  export let payloadBase: Record<string, unknown> = {};
  export let placeholder = 'Write a comment';
  export let submitLabel = 'Submit';
  export let textareaAriaLabel = 'Comment body';
  export let disabled = false;
  export let initialBody = '';
  export let suggestionSeedLines: string[] = [];

  type UploadErrorChip = {
    id: string;
    message: string;
    mime: string;
    bytes: number[];
  };

  const dispatch = createEventDispatcher<{
    submitted: { submission: SubmittedMutation; elapsedMs: number; body: string };
  }>();

  const composerInstanceId = `composer-${Math.random().toString(16).slice(2, 10)}`;
  let composerElement: HTMLDivElement | null = null;
  let textareaElement: HTMLTextAreaElement | null = null;
  let body = '';
  let tab: 'write' | 'preview' = 'write';
  let previewHtml = '';
  let previewLoading = false;
  let submitLoading = false;
  let draftId: string | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let submission: SubmittedMutation | null = null;
  let hydratedInitialBody = false;
  let postingAccountId: string | null = accountId;
  let postingIdentityTouched = false;
  let savedReplies: SavedReply[] = [];
  let savedRepliesLoading = false;
  let savedRepliesOpen = false;
  let savedRepliesError = '';
  let savedRepliesCursor = 0;
  let loadedSavedRepliesFor: string | null = null;
  let uploadErrors: UploadErrorChip[] = [];
  let unregisterFocusTarget: (() => void) | null = null;

  $: postingCandidates = $accountsStore.map((account) => ({
    id: toAccountId(account),
    login: account.login,
    host: account.host
  }));
  $: if (!postingIdentityTouched) {
    postingAccountId = $activeAccountIdStore ?? accountId;
  }
  $: resolvedPostingAccountId = postingAccountId ?? accountId;
  $: if (tab === 'preview') {
    void updatePreview();
  }
  $: if (body !== undefined) {
    queueAutosave();
  }
  $: if (accountId && loadedSavedRepliesFor !== accountId) {
    loadedSavedRepliesFor = accountId;
    void refreshSavedReplies();
  }
  $: if (savedRepliesCursor >= savedReplies.length) {
    savedRepliesCursor = Math.max(savedReplies.length - 1, 0);
  }

  onMount(async () => {
    unregisterFocusTarget = registerComposerFocusTarget({
      id: composerInstanceId,
      insertText: (text) => insertAtCursor(text)
    });
    document.addEventListener('mousedown', onDocumentMouseDown);

    const drafts = await listDrafts({
      account_id: accountId,
      target_type: targetType,
      target_id: targetId
    });
    if (drafts.length > 0) {
      draftId = drafts[0]?.id ?? null;
      body = drafts[0]?.body ?? '';
      hydratedInitialBody = true;
      return;
    }
    if (!hydratedInitialBody && initialBody.trim().length > 0) {
      body = initialBody;
      hydratedInitialBody = true;
    }
  });

  onDestroy(() => {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
    unregisterFocusTarget?.();
    unregisterFocusTarget = null;
    if (textareaElement && document.activeElement === textareaElement) {
      setFocusedComposerId(null);
    }
    document.removeEventListener('mousedown', onDocumentMouseDown);
  });

  async function refreshSavedReplies(): Promise<void> {
    savedRepliesLoading = true;
    savedRepliesError = '';
    try {
      savedReplies = await listSavedReplies(accountId);
    } catch (error) {
      savedRepliesError = error instanceof Error ? error.message : 'Failed to load saved replies.';
      savedReplies = [];
    } finally {
      savedRepliesLoading = false;
      savedRepliesCursor = 0;
    }
  }

  async function updatePreview(): Promise<void> {
    previewLoading = true;
    previewHtml = await renderComposerPreview(body, repo);
    previewLoading = false;
  }

  function queueAutosave(): void {
    if (saveTimer) {
      clearTimeout(saveTimer);
    }
    saveTimer = setTimeout(() => {
      void persistDraft();
    }, 1500);
  }

  async function persistDraft(): Promise<void> {
    if (!body.trim()) {
      if (draftId) {
        await deleteDraft(draftId);
        draftId = null;
      }
      return;
    }
    const saved = await saveDraft({
      id: draftId,
      account_id: accountId,
      target_type: targetType,
      target_id: targetId,
      body
    });
    draftId = saved.id;
  }

  async function onSubmit(): Promise<void> {
    submitLoading = true;
    const payload: Record<string, unknown> = {
      ...payloadBase,
      body,
      target_id: targetId
    };
    if (resolvedPostingAccountId) {
      payload.posting_account_id = resolvedPostingAccountId;
    }
    if (!('pr_id' in payload) && targetId.startsWith('pr_')) {
      payload.pr_id = targetId;
    }
    const result = await submitComposerMutation(accountId, submitKind, payload);
    submission = result.submission;
    dispatch('submitted', { submission: result.submission, elapsedMs: result.elapsedMs, body });
    body = '';
    if (draftId) {
      await deleteDraft(draftId);
      draftId = null;
    }
    submitLoading = false;
  }

  function insertAtCursor(text: string): void {
    const element = textareaElement;
    const start = element?.selectionStart ?? body.length;
    const end = element?.selectionEnd ?? start;
    const next = insertTextAtSelection(body, text, start, end);
    body = next.nextBody;
    queueMicrotask(() => {
      if (!textareaElement) {
        return;
      }
      textareaElement.focus();
      textareaElement.setSelectionRange(next.nextCaret, next.nextCaret);
    });
  }

  function onInsertSuggestion(): void {
    const element = textareaElement;
    const start = element?.selectionStart ?? body.length;
    const end = element?.selectionEnd ?? start;
    const next = insertSuggestionBlock(body, suggestionSeedLines, start, end);
    body = next.nextBody;
    queueMicrotask(() => {
      if (!textareaElement) {
        return;
      }
      textareaElement.focus();
      textareaElement.setSelectionRange(next.nextCaret, next.nextCaret);
    });
  }

  function onPostingIdentityChange(event: Event): void {
    postingIdentityTouched = true;
    const value = (event.currentTarget as HTMLSelectElement).value;
    postingAccountId = value.length > 0 ? value : accountId;
  }

  function onTextareaFocus(): void {
    setFocusedComposerId(composerInstanceId);
  }

  function onTextareaBlur(): void {
    if (savedRepliesOpen) {
      return;
    }
  }

  function openSavedRepliesDropdown(): void {
    savedRepliesOpen = true;
    savedRepliesCursor = 0;
    if (savedReplies.length === 0 && !savedRepliesLoading) {
      void refreshSavedReplies();
    }
  }

  function closeSavedRepliesDropdown(): void {
    savedRepliesOpen = false;
  }

  function insertSavedReply(reply: SavedReply): void {
    insertAtCursor(reply.body);
    closeSavedRepliesDropdown();
  }

  function onDocumentMouseDown(event: MouseEvent): void {
    if (!savedRepliesOpen || !composerElement) {
      return;
    }
    const target = event.target as Node | null;
    if (target && composerElement.contains(target)) {
      return;
    }
    closeSavedRepliesDropdown();
  }

  function onTextareaKeydown(event: KeyboardEvent): void {
    if (event.ctrlKey && event.shiftKey && event.key === '.') {
      event.preventDefault();
      openSavedRepliesPalette();
      return;
    }
    if (event.ctrlKey && !event.shiftKey && event.key === '.') {
      event.preventDefault();
      openSavedRepliesDropdown();
      return;
    }
    if (!savedRepliesOpen) {
      return;
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      closeSavedRepliesDropdown();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (savedReplies.length === 0) {
        return;
      }
      savedRepliesCursor = (savedRepliesCursor + 1) % savedReplies.length;
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (savedReplies.length === 0) {
        return;
      }
      savedRepliesCursor = (savedRepliesCursor - 1 + savedReplies.length) % savedReplies.length;
      return;
    }
    if (event.key === 'Enter') {
      const selected = savedReplies[savedRepliesCursor];
      if (!selected) {
        return;
      }
      event.preventDefault();
      insertSavedReply(selected);
    }
  }

  function createUploadToken(): string {
    const random = Math.random().toString(16).slice(2, 10);
    return `${Date.now()}-${random}`;
  }

  async function uploadImageBytes(bytes: number[], mime: string): Promise<void> {
    const token = createUploadToken();
    const placeholder = `![Uploading image…](pending-${token})`;
    insertAtCursor(placeholder);
    try {
      const uploaded = await uploadImageToGithubUserContent(accountId, bytes, mime);
      const alt = uploaded.alt && uploaded.alt.length > 0 ? uploaded.alt : 'pasted-image';
      body = body.replace(placeholder, `![${alt}](${uploaded.url})`);
    } catch (error) {
      body = body.replace(placeholder, '');
      uploadErrors = [
        ...uploadErrors,
        {
          id: token,
          message: error instanceof Error ? error.message : 'Image upload failed.',
          mime,
          bytes
        }
      ];
    }
  }

  async function handleImageFile(file: File): Promise<void> {
    const mime = file.type || 'image/png';
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    await uploadImageBytes(bytes, mime);
  }

  async function onTextareaPaste(event: ClipboardEvent): Promise<void> {
    const items = Array.from(event.clipboardData?.items ?? []);
    const imageItem = items.find((item) => item.type.toLowerCase().startsWith('image/'));
    if (!imageItem) {
      return;
    }
    const file = imageItem.getAsFile();
    if (!file) {
      return;
    }
    event.preventDefault();
    await handleImageFile(file);
  }

  function onTextareaDragOver(event: DragEvent): void {
    const hasImage = Array.from(event.dataTransfer?.items ?? []).some((item) =>
      item.type.toLowerCase().startsWith('image/')
    );
    if (hasImage) {
      event.preventDefault();
    }
  }

  async function onTextareaDrop(event: DragEvent): Promise<void> {
    const files = Array.from(event.dataTransfer?.files ?? []).filter((file) =>
      file.type.toLowerCase().startsWith('image/')
    );
    if (files.length === 0) {
      return;
    }
    event.preventDefault();
    for (const file of files) {
      await handleImageFile(file);
    }
  }

  async function retryUpload(errorChip: UploadErrorChip): Promise<void> {
    uploadErrors = uploadErrors.filter((entry) => entry.id !== errorChip.id);
    await uploadImageBytes(errorChip.bytes, errorChip.mime);
  }

  function dismissUploadError(id: string): void {
    uploadErrors = uploadErrors.filter((entry) => entry.id !== id);
  }
</script>

<div class="Box composer" bind:this={composerElement}>
  <div class="Box-header d-flex flex-items-center gap-2">
    <div class="BtnGroup">
      <button
        class={`btn btn-sm ${tab === 'write' ? 'selected' : ''}`}
        type="button"
        data-testid="composer-tab-write"
        on:click={() => (tab = 'write')}
      >
        Write
      </button>
      <button
        class={`btn btn-sm ${tab === 'preview' ? 'selected' : ''}`}
        type="button"
        data-testid="composer-tab-preview"
        on:click={() => (tab = 'preview')}
      >
        Preview
      </button>
    </div>
    <button
      class="btn btn-sm"
      type="button"
      on:click={openSavedRepliesDropdown}
      data-testid="composer-saved-replies-button"
    >
      <span class="octicon octicon-comment-discussion" aria-hidden="true"></span>
      Saved replies
    </button>
    <span class="f6 color-fg-muted">Autosaves every 1.5s · Ctrl+. · Ctrl+Shift+.</span>
  </div>
  <div class="Box-body">
    {#if tab === 'write'}
      <div class="mb-2">
        <label class="f6 text-bold d-block mb-1" for="composer-posting-identity">Posting identity</label>
        <select
          id="composer-posting-identity"
          class="form-select width-full"
          value={resolvedPostingAccountId}
          on:change={onPostingIdentityChange}
          data-testid="composer-posting-identity-select"
        >
          {#each postingCandidates as candidate}
            <option value={candidate.id}>@{candidate.login} · {candidate.host}</option>
          {/each}
        </select>
      </div>
      <div class="mb-2 d-flex flex-items-center flex-wrap gap-1">
        <button
          class="btn btn-sm"
          type="button"
          data-testid="composer-insert-suggestion"
          on:click={onInsertSuggestion}
        >
          [+ Suggestion]
        </button>
      </div>
      {#if savedRepliesOpen}
        <div class="saved-replies-dropdown Box mb-2" data-testid="composer-saved-replies-dropdown">
          {#if savedRepliesLoading}
            <p class="f6 color-fg-muted m-2">Loading saved replies…</p>
          {:else if savedRepliesError}
            <p class="f6 color-fg-danger m-2">{savedRepliesError}</p>
          {:else if savedReplies.length === 0}
            <p class="f6 color-fg-muted m-2">No saved replies yet for this account.</p>
          {:else}
            <ul class="list-style-none m-0 p-0">
              {#each savedReplies as reply, index (reply.id)}
                <li>
                  <button
                    class={`saved-reply-option btn-link width-full text-left ${index === savedRepliesCursor ? 'is-active' : ''}`}
                    type="button"
                    data-testid="composer-saved-reply-item"
                    on:mouseenter={() => (savedRepliesCursor = index)}
                    on:click={() => insertSavedReply(reply)}
                  >
                    <span class="text-bold">{reply.name}</span>
                    <span class="f6 color-fg-muted text-truncate">{reply.body.replace(/\n/g, ' ')}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
      <textarea
        bind:this={textareaElement}
        class="form-control width-full"
        rows={5}
        bind:value={body}
        placeholder={placeholder}
        aria-label={textareaAriaLabel}
        data-testid="composer-textarea"
        on:focus={onTextareaFocus}
        on:blur={onTextareaBlur}
        on:keydown={onTextareaKeydown}
        on:paste={onTextareaPaste}
        on:dragover={onTextareaDragOver}
        on:drop={onTextareaDrop}
      ></textarea>
      {#if uploadErrors.length > 0}
        <div class="mt-2 d-flex flex-column gap-1" data-testid="composer-upload-errors">
          {#each uploadErrors as uploadError (uploadError.id)}
            <div class="upload-error-chip">
              <span class="f6 color-fg-danger">{uploadError.message}</span>
              <button class="btn btn-sm" type="button" on:click={() => retryUpload(uploadError)}>Retry</button>
              <button class="btn btn-sm" type="button" on:click={() => dismissUploadError(uploadError.id)}>
                Dismiss
              </button>
            </div>
          {/each}
        </div>
      {/if}
    {:else if previewLoading}
      <p class="f6 color-fg-muted mb-0">Rendering preview…</p>
    {:else}
      <article class="markdown-body" data-testid="composer-preview">
        {@html previewHtml}
      </article>
    {/if}

    {#if submission}
      <div class="mt-2 d-flex flex-wrap gap-1" data-testid="composer-projection">
        <span class="Label Label--secondary">optimism: {submission.optimism_level}</span>
        {#each submission.projected_changes as change}
          <span class="Label">{change}</span>
        {/each}
      </div>
    {/if}

    <div class="mt-2 d-flex flex-justify-end">
      <button
        class="btn btn-primary"
        type="button"
        on:click={onSubmit}
        disabled={disabled || submitLoading || body.trim().length === 0}
      >
        {submitLoading ? 'Submitting…' : submitLabel}
      </button>
    </div>
  </div>
</div>

<style>
  .saved-replies-dropdown {
    max-height: 220px;
    overflow: auto;
  }

  .saved-reply-option {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px;
    border: 1px solid transparent;
    border-radius: 6px;
  }

  .saved-reply-option.is-active {
    border-color: var(--borderColor-accent-emphasis, #0969da);
    background: var(--bgColor-accent-muted, #ddf4ff);
  }

  .upload-error-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid var(--borderColor-danger-emphasis, #d1242f);
    border-radius: 999px;
    padding: 4px 10px;
    background: var(--bgColor-danger-muted, #ffebe9);
    width: fit-content;
  }
</style>

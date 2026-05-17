<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from 'svelte';

  import type { MutationKind, SubmittedMutation } from '$lib/ipc/bindings';
  import { deleteDraft, listDrafts, saveDraft } from '$lib/ipc/client';
  import {
    insertSuggestionBlock,
    renderComposerPreview,
    submitComposerMutation
  } from '$lib/components/composer-model';

  export let accountId: string;
  export let targetType = 'comment';
  export let targetId: string;
  export let repo: string | null = null;
  export let submitKind: MutationKind = 'add_comment';
  export let payloadBase: Record<string, unknown> = {};
  export let placeholder = 'Write a comment';
  export let submitLabel = 'Submit';
  export let disabled = false;
  export let initialBody = '';
  export let suggestionSeedLines: string[] = [];

  const dispatch = createEventDispatcher<{
    submitted: { submission: SubmittedMutation; elapsedMs: number; body: string };
  }>();

  let body = '';
  let textareaElement: HTMLTextAreaElement | null = null;
  let tab: 'write' | 'preview' = 'write';
  let previewHtml = '';
  let previewLoading = false;
  let submitLoading = false;
  let draftId: string | null = null;
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let submission: SubmittedMutation | null = null;
  let hydratedInitialBody = false;

  onMount(async () => {
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
  });

  $: if (tab === 'preview') {
    void updatePreview();
  }

  $: if (body !== undefined) {
    queueAutosave();
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

  function onInsertSuggestion(): void {
    const element = textareaElement;
    if (!element) {
      const fallback = insertSuggestionBlock(body, suggestionSeedLines, body.length, body.length);
      body = fallback.nextBody;
      return;
    }
    const start = element.selectionStart ?? body.length;
    const end = element.selectionEnd ?? start;
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
</script>

<div class="Box composer">
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
    <span class="f6 color-fg-muted">Autosaves every 1.5s</span>
  </div>
  <div class="Box-body">
    {#if tab === 'write'}
      <div class="mb-2 d-flex flex-items-center flex-wrap gap-1">
        <button class="btn btn-sm" type="button" data-testid="composer-insert-suggestion" on:click={onInsertSuggestion}>
          [+ Suggestion]
        </button>
      </div>
      <textarea
        bind:this={textareaElement}
        class="form-control width-full"
        rows={5}
        bind:value={body}
        placeholder={placeholder}
        data-testid="composer-textarea"
      ></textarea>
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

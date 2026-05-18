<script lang="ts">
  import { renderPreview } from '$lib/ipc/client';
  import type { SavedReply } from '$lib/ipc/bindings';
  import {
    createSavedReply,
    deleteSavedReply,
    importSavedRepliesFromGithub,
    listSavedReplies,
    reorderSavedReplies,
    updateSavedReply
  } from '$lib/ipc/client';

  export let accountId: string | null;

  let loading = false;
  let error = '';
  let replies: SavedReply[] = [];
  let dragReplyId: number | null = null;
  let modalOpen = false;
  let editingReplyId: number | null = null;
  let formName = '';
  let formBody = '';
  let formError = '';
  let formSaving = false;
  let previewHtml = '';
  let previewLoading = false;
  let previewTab: 'write' | 'preview' = 'write';
  let importDisabled = false;
  let importNotice = '';
  let importing = false;

  $: if (accountId) {
    void refreshReplies();
  } else {
    replies = [];
    error = '';
  }

  $: if (previewTab === 'preview' && modalOpen) {
    void refreshPreview();
  }

  async function refreshPreview(): Promise<void> {
    previewLoading = true;
    try {
      const rendered = await renderPreview(formBody, null);
      previewHtml = rendered.html;
    } finally {
      previewLoading = false;
    }
  }

  async function refreshReplies(): Promise<void> {
    if (!accountId) {
      return;
    }
    loading = true;
    error = '';
    try {
      replies = await listSavedReplies(accountId);
    } catch (loadError) {
      error = loadError instanceof Error ? loadError.message : 'Failed to load saved replies.';
    } finally {
      loading = false;
    }
  }

  function openNewModal(): void {
    editingReplyId = null;
    formName = '';
    formBody = '';
    formError = '';
    previewHtml = '';
    previewTab = 'write';
    modalOpen = true;
  }

  function openEditModal(reply: SavedReply): void {
    editingReplyId = reply.id;
    formName = reply.name;
    formBody = reply.body;
    formError = '';
    previewHtml = '';
    previewTab = 'write';
    modalOpen = true;
  }

  async function submitReply(): Promise<void> {
    if (!accountId) {
      return;
    }
    formSaving = true;
    formError = '';
    try {
      if (editingReplyId == null) {
        await createSavedReply(accountId, formName, formBody);
      } else {
        await updateSavedReply(editingReplyId, formName, formBody);
      }
      modalOpen = false;
      await refreshReplies();
    } catch (saveError) {
      formError = saveError instanceof Error ? saveError.message : 'Failed to save saved reply.';
    } finally {
      formSaving = false;
    }
  }

  async function onDelete(reply: SavedReply): Promise<void> {
    await deleteSavedReply(reply.id);
    await refreshReplies();
  }

  function onDragStart(replyId: number): void {
    dragReplyId = replyId;
  }

  async function onDrop(targetId: number): Promise<void> {
    if (!accountId || dragReplyId == null || dragReplyId === targetId) {
      dragReplyId = null;
      return;
    }
    const current = replies.map((reply) => reply.id);
    const fromIndex = current.indexOf(dragReplyId);
    const toIndex = current.indexOf(targetId);
    if (fromIndex < 0 || toIndex < 0) {
      dragReplyId = null;
      return;
    }
    const [moved] = current.splice(fromIndex, 1);
    if (moved == null) {
      dragReplyId = null;
      return;
    }
    current.splice(toIndex, 0, moved);
    dragReplyId = null;
    await reorderSavedReplies(accountId, current);
    await refreshReplies();
  }

  async function onImport(): Promise<void> {
    if (!accountId || importDisabled) {
      return;
    }
    importing = true;
    importNotice = '';
    try {
      await importSavedRepliesFromGithub(accountId);
      await refreshReplies();
    } catch (importError) {
      const message = importError instanceof Error ? importError.message : 'Import failed.';
      importNotice = message;
      if (message.includes('SavedRepliesImportUnavailable')) {
        importDisabled = true;
      }
    } finally {
      importing = false;
    }
  }
</script>

<section class="Box">
  <div class="Box-header d-flex flex-items-center flex-justify-between">
    <h2 class="f4 m-0">Saved replies</h2>
    <div class="d-flex flex-items-center gap-2">
      <button class="btn btn-sm" type="button" on:click={onImport} disabled={importing || importDisabled}>
        {importing ? 'Importing…' : 'Import from GitHub'}
      </button>
      <button class="btn btn-primary btn-sm" type="button" on:click={openNewModal}>
        New saved reply
      </button>
    </div>
  </div>
  <div class="Box-body">
    {#if loading}
      <p class="f6 color-fg-muted m-0">Loading saved replies…</p>
    {:else if error}
      <p class="f6 color-fg-danger m-0">{error}</p>
    {:else if replies.length === 0}
      <p class="f6 color-fg-muted m-0">No saved replies yet for this account.</p>
    {:else}
      <ul class="list-style-none m-0 p-0" data-testid="saved-replies-settings-list">
        {#each replies as reply (reply.id)}
          <li
            class="saved-reply-row border rounded-2 mb-2 p-2"
            on:dragover|preventDefault
            on:drop={() => onDrop(reply.id)}
          >
            <div class="d-flex flex-items-center gap-2">
              <button
                class="btn-link color-fg-muted saved-reply-drag"
                type="button"
                draggable="true"
                on:dragstart={() => onDragStart(reply.id)}
                aria-label="Reorder saved reply"
              >
                ⋮⋮
              </button>
              <div class="flex-auto min-width-0">
                <p class="f6 text-bold mb-1">{reply.name}</p>
                <p class="f6 color-fg-muted mb-0 text-truncate">{reply.body.replace(/\n/g, ' ')}</p>
              </div>
              <button class="btn btn-sm" type="button" on:click={() => openEditModal(reply)}>Edit</button>
              <button class="btn btn-sm btn-danger" type="button" on:click={() => onDelete(reply)}>
                Delete
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
    {#if importNotice}
      <p class="f6 color-fg-attention mt-2 mb-0" data-testid="saved-replies-import-notice">{importNotice}</p>
    {/if}
  </div>
</section>

{#if modalOpen}
  <div class="modal-backdrop" role="presentation" data-testid="saved-replies-modal">
    <div class="modal Box p-3" role="dialog" aria-modal="true" aria-label="Saved reply form">
      <h3 class="f4 mt-0 mb-2">{editingReplyId == null ? 'New saved reply' : 'Edit saved reply'}</h3>
      <label class="d-block mb-2">
        <span class="f6 text-bold">Name</span>
        <input class="form-control mt-1" type="text" bind:value={formName} data-testid="saved-reply-name-input" />
      </label>
      <div class="BtnGroup mb-2">
        <button
          class={`btn btn-sm ${previewTab === 'write' ? 'selected' : ''}`}
          type="button"
          on:click={() => (previewTab = 'write')}
        >
          Write
        </button>
        <button
          class={`btn btn-sm ${previewTab === 'preview' ? 'selected' : ''}`}
          type="button"
          on:click={() => (previewTab = 'preview')}
        >
          Preview
        </button>
      </div>
      {#if previewTab === 'write'}
        <label class="d-block mb-2">
          <span class="f6 text-bold">Body</span>
          <textarea
            class="form-control mt-1"
            rows={6}
            bind:value={formBody}
            data-testid="saved-reply-body-input"
          ></textarea>
        </label>
      {:else if previewLoading}
        <p class="f6 color-fg-muted m-0">Rendering preview…</p>
      {:else}
        <article class="markdown-body border rounded-2 p-2" data-testid="saved-reply-preview">
          {@html previewHtml}
        </article>
      {/if}
      {#if formError}
        <p class="f6 color-fg-danger mt-2 mb-0">{formError}</p>
      {/if}
      <div class="d-flex gap-2 mt-3">
        <button class="btn btn-primary" type="button" on:click={submitReply} disabled={formSaving}>
          {formSaving ? 'Saving…' : 'Save'}
        </button>
        <button class="btn" type="button" on:click={() => (modalOpen = false)}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }

  .modal {
    width: min(640px, calc(100vw - 2rem));
    max-height: 90vh;
    overflow: auto;
  }

  .saved-reply-row {
    background: var(--bgColor-default, #fff);
  }

  .saved-reply-drag {
    cursor: grab;
  }
</style>

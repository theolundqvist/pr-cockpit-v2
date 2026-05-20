<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { listSavedReplies } from '$lib/ipc/client';
  import type { SavedReply } from '$lib/ipc/bindings';
  import { activeAccountIdStore } from '$lib/state/cockpit';
  import { insertIntoFocusedComposer } from './composerFocusRegistry';
  import {
    closeSavedRepliesPalette,
    openSavedRepliesPalette,
    savedRepliesPaletteOpenStore
  } from './palette-state';

  let query = '';
  let loading = false;
  let replies: SavedReply[] = [];
  let error = '';
  let highlightedIndex = 0;
  let status = '';

  $: filteredReplies = replies.filter((reply) => {
    if (!query.trim()) {
      return true;
    }
    const needle = query.trim().toLowerCase();
    return reply.name.toLowerCase().includes(needle) || reply.body.toLowerCase().includes(needle);
  });
  $: if (highlightedIndex >= filteredReplies.length) {
    highlightedIndex = Math.max(filteredReplies.length - 1, 0);
  }

  $: if ($savedRepliesPaletteOpenStore) {
    query = '';
    status = '';
    highlightedIndex = 0;
    void refreshReplies();
  }

  async function refreshReplies(): Promise<void> {
    if (!$activeAccountIdStore) {
      replies = [];
      error = 'Select an account to use saved replies.';
      return;
    }
    loading = true;
    error = '';
    try {
      replies = await listSavedReplies($activeAccountIdStore);
    } catch (loadError) {
      error = loadError instanceof Error ? loadError.message : 'Failed to load saved replies.';
    } finally {
      loading = false;
    }
  }

  function selectReply(reply: SavedReply): void {
    if (!insertIntoFocusedComposer(reply.body)) {
      status = 'Focus a composer before inserting a saved reply.';
      return;
    }
    closeSavedRepliesPalette();
  }

  function onModalKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeSavedRepliesPalette();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      if (filteredReplies.length === 0) {
        return;
      }
      highlightedIndex = (highlightedIndex + 1) % filteredReplies.length;
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      if (filteredReplies.length === 0) {
        return;
      }
      highlightedIndex = (highlightedIndex - 1 + filteredReplies.length) % filteredReplies.length;
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      const selected = filteredReplies[highlightedIndex];
      if (selected) {
        selectReply(selected);
      }
    }
  }

  function onWindowShortcut(event: KeyboardEvent): void {
    if (!event.ctrlKey || !event.shiftKey || event.key !== '.') {
      return;
    }
    event.preventDefault();
    openSavedRepliesPalette();
  }

  onMount(() => {
    window.addEventListener('keydown', onWindowShortcut);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', onWindowShortcut);
  });
</script>

{#if $savedRepliesPaletteOpenStore}
  <div class="conflict-modal-backdrop" role="presentation" data-testid="saved-replies-palette-modal">
    <div
      class="saved-replies-palette Box"
      role="dialog"
      aria-modal="true"
      aria-label="Saved replies palette"
      tabindex="0"
      on:keydown={onModalKeydown}
    >
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Saved replies</h2>
        <button class="btn btn-sm" type="button" on:click={closeSavedRepliesPalette}>Close</button>
      </div>
      <div class="Box-body">
        <input
          class="form-control width-full mb-2"
          type="search"
          bind:value={query}
          placeholder="Search saved replies"
          aria-label="Search saved replies"
          data-testid="saved-replies-palette-search"
        />
        {#if loading}
          <p class="f6 color-fg-muted m-0">Loading saved replies…</p>
        {:else if error}
          <p class="f6 color-fg-danger m-0">{error}</p>
        {:else if filteredReplies.length === 0}
          <p class="f6 color-fg-muted m-0">No saved replies match this search.</p>
        {:else}
          <ul class="list-style-none m-0 p-0">
            {#each filteredReplies as reply, index (reply.id)}
              <li>
                <button
                  class={`saved-replies-palette-item btn-link width-full text-left ${index === highlightedIndex ? 'is-highlighted' : ''}`}
                  type="button"
                  on:mouseenter={() => (highlightedIndex = index)}
                  on:click={() => selectReply(reply)}
                >
                  <span class="text-bold">{reply.name}</span>
                  <span class="f6 color-fg-muted text-truncate">{reply.body.replace(/\n/g, ' ')}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if status}
          <p class="f6 color-fg-attention mt-2 mb-0">{status}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .saved-replies-palette {
    width: min(680px, 96vw);
    max-height: 80vh;
    display: grid;
    grid-template-rows: auto 1fr;
  }

  .saved-replies-palette .Box-body {
    overflow: auto;
  }

  .saved-replies-palette-item {
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 1px solid transparent;
  }

  .saved-replies-palette-item.is-highlighted {
    border-color: var(--borderColor-accent-emphasis, #0969da);
    background: var(--bgColor-accent-muted, #ddf4ff);
  }
</style>

<script lang="ts">
  import { onMount } from 'svelte';

  import {
    refreshWorktreeRoots,
    saveWorktreeRoots,
    triggerWorktreeRediscovery,
    worktreeRootsStore
  } from '$lib/state/cockpit';

  let rootsText = '';
  let saving = false;

  onMount(async () => {
    await refreshWorktreeRoots();
    rootsText = $worktreeRootsStore.join('\n');
  });

  $: if (!saving) {
    rootsText = $worktreeRootsStore.join('\n');
  }

  async function submit(): Promise<void> {
    saving = true;
    const roots = rootsText
      .split('\n')
      .map((value) => value.trim())
      .filter(Boolean);
    await saveWorktreeRoots(roots);
    await triggerWorktreeRediscovery();
    saving = false;
  }
</script>

<div class="p-3 border-top color-border-muted">
  <div class="d-flex flex-items-center flex-justify-between mb-1">
    <h2 class="f6 text-bold m-0">Worktree roots</h2>
    <button class="btn btn-sm" type="button" on:click={submit} disabled={saving}>
      {saving ? 'Saving…' : 'Save'}
    </button>
  </div>
  <p class="f6 color-fg-muted mb-2">Absolute paths only; one root per line.</p>
  <textarea
    class="form-control width-full"
    rows={5}
    bind:value={rootsText}
    aria-label="Worktree roots paths"
  ></textarea>
</div>

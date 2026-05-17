<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import inboxIcon from '@primer/octicons/build/svg/inbox-24.svg?raw';

  import type { WorktreeView } from '$lib/ipc/bindings';
  import InboxRow from '$lib/components/InboxRow.svelte';
  import { prDetailPreload } from '$lib/data/pr-detail';
  import {
    activeAccountIdStore,
    inboxStore,
    refreshAccountData,
    worktreesStore
  } from '$lib/state/cockpit';

  let selectedIndex = 0;

  $: rows = $inboxStore;
  $: worktreesByPr = indexWorktreesByPr($worktreesStore);
  $: if (selectedIndex > Math.max(0, rows.length - 1)) {
    selectedIndex = Math.max(0, rows.length - 1);
  }

  onMount(async () => {
    if ($activeAccountIdStore && rows.length === 0) {
      await refreshAccountData($activeAccountIdStore);
    }
  });

  async function preloadRow(prId: string): Promise<void> {
    if (!$activeAccountIdStore) {
      return;
    }
    await prDetailPreload($activeAccountIdStore, prId);
  }

  async function openSelected(index: number): Promise<void> {
    const row = rows[index];
    if (!row || !$activeAccountIdStore) {
      return;
    }
    await preloadRow(row.pr_id);
    await goto(`/pr/${row.pr_id}`);
  }

  async function onListKeydown(event: KeyboardEvent): Promise<void> {
    if (event.key === 'j') {
      event.preventDefault();
      selectedIndex = Math.min(rows.length - 1, selectedIndex + 1);
      return;
    }
    if (event.key === 'k') {
      event.preventDefault();
      selectedIndex = Math.max(0, selectedIndex - 1);
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      await openSelected(selectedIndex);
    }
  }

  async function onWindowKeydown(event: KeyboardEvent): Promise<void> {
    const target = event.target as HTMLElement | null;
    if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) {
      return;
    }
    await onListKeydown(event);
  }

  function indexWorktreesByPr(worktrees: WorktreeView[]): Map<string, WorktreeView> {
    const byPr = new Map<string, WorktreeView>();
    for (const worktree of worktrees) {
      const prId = worktree.mapped_pr_id;
      if (!prId) {
        continue;
      }
      const existing = byPr.get(prId);
      if (!existing || (worktree.mapping_confidence ?? 0) > (existing.mapping_confidence ?? 0)) {
        byPr.set(prId, worktree);
      }
    }
    return byPr;
  }
</script>

<svelte:window on:keydown={onWindowKeydown} />

<main class="px-3 py-3">
  <section class="Box" aria-label="Pull request inbox">
    <div class="Box-header d-flex flex-items-center flex-justify-between gap-2">
      <div class="d-flex flex-items-center gap-2">
        <span class="color-fg-muted d-flex" aria-hidden="true">
          {@html inboxIcon}
        </span>
        <h1 class="f3 text-normal m-0">Pull Request Inbox</h1>
      </div>
      <span class="Counter">{rows.length}</span>
    </div>
    <div class="Box-body p-0">
      {#if rows.length === 0}
        <div class="p-4 text-center">
          <div class="color-fg-muted mb-2 d-inline-flex" aria-hidden="true">
            {@html inboxIcon}
          </div>
          <p class="color-fg-muted mb-2">No pull requests synced yet.</p>
          <p class="f6 color-fg-subtle mb-3">Connect subscriptions or wait for sync.</p>
          <button class="btn" type="button" on:click={() => $activeAccountIdStore && refreshAccountData($activeAccountIdStore)}>
            Refresh inbox
          </button>
        </div>
      {:else}
        <ul class="list-style-none m-0">
          {#each rows as row, index}
            <InboxRow
              item={row}
              worktree={worktreesByPr.get(row.pr_id) ?? null}
              selected={index === selectedIndex}
              onPreload={() => preloadRow(row.pr_id)}
              onOpen={() => openSelected(index)}
            />
          {/each}
        </ul>
      {/if}
    </div>
  </section>
</main>

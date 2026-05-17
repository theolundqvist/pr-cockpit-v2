<script lang="ts">
  import type { InboxItem, WorktreeView } from '$lib/ipc/bindings';
  import { checkLabel, syntheticLabels } from '$lib/components/inbox-row';
  import PendingAffordance from '$lib/components/PendingAffordance.svelte';
  import WorktreeBadge from '$lib/components/worktree/WorktreeBadge.svelte';
  import { formatRelative } from '$lib/utils/time';

  export let item: InboxItem;
  export let worktree: WorktreeView | null = null;
  export let selected = false;
  export let onOpen: () => void = () => {};
  export let onPreload: () => void = () => {};
</script>

<li class="Box-row p-0">
  <a
    class={`d-block px-3 py-2 text-left color-fg-default no-underline ${selected ? 'color-bg-subtle' : ''}`}
    href={`/pr/${item.pr_id}`}
    on:mouseenter={onPreload}
    on:focus={onPreload}
    on:click|preventDefault={onOpen}
  >
    <div class="d-flex flex-items-start flex-justify-between gap-2">
      <div class="d-flex flex-column flex-auto min-width-0 gap-1">
        <div class="d-flex flex-items-center gap-2">
          {#if item.unread_notification_count > 0}
            <span class="IssueLabel color-bg-accent-emphasis" aria-label="Unread">
              ●
            </span>
          {/if}
          <PendingAffordance overlay={item.pending_overlay} optimism="full" />
          <strong class="text-bold text-truncate">{item.title}</strong>
          <span class="color-fg-muted">#{item.pr_number}</span>
        </div>
        <div class="f6 color-fg-muted d-flex flex-wrap gap-2">
          <span>{item.repo_owner}/{item.repo_name}</span>
          <span>@{item.author_login}</span>
          <span>{formatRelative(item.updated_at)}</span>
        </div>
        <div class="d-flex flex-wrap gap-1">
          {#if item.draft}
            <span class="Label Label--secondary">Draft</span>
          {/if}
          {#if item.unread_notification_count > 0}
            <span class="Label Label--attention">Review requested</span>
          {/if}
          {#each syntheticLabels(item) as label}
            <span class="Label">{label}</span>
          {/each}
          {#if worktree}
            <WorktreeBadge {worktree} />
          {/if}
        </div>
      </div>
      <span class="Label Label--success">{checkLabel(item)}</span>
    </div>
  </a>
</li>

<script lang="ts">
  import type { PendingMutationView } from '$lib/ipc/bindings';

  export let open = false;
  export let failedMutations: PendingMutationView[] = [];
  export let onClose: () => void = () => {};
  export let onRetry: (mutationId: string) => Promise<void> | void = async () => {};
  export let onDiscard: (mutationId: string) => Promise<void> | void = async () => {};

  function groupKey(row: PendingMutationView): string {
    return row.target_id.startsWith('pr_') ? row.target_id : 'misc';
  }

  $: grouped = failedMutations.reduce(
    (acc, row) => {
      const key = groupKey(row);
      const next = acc.get(key) ?? [];
      next.push(row);
      acc.set(key, next);
      return acc;
    },
    new Map<string, PendingMutationView[]>()
  );
</script>

<aside class={`sync-errors-tray ${open ? 'open' : ''}`} aria-hidden={!open}>
  <div class="Box">
    <div class="Box-header d-flex flex-items-center flex-justify-between">
      <h2 class="f5 m-0">Sync errors</h2>
      <button class="btn btn-sm" type="button" on:click={onClose}>Close</button>
    </div>
    <div class="Box-body">
      {#if failedMutations.length === 0}
        <p class="f6 color-fg-muted mb-0">No failed mutations.</p>
      {:else}
        {#each [...grouped.entries()] as [group, rows]}
          <section class="mb-2">
            <h3 class="f6 text-bold mb-1">PR: {group}</h3>
            <ul class="list-style-none m-0">
              {#each rows as row}
                <li class="border rounded-2 p-2 mb-1">
                  <div class="f6 text-bold">{row.kind}</div>
                  <div class="f6 color-fg-muted mb-1">{row.last_error ?? 'unknown error'}</div>
                  <div class="d-flex gap-1">
                    <button class="btn btn-sm" type="button" on:click={() => onRetry(row.id)}>
                      Retry
                    </button>
                    <button class="btn btn-sm btn-danger" type="button" on:click={() => onDiscard(row.id)}>
                      Discard
                    </button>
                  </div>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      {/if}
    </div>
  </div>
</aside>

<script lang="ts">
  import type { PendingMutationView } from '$lib/ipc/bindings';

  export let mutation: PendingMutationView | null = null;
  export let onRetry: (mutationId: string) => Promise<void> | void = async () => {};
  export let onDiscard: (mutationId: string) => Promise<void> | void = async () => {};
</script>

{#if mutation}
  <div class="flash flash-error d-flex flex-items-center flex-justify-between gap-2 mt-2">
    <span class="f6">
      Failed {mutation.kind.replaceAll('_', ' ')} ({mutation.last_error ?? 'unknown error'})
    </span>
    <div class="d-flex gap-1">
      <button class="btn btn-sm" type="button" on:click={() => onRetry(mutation.id)}>Retry</button>
      <button class="btn btn-sm btn-danger" type="button" on:click={() => onDiscard(mutation.id)}>
        Discard
      </button>
    </div>
  </div>
{/if}

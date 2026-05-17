<script lang="ts">
  import type { HardConflictPayload } from '$lib/ipc/bindings';

  export let conflict: HardConflictPayload | null = null;
  export let onRefreshAndRetry: (mutationId: string) => Promise<void> | void = async () => {};
  export let onDiscard: (mutationId: string) => Promise<void> | void = async () => {};
  export let onClose: () => void = () => {};
</script>

{#if conflict}
  <div class="conflict-modal-backdrop" role="presentation">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Hard conflict">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Hard conflict</h2>
        <button class="btn btn-sm" type="button" on:click={onClose}>Close</button>
      </div>
      <div class="Box-body">
        <p class="f6 mb-2">{conflict.diff.summary}</p>
        <div class="d-flex gap-2">
          <div class="flex-auto">
            <h3 class="f6 text-bold mb-1">Predicted</h3>
            <pre class="conflict-diff-block">{conflict.diff.local_body ?? conflict.predicted_snapshot_json}</pre>
          </div>
          <div class="flex-auto">
            <h3 class="f6 text-bold mb-1">Server</h3>
            <pre class="conflict-diff-block">{conflict.diff.server_body ?? conflict.server_snapshot_json}</pre>
          </div>
        </div>
        <div class="mt-2 d-flex gap-1">
          <button class="btn btn-primary" type="button" on:click={() => onRefreshAndRetry(conflict.mutation_id)}>
            Refresh and retry
          </button>
          <button class="btn btn-danger" type="button" on:click={() => onDiscard(conflict.mutation_id)}>
            Discard
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

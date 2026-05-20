<script lang="ts">
  import type { StackEdge, StackWarning } from '$lib/ipc/bindings';

  export let warning: StackWarning | null = null;
  export let edges: StackEdge[] = [];
</script>

{#if warning}
  <div class="flash flash-warn mb-2" data-testid="stack-dag-warning">
    <strong>Stack has a diamond / cycle:</strong>
    {#if warning.diamond_pr_ids.length > 0}
      PRs {warning.diamond_pr_ids.join(', ')}
    {:else}
      ambiguous topology
    {/if}
    <div class="mt-2">
      <details>
        <summary class="f6">Show raw edges</summary>
        <ul class="list-style-none mt-2 mb-0 pl-0">
          {#each edges as edge}
            <li class="text-mono f6">{edge.from_pr_id} → {edge.to_pr_id}</li>
          {/each}
        </ul>
      </details>
    </div>
  </div>
{/if}

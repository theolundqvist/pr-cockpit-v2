<script lang="ts">
  import type { OptimismLevel, PendingOverlay } from '$lib/ipc/bindings';

  export let overlay: PendingOverlay | null = null;
  export let optimism: OptimismLevel | null = null;

  $: visible = Boolean(overlay);
  $: label = optimism === 'cautious' ? 'submitting…' : 'pending';
  $: styleClass = optimism === 'cautious' ? 'Label--attention' : 'Label--secondary';
</script>

{#if visible}
  <span class={`Label {styleClass} pending-affordance`} title={`Mutation ${overlay?.mutation_id}`}>
    {label}
  </span>
{/if}

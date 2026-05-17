<script lang="ts">
  export let visible = false;
</script>

{#if visible}
  <span
    class="Label Label--secondary mt-1"
    title="GitHub re-rendered your markdown"
    aria-label="Server adjusted markdown"
  >
    server adjusted
  </span>
{/if}

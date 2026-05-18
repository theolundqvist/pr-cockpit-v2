<script lang="ts">
  export let login: string;
  export let host: string;
  export let interactive = false;
  export let onSelect: (() => void) | null = null;

  $: toneClass = host === 'github.com' ? 'account-badge--github' : 'account-badge--ghe';

  function handleClick(event: MouseEvent): void {
    if (!interactive || !onSelect) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    onSelect();
  }
</script>

{#if interactive}
  <button
    class={`Label account-badge ${toneClass}`}
    type="button"
    on:click={handleClick}
    data-testid="inbox-account-badge"
  >
    @{login} · {host}
  </button>
{:else}
  <span class={`Label account-badge ${toneClass}`} data-testid="inbox-account-badge">
    @{login} · {host}
  </span>
{/if}

<style>
  .account-badge {
    border: 1px solid transparent;
  }

  .account-badge--github {
    background: #ddf4ff;
    color: #0969da;
    border-color: #54aeff66;
  }

  .account-badge--ghe {
    background: #fbefff;
    color: #8250df;
    border-color: #bf8cff66;
  }

  button.account-badge {
    cursor: pointer;
  }
</style>

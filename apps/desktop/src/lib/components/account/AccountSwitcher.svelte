<script lang="ts">
  import { toAccountId } from '$lib/ipc/client';
  import type { AuthAccount } from '$lib/ipc/bindings';

  export let accounts: AuthAccount[] = [];
  export let activeAccountId: string | null = null;
  export let selectedAccountFilter: string | null = null;
  export let onSelect: (accountId: string | null) => Promise<void> | void = () => {};

  let open = false;

  $: selectedAccount =
    selectedAccountFilter === null
      ? null
      : accounts.find((account) => toAccountId(account) === selectedAccountFilter) ?? null;
  $: label = selectedAccount
    ? `${selectedAccount.login} · ${selectedAccount.host}`
    : 'All accounts';
  $: isSelectedGhe = selectedAccount ? selectedAccount.host !== 'github.com' : false;

  async function choose(accountId: string | null): Promise<void> {
    open = false;
    await onSelect(accountId);
  }
</script>

<details class="details-reset details-overlay position-relative width-full" bind:open>
  <summary class="btn width-full d-flex flex-items-center flex-justify-between" data-testid="account-switcher-trigger">
    <span class="d-flex flex-items-center gap-2">
      <span class="text-truncate">{label}</span>
      {#if isSelectedGhe}
        <span class="Label Label--accent-emphasis">GHE</span>
      {/if}
    </span>
    <span class="color-fg-muted" aria-hidden="true">▾</span>
  </summary>
  <div class="SelectMenu right-0 mt-1 width-full" data-testid="account-switcher-menu">
    <div class="SelectMenu-modal">
      <header class="SelectMenu-header">
        <h3 class="SelectMenu-title">Switch inbox account</h3>
      </header>
      <div class="SelectMenu-list">
        <button
          class={`SelectMenu-item ${selectedAccountFilter === null ? 'selected' : ''}`}
          type="button"
          on:click={() => choose(null)}
          data-testid="account-option-all"
        >
          <span class="SelectMenu-icon" aria-hidden="true">{selectedAccountFilter === null ? '✓' : ''}</span>
          <span class="SelectMenu-item-text text-bold">All accounts</span>
        </button>
        {#each accounts as account}
          <button
            class={`SelectMenu-item ${toAccountId(account) === selectedAccountFilter ? 'selected' : ''}`}
            type="button"
            on:click={() => choose(toAccountId(account))}
            data-testid={`account-option-${toAccountId(account)}`}
          >
            <span class="SelectMenu-icon" aria-hidden="true">
              {toAccountId(account) === selectedAccountFilter ? '✓' : ''}
            </span>
            <span class="SelectMenu-item-text d-flex flex-items-center gap-2">
              <img
                src={`https://github.com/${account.login}.png`}
                alt=""
                width="16"
                height="16"
                class="avatar avatar-small"
              />
              <span class="text-bold">@{account.login}</span>
              <span class="color-fg-muted">{account.host}</span>
              {#if account.host !== 'github.com'}
                <span class="Label Label--accent-emphasis">GHE</span>
              {/if}
              {#if toAccountId(account) === activeAccountId}
                <span class="Label Label--secondary">active</span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
    </div>
  </div>
</details>

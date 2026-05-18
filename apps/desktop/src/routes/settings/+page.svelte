<script lang="ts">
  import { get } from 'svelte/store';

  import { savePatToken, testEndpoints, toAccountId } from '$lib/ipc/client';
  import type { EndpointTestResult } from '$lib/ipc/bindings';
  import NotificationsSettings from '$lib/components/settings/Notifications.svelte';
  import SavedRepliesSettings from '$lib/components/settings/SavedRepliesSettings.svelte';
  import {
    accountsStore,
    activeAccountIdStore,
    refreshAccountData,
    refreshAccounts,
    selectAccountById
  } from '$lib/state/cockpit';

  let addModalOpen = false;
  let showHostField = false;
  let host = 'github.com';
  let token = '';
  let addError = '';
  let addPending = false;
  let endpointResult: EndpointTestResult | null = null;
  let activeTab: 'accounts' | 'notifications' | 'saved-replies' = 'notifications';

  function openAddModal(): void {
    addModalOpen = true;
    showHostField = false;
    host = 'github.com';
    token = '';
    addError = '';
    endpointResult = null;
  }

  function closeAddModal(): void {
    addModalOpen = false;
  }

  async function submitAddAccount(): Promise<void> {
    addPending = true;
    addError = '';
    endpointResult = null;
    try {
      const normalizedHost = (host || 'github.com').trim().toLowerCase();
      const savedAccount = await savePatToken(normalizedHost, token);
      const validation = await testEndpoints(normalizedHost);
      endpointResult = validation;
      if (!validation.api_ok || !validation.graphql_ok) {
        addError = `Endpoint check failed for ${normalizedHost}. REST=${validation.api_ok ? 'ok' : 'failed'}, GraphQL=${validation.graphql_ok ? 'ok' : 'failed'}.`;
        return;
      }
      await refreshAccounts();
      const savedAccountId = toAccountId(savedAccount);
      await selectAccountById(savedAccountId);
      const selected = get(activeAccountIdStore);
      await refreshAccountData(selected);
      closeAddModal();
    } catch (error) {
      addError = error instanceof Error ? error.message : 'Failed to save account.';
    } finally {
      addPending = false;
    }
  }
</script>

<main class="px-3 py-3">
  <nav class="UnderlineNav mb-3" aria-label="Settings sections">
    <div class="UnderlineNav-body">
      <button
        class={`UnderlineNav-item btn-link ${activeTab === 'accounts' ? 'selected' : ''}`}
        type="button"
        on:click={() => (activeTab = 'accounts')}
      >
        Accounts
      </button>
      <button
        class={`UnderlineNav-item btn-link ${activeTab === 'notifications' ? 'selected' : ''}`}
        type="button"
        on:click={() => (activeTab = 'notifications')}
      >
        Notifications
      </button>
      <button
        class={`UnderlineNav-item btn-link ${activeTab === 'saved-replies' ? 'selected' : ''}`}
        type="button"
        data-testid="settings-tab-saved-replies"
        on:click={() => (activeTab = 'saved-replies')}
      >
        Saved replies
      </button>
    </div>
  </nav>

  {#if activeTab === 'accounts'}
    <section class="Box mb-3">
      <div class="Box-header d-flex flex-justify-between flex-items-center">
        <h1 class="f3 m-0">Accounts</h1>
        <button class="btn btn-sm" type="button" on:click={openAddModal} data-testid="add-account-button">
          Add account
        </button>
      </div>
      <div class="Box-body">
        <p class="f6 color-fg-muted mt-0">
          Device flow + gh import are github.com only; use a PAT for GHE.
        </p>
        <ul class="list-style-none m-0 p-0">
          {#each $accountsStore as account}
            <li class="mb-2 d-flex flex-items-center gap-2">
              <span class="text-bold">@{account.login}</span>
              <span class="color-fg-muted">{account.host}</span>
              {#if account.host !== 'github.com'}
                <span class="Label Label--accent-emphasis">GHE</span>
              {/if}
              {#if toAccountId(account) === $activeAccountIdStore}
                <span class="Label Label--secondary">active</span>
              {/if}
            </li>
          {/each}
        </ul>
      </div>
    </section>
  {:else if activeTab === 'notifications'}
    <section class="Box">
      <div class="Box-header">
        <h1 class="f3 m-0">Notification settings</h1>
      </div>
      <div class="Box-body">
        <NotificationsSettings accountId={$activeAccountIdStore} />
      </div>
    </section>
  {:else}
    <SavedRepliesSettings accountId={$activeAccountIdStore} />
  {/if}
</main>

{#if addModalOpen}
  <div class="modal-backdrop" data-testid="add-account-modal">
    <div class="modal Box p-3">
      <h2 class="f4 mt-0 mb-2">Add account</h2>
      <p class="f6 color-fg-muted mt-0 mb-2">
        Save a PAT and validate REST/GraphQL endpoints for this host.
      </p>
      {#if !showHostField}
        <p class="f6 mb-2">
          Host: <code>github.com</code>
          <button
            class="btn-link ml-1"
            type="button"
            on:click={() => (showHostField = true)}
            data-testid="ghe-host-link"
          >
            GitHub Enterprise host?
          </button>
        </p>
      {/if}
      {#if showHostField}
        <label class="d-block mb-2">
          <span class="f6 text-bold">Host</span>
          <input
            class="form-control mt-1"
            type="text"
            bind:value={host}
            placeholder="github.com"
            data-testid="add-account-host-input"
          />
        </label>
      {/if}
      <label class="d-block mb-2">
        <span class="f6 text-bold">Token</span>
        <input
          class="form-control mt-1"
          type="password"
          bind:value={token}
          autocomplete="off"
          data-testid="add-account-token-input"
        />
      </label>
      {#if addError}
        <p class="color-fg-danger f6 mt-2 mb-2" data-testid="add-account-error">{addError}</p>
      {/if}
      {#if endpointResult}
        <p class="color-fg-muted f6 mt-2 mb-2" data-testid="endpoint-test-result">
          REST {endpointResult.api_ok ? 'ok' : 'failed'} ({endpointResult.api_latency_ms}ms) · GraphQL
          {endpointResult.graphql_ok ? 'ok' : 'failed'} ({endpointResult.graphql_latency_ms}ms)
        </p>
      {/if}
      <div class="d-flex gap-2 mt-3">
        <button
          class="btn btn-primary"
          type="button"
          on:click={submitAddAccount}
          disabled={addPending || token.trim().length === 0}
          data-testid="validate-save-account-button"
        >
          {addPending ? 'Validating…' : 'Validate + save'}
        </button>
        <button class="btn" type="button" on:click={closeAddModal}>
          Cancel
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .modal {
    width: min(560px, calc(100vw - 2rem));
  }
</style>

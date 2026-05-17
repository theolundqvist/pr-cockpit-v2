<script lang="ts">
  import '../app.css';
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/stores';
  import { get } from 'svelte/store';

  import { listenEvent, listenEventPayload, listNotificationEvents, toAccountId } from '$lib/ipc/client';
  import type { NotificationEventPayload } from '$lib/ipc/bindings';
  import {
    activeAccountIdStore,
    accountsStore,
    focusModeStore,
    initializeCockpit,
    refreshAccountData,
    repoSubscriptionsStore,
    selectAccountById,
    shellBootedStore,
    statusStore
  } from '$lib/state/cockpit';
  import type { LayoutData } from './$types';

  export let data: LayoutData;

  let unlistenRateLimit: (() => void) | null = null;
  let unlistenNotification: (() => void) | null = null;
  let notificationIndicatorCount = 0;

  $: activeRateLimit = $statusStore?.rate_limits[0] ?? null;
  $: selectedAccount = $accountsStore.find((account) => toAccountId(account) === $activeAccountIdStore) ?? null;

  onMount(async () => {
    await initializeCockpit(data.boot);
    await subscribeRateLimit();
    await refreshNotificationIndicator();
    await subscribeNotificationEvents();
  });

  onDestroy(() => {
    unlistenRateLimit?.();
    unlistenRateLimit = null;
    unlistenNotification?.();
    unlistenNotification = null;
  });

  async function subscribeRateLimit(): Promise<void> {
    unlistenRateLimit?.();
    const activeId = get(activeAccountIdStore);
    if (!activeId) {
      return;
    }
    unlistenRateLimit = await listenEvent(`rate_limit:account:${activeId} changed`, async () => {
      await refreshAccountData(activeId);
    });
  }

  async function refreshNotificationIndicator(): Promise<void> {
    const activeId = get(activeAccountIdStore);
    if (!activeId) {
      notificationIndicatorCount = 0;
      return;
    }
    const events = await listNotificationEvents(activeId, 100, 0, null);
    notificationIndicatorCount = events.filter((event) => !event.seen).length;
  }

  async function subscribeNotificationEvents(): Promise<void> {
    unlistenNotification?.();
    unlistenNotification = await listenEventPayload<NotificationEventPayload>(
      'notification:event',
      async (payload) => {
        const activeId = get(activeAccountIdStore);
        if (!activeId || payload.account_id !== activeId) {
          return;
        }
        notificationIndicatorCount += 1;
      }
    );
  }

  async function onAccountChange(event: Event): Promise<void> {
    const nextAccountId = (event.currentTarget as HTMLSelectElement).value;
    await selectAccountById(nextAccountId);
    await subscribeRateLimit();
    await refreshNotificationIndicator();
  }

  function toggleFocusMode(): void {
    focusModeStore.update((mode) => (mode === 'focused' ? 'background' : 'focused'));
  }
</script>

<div class="cockpit-app">
  <aside class="cockpit-sidebar border-right color-border-muted">
    <div class="p-3 border-bottom color-border-muted">
      <h1 class="f4 text-bold m-0">PR Cockpit</h1>
      <p class="f6 color-fg-muted mt-1 mb-0">Read-only offline workspace</p>
    </div>

    <div class="p-3 border-bottom color-border-muted">
      <label class="f6 text-bold mb-1 d-block" for="account-switcher">Account</label>
      <select
        id="account-switcher"
        class="form-select width-full"
        value={$activeAccountIdStore ?? ''}
        on:change={onAccountChange}
      >
        {#each $accountsStore as account}
          <option value={toAccountId(account)}>
            {account.login} · {account.host}
          </option>
        {/each}
      </select>
    </div>

    <div class="p-3 flex-auto overflow-auto">
      <nav class="mb-3">
        <a
          class={`btn btn-sm width-full mb-1 ${$page.url.pathname === '/' ? 'btn-primary' : ''}`}
          href="/"
        >
          Inbox
        </a>
        <a
          class={`btn btn-sm width-full ${$page.url.pathname.startsWith('/settings') ? 'btn-primary' : ''}`}
          href="/settings"
        >
          Settings
        </a>
      </nav>

      <h2 class="f6 text-bold mb-2">Repo subscriptions</h2>
      {#if $repoSubscriptionsStore.length === 0}
        <p class="f6 color-fg-muted">No subscriptions yet.</p>
      {:else}
        <ul class="list-style-none m-0">
          {#each $repoSubscriptionsStore as subscription}
            <li class="mb-2 d-flex flex-items-center flex-justify-between gap-2">
              <span class="f6 text-truncate">{subscription.repo_owner}/{subscription.repo_name}</span>
              <span class="Label">{subscription.watch_tier}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="p-3 border-top color-border-muted">
      <div class="f6 text-bold mb-1">Rate limit</div>
      {#if activeRateLimit}
        <div class="Progress mb-1" aria-label="Rate limit budget">
          <span
            class="Progress-item color-bg-success-emphasis"
            style={`width: ${(activeRateLimit.remaining / Math.max(1, activeRateLimit.limit_total)) * 100}%`}
          ></span>
        </div>
        <div class="f6 color-fg-muted">
          {activeRateLimit.remaining}/{activeRateLimit.limit_total} {activeRateLimit.resource}
        </div>
      {:else}
        <div class="f6 color-fg-muted">No budget data</div>
      {/if}
    </div>
  </aside>

  <section class="cockpit-main">
    <header class="cockpit-topbar border-bottom color-border-muted px-3 py-2 d-flex flex-items-center gap-2">
      <input
        class="form-control flex-auto"
        type="search"
        placeholder="Search PRs (coming soon)"
        aria-label="Search pull requests"
      />
      <button class="btn" type="button" on:click={toggleFocusMode}>
        Focus: {$focusModeStore === 'focused' ? 'On' : 'Off'}
      </button>
      <span class="Label" aria-label="Notification inbox indicator">
        Notifications {notificationIndicatorCount}
      </span>
      {#if selectedAccount}
        <span class="f6 color-fg-muted">@{selectedAccount.login}</span>
      {/if}
    </header>

    <main class="cockpit-pane">
      {#if $shellBootedStore}
        <slot />
      {:else}
        <div class="p-4">
          <div class="skeleton-row mb-2"></div>
          <div class="skeleton-row mb-2"></div>
          <div class="skeleton-row"></div>
        </div>
      {/if}
    </main>
  </section>
</div>

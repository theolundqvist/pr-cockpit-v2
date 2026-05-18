<script lang="ts">
  import '../app.css';
  import { onDestroy, onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { get } from 'svelte/store';

  import {
    listenEvent,
    listenEventPayload,
    listNotificationEvents,
    listSavedReplies,
    openExternalUrl,
    submitMutation,
    toAccountId
  } from '$lib/ipc/client';
  import type { MutationKind, NotificationEventPayload, RateLimitChangedEventPayload } from '$lib/ipc/bindings';
  import AccountSwitcher from '$lib/components/account/AccountSwitcher.svelte';
  import CommandPalette from '$lib/components/palette/CommandPalette.svelte';
  import {
    commandPaletteState,
    openAccountSwitchPalette,
    openCommandPalette,
    openPrNumberPalette
  } from '$lib/components/palette/state';
  import SavedRepliesPalette from '$lib/components/saved-replies/SavedRepliesPalette.svelte';
  import { insertIntoFocusedComposer } from '$lib/components/saved-replies/composerFocusRegistry';
  import { STATIC_COMMANDS, buildSavedReplyCommands } from '$lib/commands/commands';
  import { createKeymapResolver } from '$lib/commands/keymap';
  import {
    executeCommand,
    registerStaticCommands,
    setDynamicCommands,
    type CommandContext
  } from '$lib/commands/registry';
  import RateLimitMeter from '$lib/components/status/RateLimitMeter.svelte';
  import WorktreeRoots from '$lib/components/worktree/WorktreeRoots.svelte';
  import {
    activeAccountIdStore,
    accountsStore,
    focusModeStore,
    inboxStore,
    inboxAccountFilterStore,
    initializeCockpit,
    refreshAccountData,
    repoSubscriptionsStore,
    selectAccountById,
    shellBootedStore,
    statusStore
  } from '$lib/state/cockpit';
  import type { LayoutData } from './$types';

  export let data: LayoutData;

  const keymapResolver = createKeymapResolver();
  registerStaticCommands(STATIC_COMMANDS);

  let unlistenRateLimit: (() => void) | null = null;
  let unlistenWorktreeDiscovery: (() => void) | null = null;
  let unlistenNotification: (() => void) | null = null;
  let notificationIndicatorCount = 0;
  let savedRepliesAccountLoaded: string | null = null;

  $: selectedAccount = $accountsStore.find((account) => toAccountId(account) === $activeAccountIdStore) ?? null;
  $: if ($activeAccountIdStore && savedRepliesAccountLoaded !== $activeAccountIdStore) {
    savedRepliesAccountLoaded = $activeAccountIdStore;
    void refreshSavedReplyCommands($activeAccountIdStore);
  }

  onMount(async () => {
    try {
      const storedTheme = window.localStorage.getItem('cockpit.color-mode');
      if (storedTheme === 'dark' || storedTheme === 'light') {
        document.documentElement.dataset.colorMode = storedTheme;
      }
    } catch {
      // Ignore theme hydration issues.
    }
    await initializeCockpit(data.boot);
    await subscribeRateLimit();
    await refreshNotificationIndicator();
    await subscribeNotificationEvents();
  });

  onDestroy(() => {
    unlistenRateLimit?.();
    unlistenRateLimit = null;
    unlistenWorktreeDiscovery?.();
    unlistenWorktreeDiscovery = null;
    unlistenNotification?.();
    unlistenNotification = null;
    keymapResolver.resetSequence();
  });

  async function subscribeRateLimit(): Promise<void> {
    unlistenRateLimit?.();
    unlistenWorktreeDiscovery?.();
    unlistenRateLimit = await listenEventPayload<RateLimitChangedEventPayload>(
      'rate_limit:account:<id> changed',
      async () => {
        await refreshAccountData();
      }
    );
    unlistenWorktreeDiscovery = await listenEvent('worktree:discovery completed', async () => {
      await refreshAccountData();
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

  async function onAccountSelect(nextAccountId: string | null): Promise<void> {
    await selectAccountById(nextAccountId);
    await subscribeRateLimit();
    await refreshNotificationIndicator();
  }

  async function refreshSavedReplyCommands(accountId: string): Promise<void> {
    try {
      const replies = await listSavedReplies(accountId);
      setDynamicCommands('saved-replies', buildSavedReplyCommands(replies));
    } catch {
      setDynamicCommands('saved-replies', []);
    }
  }

  function openModal(id: string, _props?: unknown): void {
    if (id === 'palette.commands') {
      openCommandPalette();
      return;
    }
    if (id === 'account.switch') {
      openAccountSwitchPalette();
      return;
    }
    if (id === 'pr.openByNumber') {
      openPrNumberPalette();
    }
  }

  function getFocusedContext(): {
    activeFilePath?: string | undefined;
    activeSuggestionId?: string | undefined;
    activeThreadId?: string | undefined;
    activeCheckRunId?: string | undefined;
    activeCheckSuiteId?: string | undefined;
  } {
    if (typeof document === 'undefined') {
      return {};
    }
    const active = (document.activeElement as HTMLElement | null) ?? null;
    if (!active) {
      return {};
    }
    const threadNode = active.closest<HTMLElement>('[data-command-thread-id]');
    const fileNode = active.closest<HTMLElement>('[data-command-file-path]');
    const suggestionNode = active.closest<HTMLElement>('[data-command-suggestion-id]');
    const checkRunNode = active.closest<HTMLElement>('[data-command-check-run-id]');
    const checkSuiteNode = active.closest<HTMLElement>('[data-command-check-suite-id]');
    return {
      activeFilePath: fileNode?.dataset.commandFilePath,
      activeSuggestionId: suggestionNode?.dataset.commandSuggestionId,
      activeThreadId: threadNode?.dataset.commandThreadId,
      activeCheckRunId: checkRunNode?.dataset.commandCheckRunId,
      activeCheckSuiteId: checkSuiteNode?.dataset.commandCheckSuiteId
    };
  }

  function activePrIdFromRoute(pathname: string): string | undefined {
    const match = pathname.match(/^\/pr\/([^/]+)/);
    return match?.[1];
  }

  function activePrUrl(activePrId: string | undefined): string | null {
    if (!activePrId) {
      return null;
    }
    const rows = get(inboxStore);
    const activeAccountId = get(activeAccountIdStore);
    const byId =
      rows.find(
        (row) => row.pr_id === activePrId && (!activeAccountId || row.account_id === activeAccountId)
      ) ?? rows.find((row) => row.pr_id === activePrId);
    if (!byId) {
      return null;
    }
    return `https://${byId.account_host}/${byId.repo_owner}/${byId.repo_name}/pull/${byId.pr_number}`;
  }

  function inboxUrl(): string {
    const account = $accountsStore.find((entry) => toAccountId(entry) === $activeAccountIdStore);
    const host = account?.host ?? 'github.com';
    return `https://${host}/issues?q=author%3A%40me`;
  }

  function buildCommandContext(): CommandContext {
    const activeRoute = $page.url.pathname;
    const activePrId = activePrIdFromRoute(activeRoute);
    const focused = getFocusedContext();
    const hasSuggestions =
      typeof document !== 'undefined' && document.querySelector('[data-command-suggestion-id]') !== null;
    const hasBatchSuggestions =
      typeof document !== 'undefined' && document.querySelector('[data-command-open-batch]') !== null;
    return {
      activeAccountId: $activeAccountIdStore ?? '',
      activeRoute,
      activePrId,
      activeFilePath: focused.activeFilePath,
      activeSuggestionId: focused.activeSuggestionId,
      openModal,
      navigate: (path: string) => {
        void goto(path);
      },
      submitMutation: async (kind: string, payload: unknown) => {
        if (!$activeAccountIdStore) {
          return;
        }
        await submitMutation(
          $activeAccountIdStore,
          kind as MutationKind,
          JSON.stringify((payload ?? {}) as Record<string, unknown>)
        );
      },
      invokeAction: (action: string) => invokeUiAction(action),
      openExternalUrl,
      insertIntoComposer: (text: string) => insertIntoFocusedComposer(text),
      activeThreadId: focused.activeThreadId,
      activeCheckRunId: focused.activeCheckRunId,
      activeCheckSuiteId: focused.activeCheckSuiteId,
      activePrUrl: activePrUrl(activePrId),
      inboxUrl: inboxUrl(),
      hasSuggestions,
      hasBatchSuggestions,
      paletteOpen: $commandPaletteState.open
    };
  }

  function clickNode(node: Element | null): void {
    if (!node || !(node instanceof HTMLElement)) {
      return;
    }
    node.click();
  }

  async function invokeUiAction(action: string): Promise<void> {
    const focused = document.activeElement as HTMLElement | null;
    if (action === 'composer.open' || action === 'composer.focus') {
      window.dispatchEvent(
        new CustomEvent('command:composer-focus', {
          detail: { ensureConversationTab: action === 'composer.open' }
        })
      );
      return;
    }
    if (action === 'composer.identity.switch') {
      window.dispatchEvent(new CustomEvent('command:composer-identity-switch'));
      return;
    }
    if (action === 'suggestion.apply') {
      const focusedSuggestion = focused?.closest<HTMLElement>('[data-command-suggestion-id]');
      const suggestionButton =
        focusedSuggestion?.querySelector('button') ??
        document.querySelector('[data-command-open-batch]') ??
        document.querySelector('[data-command-suggestion-id] button');
      if (suggestionButton) {
        clickNode(suggestionButton);
        return;
      }
      window.dispatchEvent(new CustomEvent('command:suggestion-batch-open'));
      return;
    }
    if (action === 'suggestion.applyBatch') {
      const trigger = document.querySelector('[data-command-open-batch]');
      if (trigger) {
        clickNode(trigger);
        return;
      }
      window.dispatchEvent(new CustomEvent('command:suggestion-batch-open'));
      return;
    }
    if (action === 'thread.resolve' || action === 'thread.unresolve') {
      const threadNode = focused?.closest<HTMLElement>('[data-command-thread-id]');
      const selector =
        action === 'thread.resolve'
          ? '[data-command-action="resolve-thread"]'
          : '[data-command-action="unresolve-thread"]';
      clickNode(threadNode?.querySelector(selector) ?? document.querySelector(selector));
      return;
    }
    if (action === 'thread.focusNext' || action === 'thread.focusPrevious') {
      const threadNodes = [...document.querySelectorAll<HTMLElement>('[data-command-thread-focus]')];
      if (threadNodes.length === 0) {
        return;
      }
      const activeThread = focused?.closest<HTMLElement>('[data-command-thread-focus]');
      const currentIndex = activeThread
        ? threadNodes.findIndex((node) => node === activeThread)
        : -1;
      const delta = action === 'thread.focusNext' ? 1 : -1;
      const nextIndex = (currentIndex + delta + threadNodes.length) % threadNodes.length;
      threadNodes[nextIndex]?.focus();
      return;
    }
    if (action === 'file.markViewed' || action === 'file.unmarkViewed') {
      const filesTabButton = document.querySelector<HTMLButtonElement>('[data-command-tab="files"]');
      if (filesTabButton && !filesTabButton.classList.contains('selected')) {
        filesTabButton.click();
        await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
      }
      const fileNode =
        focused?.closest<HTMLElement>('[data-command-file-path]') ??
        document.querySelector<HTMLElement>('[data-command-file-path]');
      const checkbox =
        fileNode instanceof HTMLInputElement
          ? fileNode
          : fileNode?.querySelector<HTMLInputElement>('input[type="checkbox"]');
      const allFileToggles = [...document.querySelectorAll<HTMLInputElement>('[data-command-file-path]')];
      const shouldCheck = action === 'file.markViewed';
      const targetCheckbox =
        checkbox && checkbox.checked !== shouldCheck
          ? checkbox
          : shouldCheck
            ? allFileToggles.find((entry) => !entry.checked) ?? checkbox
            : allFileToggles.find((entry) => entry.checked) ?? checkbox;
      if (!targetCheckbox) {
        return;
      }
      if (targetCheckbox.checked !== shouldCheck) {
        targetCheckbox.click();
      }
      return;
    }
    if (action === 'checks.rerunRun') {
      const runNode = focused?.closest<HTMLElement>('[data-command-check-run-id]');
      const target =
        runNode?.matches('[data-command-action="rerun-check-run"]')
          ? runNode
          : runNode?.querySelector('[data-command-action="rerun-check-run"]');
      clickNode(target ?? null);
      return;
    }
    if (action === 'checks.rerunSuite') {
      const suiteNode = focused?.closest<HTMLElement>('[data-command-check-suite-id]');
      const target =
        suiteNode?.matches('[data-command-action="rerun-check-suite"]')
          ? suiteNode
          : suiteNode?.querySelector('[data-command-action="rerun-check-suite"]');
      clickNode(target ?? null);
      return;
    }
    if (action === 'pr.refresh') {
      window.dispatchEvent(new CustomEvent('command:pr-refresh'));
      return;
    }
    if (action === 'theme.toggle') {
      const current = document.documentElement.dataset.colorMode ?? 'light';
      const next = current === 'dark' ? 'light' : 'dark';
      document.documentElement.dataset.colorMode = next;
      try {
        window.localStorage.setItem('cockpit.color-mode', next);
      } catch {
        // Ignore localStorage write failures.
      }
      return;
    }
  }

  async function onGlobalKeydown(event: KeyboardEvent): Promise<void> {
    if (event.defaultPrevented) {
      return;
    }
    const resolution = keymapResolver.resolve(event, $commandPaletteState.open);
    if (resolution.consume) {
      event.preventDefault();
    }
    if (!resolution.commandId) {
      return;
    }
    await executeCommand(resolution.commandId, buildCommandContext());
  }

  function toggleFocusMode(): void {
    focusModeStore.update((mode) => (mode === 'focused' ? 'background' : 'focused'));
  }
</script>

<svelte:window on:keydown={onGlobalKeydown} />

<div class="cockpit-app">
  <aside class="cockpit-sidebar border-right color-border-muted">
    <div class="p-3 border-bottom color-border-muted">
      <h1 class="f4 text-bold m-0">PR Cockpit</h1>
      <p class="f6 color-fg-muted mt-1 mb-0">Read-only offline workspace</p>
    </div>

    <div class="p-3 border-bottom color-border-muted">
      <div class="f6 text-bold mb-1 d-block">Account</div>
      <AccountSwitcher
        accounts={$accountsStore}
        activeAccountId={$activeAccountIdStore}
        selectedAccountFilter={$inboxAccountFilterStore}
        onSelect={onAccountSelect}
      />
      {#if selectedAccount}
        <p class="f6 color-fg-muted mt-2 mb-0" data-testid="posting-identity-indicator">
          Posting as @{selectedAccount.login}
        </p>
      {/if}
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

    <WorktreeRoots />

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
    <footer class="cockpit-statusbar border-top color-border-muted px-3 py-2">
      <RateLimitMeter
        rateLimits={$statusStore?.rate_limits ?? []}
        accounts={$accountsStore}
        activeAccountId={$activeAccountIdStore}
        selectedAccountFilter={$inboxAccountFilterStore}
      />
    </footer>
  </section>
</div>
<CommandPalette contextFactory={buildCommandContext} />
<SavedRepliesPalette />

<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { get } from 'svelte/store';
  import { onDestroy, onMount } from 'svelte';

  import type { AuthAccount, InboxItem } from '$lib/ipc/bindings';
  import { activeAccountIdStore, accountsStore, inboxStore, selectAccountById } from '$lib/state/cockpit';
  import type { CommandContext, Command } from '$lib/commands/registry';
  import {
    commandPaletteState,
    closeCommandPalette,
    setPaletteController,
    type PaletteMode
  } from './state';
  import {
    executeCommand,
    getRecentCommandIds,
    listCommands
  } from '$lib/commands/registry';

  export let contextFactory: () => CommandContext;

  type RowModel = { kind: 'section'; id: string; title: string } | { kind: 'command'; command: Command };
  type ScoredCommand = { command: Command; score: number };
  type AccountPaletteItem = { kind: 'account'; id: string; label: string };
  type PrPaletteItem = { kind: 'pr'; id: string; label: string; row: InboxItem };
  type PaletteSelectableItem = Command | AccountPaletteItem | PrPaletteItem;

  let query = '';
  let filteredCommands: Command[] = [];
  let selectedIndex = 0;
  let resultsScrollElement: HTMLDivElement | null = null;
  let searchInput: HTMLInputElement | null = null;
  let debounceHandle: ReturnType<typeof setTimeout> | null = null;
  let idleHandle: number | null = null;
  let mountedOpen = false;
  let selectableItems: PaletteSelectableItem[] = [];
  let palette = get(commandPaletteState);
  let mode: PaletteMode = 'commands';
  let allCommands: Command[] = [];
  let recentIds: string[] = [];
  let recentCommands: Command[] = [];
  let otherCommands: Command[] = [];
  let otherRows: RowModel[] = [];
  let usesVirtualizedList = false;
  let modeSpecificItems: Array<AccountPaletteItem | PrPaletteItem> = [];

  const virtualizationThreshold = 50;
  const virtualizer = createVirtualizer({
    count: 0,
    getScrollElement: () => resultsScrollElement,
    estimateSize: (index) => (otherRows[index]?.kind === 'section' ? 32 : 44),
    overscan: 8
  });

  let virtualItems: Array<{ index: number; start: number; size: number; key: number }> = [];

  $: palette = $commandPaletteState;
  $: mode = palette.mode;
  $: if (palette.open) {
    recentIds = getRecentCommandIds();
  }
  $: if (palette.open && mode === 'commands' && allCommands) {
    scheduleFilter();
  }
  $: recentCommands = filterRecentCommands(filteredCommands, recentIds);
  $: otherCommands = filteredCommands.filter(
    (command) => !recentCommands.some((recent) => recent.id === command.id)
  );
  $: otherRows = flattenRows(groupBySection(otherCommands));
  $: usesVirtualizedList = otherRows.length > virtualizationThreshold;
  $: {
    $virtualizer.setOptions({
      count: usesVirtualizedList ? otherRows.length : 0,
      getScrollElement: () => resultsScrollElement
    });
    virtualItems = usesVirtualizedList
      ? ($virtualizer.getVirtualItems() as Array<{ index: number; start: number; size: number; key: number }>)
      : [];
  }
  $: selectableItems =
    mode === 'commands'
      ? [...recentCommands, ...otherCommands]
      : modeItems(mode, query, get(accountsStore), get(inboxStore));
  $: modeSpecificItems = mode === 'commands' ? [] : (selectableItems as Array<AccountPaletteItem | PrPaletteItem>);
  $: if (selectedIndex >= selectableItems.length) {
    selectedIndex = Math.max(selectableItems.length - 1, 0);
  }
  $: if (palette.open && !mountedOpen) {
    mountedOpen = true;
    query = palette.initialQuery ?? '';
    selectedIndex = 0;
    scheduleFilter(true);
    queueMicrotask(() => {
      searchInput?.focus();
      searchInput?.select();
    });
  }
  $: if (!palette.open && mountedOpen) {
    mountedOpen = false;
    query = '';
    selectedIndex = 0;
  }

  function modeItems(
    currentMode: PaletteMode,
    currentQuery: string,
    accounts: AuthAccount[],
    inboxRows: InboxItem[]
  ): Array<AccountPaletteItem | PrPaletteItem> {
    if (currentMode === 'account-switch') {
      const needle = currentQuery.trim().toLowerCase();
      return accounts
        .filter((account) =>
          needle.length === 0
            ? true
            : `${account.login} ${account.host}`.toLowerCase().includes(needle)
        )
        .map((account) => ({
          kind: 'account' as const,
          id: `${account.host}:${account.login}`,
          label: `@${account.login} · ${account.host}`
        }));
    }
    if (currentMode === 'pr-open-number') {
      const digits = currentQuery.replace(/[^\d]/g, '');
      return inboxRows
        .filter((row) => (digits.length === 0 ? true : String(row.pr_number).includes(digits)))
        .sort((left, right) => left.pr_number - right.pr_number)
        .map((row) => ({
          kind: 'pr' as const,
          id: row.pr_id,
          row,
          label: `#${row.pr_number} ${row.repo_owner}/${row.repo_name}`
        }));
    }
    return [];
  }

  function normalize(text: string): string {
    return text.trim().toLowerCase();
  }

  function refreshCommands(): void {
    const nextContext: CommandContext = contextFactory();
    allCommands = listCommands(nextContext).filter((command) => !command.id.startsWith('palette.navigate'));
  }

  function fuzzyScore(text: string, queryText: string): number {
    const haystack = text.toLowerCase();
    const needle = queryText.toLowerCase();
    let cursor = 0;
    let score = 0;
    let streak = 0;
    for (const ch of needle) {
      const index = haystack.indexOf(ch, cursor);
      if (index < 0) {
        return Number.NEGATIVE_INFINITY;
      }
      if (index === cursor) {
        streak += 1;
        score += 8 + streak * 2;
      } else {
        streak = 0;
        score += Math.max(1, 6 - (index - cursor));
      }
      cursor = index + 1;
    }
    score += Math.max(0, 24 - Math.max(0, haystack.length - needle.length));
    return score;
  }

  function scoreCommands(commands: Command[], queryText: string): Command[] {
    const needle = normalize(queryText);
    if (!needle) {
      return commands;
    }
    const scored: ScoredCommand[] = [];
    for (const command of commands) {
      const haystack = `${command.title} ${(command.keywords ?? []).join(' ')}`;
      const score = fuzzyScore(haystack, needle);
      if (score === Number.NEGATIVE_INFINITY) {
        continue;
      }
      scored.push({ command, score });
    }
    scored.sort((left, right) => right.score - left.score || left.command.title.localeCompare(right.command.title));
    return scored.map((entry) => entry.command);
  }

  function scheduleFilter(force = false): void {
    if (mode !== 'commands') {
      return;
    }
    if (debounceHandle) {
      clearTimeout(debounceHandle);
      debounceHandle = null;
    }
    if (idleHandle !== null && typeof window !== 'undefined' && 'cancelIdleCallback' in window) {
      (window as Window & { cancelIdleCallback: (id: number) => void }).cancelIdleCallback(idleHandle);
      idleHandle = null;
    }
    const run = () => {
      refreshCommands();
      filteredCommands = scoreCommands(allCommands, query);
      selectedIndex = 0;
    };
    if (force) {
      run();
      return;
    }
    debounceHandle = setTimeout(() => {
      debounceHandle = null;
      if (typeof window !== 'undefined' && 'requestIdleCallback' in window) {
        idleHandle = (window as Window & { requestIdleCallback: (callback: IdleRequestCallback) => number }).requestIdleCallback(
          () => {
            idleHandle = null;
            run();
          }
        );
        return;
      }
      run();
    }, 16);
  }

  function filterRecentCommands(commands: Command[], ids: string[]): Command[] {
    const byId = new Map(commands.map((command) => [command.id, command]));
    const ordered: Command[] = [];
    for (const id of ids.slice(0, 5)) {
      const matched = byId.get(id);
      if (matched) {
        ordered.push(matched);
      }
    }
    return ordered;
  }

  function groupBySection(commands: Command[]): Array<{ section: string; commands: Command[] }> {
    const grouped = new Map<string, Command[]>();
    for (const command of commands) {
      const existing = grouped.get(command.section) ?? [];
      existing.push(command);
      grouped.set(command.section, existing);
    }
    return [...grouped.entries()].map(([section, sectionCommands]) => ({
      section,
      commands: sectionCommands
    }));
  }

  function flattenRows(groups: Array<{ section: string; commands: Command[] }>): RowModel[] {
    const rows: RowModel[] = [];
    for (const group of groups) {
      rows.push({ kind: 'section', id: `section:${group.section}`, title: group.section });
      for (const command of group.commands) {
        rows.push({ kind: 'command', command });
      }
    }
    return rows;
  }

  function moveSelection(delta: number): void {
    if (selectableItems.length === 0) {
      selectedIndex = 0;
      return;
    }
    selectedIndex = (selectedIndex + delta + selectableItems.length) % selectableItems.length;
    if (resultsScrollElement) {
      const node = resultsScrollElement.querySelector<HTMLElement>(
        `[data-command-index="${selectedIndex}"]`
      );
      node?.scrollIntoView({ block: 'nearest' });
    }
  }

  async function confirmSelection(): Promise<void> {
    if (!palette.open) {
      return;
    }
    if (mode === 'commands') {
      const selected = selectableItems[selectedIndex];
      if (!selected || 'kind' in selected) {
        return;
      }
      const beforeMode = get(commandPaletteState).mode;
      await executeCommand(selected.id, contextFactory());
      const afterMode = get(commandPaletteState).mode;
      if (beforeMode === 'commands' && afterMode === 'commands') {
        closeCommandPalette();
      }
      return;
    }
    if (mode === 'account-switch') {
      const selected = selectableItems[selectedIndex];
      if (!selected || !('kind' in selected) || selected.kind !== 'account') {
        return;
      }
      await selectAccountById(selected.id);
      closeCommandPalette();
      return;
    }
    if (mode === 'pr-open-number') {
      const selected = selectableItems[selectedIndex];
      if (!selected || !('kind' in selected) || selected.kind !== 'pr') {
        return;
      }
      if (selected.row.account_id !== get(activeAccountIdStore)) {
        await selectAccountById(selected.row.account_id);
      }
      contextFactory().navigate(`/pr/${selected.row.pr_id}`);
      closeCommandPalette();
    }
  }

  function dismiss(): void {
    closeCommandPalette();
  }

  function onCommandRowClick(index: number): void {
    selectedIndex = index;
    void confirmSelection();
  }

  function shortcutChunks(shortcut: string | undefined): string[] {
    if (!shortcut) {
      return [];
    }
    return shortcut.split(' / ');
  }

  onMount(() => {
    setPaletteController({
      navigateUp: () => moveSelection(-1),
      navigateDown: () => moveSelection(1),
      confirm: () => void confirmSelection(),
      dismiss
    });
  });

  onDestroy(() => {
    setPaletteController(null);
    if (debounceHandle) {
      clearTimeout(debounceHandle);
      debounceHandle = null;
    }
  });
</script>

<div
  class={`command-palette-backdrop ${palette.open ? 'is-open' : ''}`}
  role="presentation"
  aria-hidden={!palette.open}
  data-testid="command-palette-root"
>
  <div
    class="command-palette-dialog Box"
    role="dialog"
    aria-modal="true"
    aria-label="Command palette"
    data-testid="command-palette-modal"
  >
    <div class="Box-header">
      <input
        bind:this={searchInput}
        class="form-control width-full"
        type="search"
        bind:value={query}
        on:input={() => scheduleFilter()}
        placeholder={
          mode === 'commands'
            ? 'Type a command…'
            : mode === 'account-switch'
              ? 'Switch account…'
              : 'Open PR by number…'
        }
        data-testid="command-palette-search"
      />
    </div>
    <div class="Box-body command-palette-results" bind:this={resultsScrollElement}>
      {#if mode === 'commands'}
        {#if recentCommands.length > 0}
          <div class="command-section sticky-section">Recently used</div>
          {#each recentCommands as command, index (command.id)}
            <button
              class={`command-row ${index === selectedIndex ? 'is-selected' : ''}`}
              type="button"
              data-command-index={index}
              on:mouseenter={() => (selectedIndex = index)}
              on:click={() => onCommandRowClick(index)}
            >
              <span class="command-title">{command.title}</span>
              <span class="Label Label--secondary">{command.section}</span>
              <span class="command-shortcut">
                {#each shortcutChunks(command.shortcut) as chunk}
                  <kbd>{chunk}</kbd>
                {/each}
              </span>
            </button>
          {/each}
        {/if}

        {#if usesVirtualizedList}
          <div class="virtual-list" style={`height: ${$virtualizer.getTotalSize()}px`}>
            {#each virtualItems as item (item.key)}
              {@const row = otherRows[item.index]}
              {#if row}
                <div style={`transform: translateY(${item.start}px); height: ${item.size}px`}>
                  {#if row.kind === 'section'}
                    <div class="command-section">{row.title}</div>
                  {:else}
                    {@const absoluteIndex = recentCommands.length + otherCommands.findIndex((entry) => entry.id === row.command.id)}
                    <button
                      class={`command-row ${absoluteIndex === selectedIndex ? 'is-selected' : ''}`}
                      type="button"
                      data-command-index={absoluteIndex}
                      data-testid={`command-palette-item-${row.command.id}`}
                      on:mouseenter={() => (selectedIndex = absoluteIndex)}
                      on:click={() => onCommandRowClick(absoluteIndex)}
                    >
                      <span class="command-title">{row.command.title}</span>
                      <span class="Label Label--secondary">{row.command.section}</span>
                      <span class="command-shortcut">
                        {#each shortcutChunks(row.command.shortcut) as chunk}
                          <kbd>{chunk}</kbd>
                        {/each}
                      </span>
                    </button>
                  {/if}
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          {#each groupBySection(otherCommands) as group}
            <div class="command-section">{group.section}</div>
            {#each group.commands as command}
              {@const absoluteIndex = recentCommands.length + otherCommands.findIndex((entry) => entry.id === command.id)}
              <button
                class={`command-row ${absoluteIndex === selectedIndex ? 'is-selected' : ''}`}
                type="button"
                data-command-index={absoluteIndex}
                data-testid={`command-palette-item-${command.id}`}
                on:mouseenter={() => (selectedIndex = absoluteIndex)}
                on:click={() => onCommandRowClick(absoluteIndex)}
              >
                <span class="command-title">{command.title}</span>
                <span class="Label Label--secondary">{command.section}</span>
                <span class="command-shortcut">
                  {#each shortcutChunks(command.shortcut) as chunk}
                    <kbd>{chunk}</kbd>
                  {/each}
                </span>
              </button>
            {/each}
          {/each}
        {/if}

        {#if selectableItems.length === 0}
          <p class="f6 color-fg-muted m-0">No commands match this search.</p>
        {/if}
      {:else}
        {#if selectableItems.length === 0}
          <p class="f6 color-fg-muted m-0">No results.</p>
        {:else}
          {#each modeSpecificItems as item, index (item.id)}
            <button
              class={`command-row ${index === selectedIndex ? 'is-selected' : ''}`}
              type="button"
              data-command-index={index}
              on:mouseenter={() => (selectedIndex = index)}
              on:click={() => {
                selectedIndex = index;
                void confirmSelection();
              }}
            >
              <span class="command-title">{item.label}</span>
            </button>
          {/each}
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .command-palette-backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 35%);
    display: none;
    align-items: flex-start;
    justify-content: center;
    z-index: 120;
    padding-top: 8vh;
  }

  .command-palette-backdrop.is-open {
    display: flex;
  }

  .command-palette-dialog {
    width: min(820px, 94vw);
    max-height: 82vh;
    display: grid;
    grid-template-rows: auto minmax(280px, 1fr);
    overflow: hidden;
  }

  .command-palette-results {
    overflow: auto;
    display: grid;
    gap: 4px;
    padding: 8px;
  }

  .command-section {
    font-size: 12px;
    font-weight: 600;
    color: var(--fgColor-muted, #57606a);
    padding: 4px 8px;
  }

  .sticky-section {
    position: sticky;
    top: -8px;
    background: var(--bgColor-default, #fff);
    z-index: 1;
  }

  .command-row {
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 8px;
    background: transparent;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 8px;
    align-items: center;
    text-align: left;
  }

  .command-row.is-selected {
    border-color: var(--borderColor-accent-emphasis, #0969da);
    background: var(--bgColor-accent-muted, #ddf4ff);
  }

  .command-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .command-shortcut {
    display: inline-flex;
    gap: 4px;
  }

  .command-shortcut kbd {
    font-size: 11px;
    line-height: 1;
  }

  .virtual-list {
    position: relative;
  }
</style>

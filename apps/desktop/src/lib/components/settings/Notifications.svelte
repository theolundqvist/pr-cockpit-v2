<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import type {
    NotificationEventPayload,
    NotificationEventRow,
    NotificationRule
  } from '$lib/ipc/bindings';
  import {
    listenEventPayload,
    listNotificationEvents,
    listNotificationRules,
    markNotificationEventSeen,
    setFocusMode,
    setNotificationRule,
    setPerRepoFilters,
    setQuietHours
  } from '$lib/ipc/client';

  export let accountId: string | null = null;

  const triggerKinds = [
    'review_requested',
    'changes_requested',
    'approved',
    'mention',
    'ci_fail',
    'ci_recover',
    'merge_conflict',
    'mutation_failure'
  ];

  const dayLabels = [
    { id: 0, label: 'Sun' },
    { id: 1, label: 'Mon' },
    { id: 2, label: 'Tue' },
    { id: 3, label: 'Wed' },
    { id: 4, label: 'Thu' },
    { id: 5, label: 'Fri' },
    { id: 6, label: 'Sat' }
  ];

  let rules: NotificationRule[] = [];
  let events: NotificationEventRow[] = [];
  let loadedAccountId: string | null = null;
  let quietStart = '22:00';
  let quietEnd = '08:00';
  let quietDays = [1, 2, 3, 4, 5];
  let timezone =
    Intl.DateTimeFormat().resolvedOptions().timeZone && Intl.DateTimeFormat().resolvedOptions().timeZone.length > 0
      ? Intl.DateTimeFormat().resolvedOptions().timeZone
      : 'UTC';
  let focusMode = false;
  let allowText = '';
  let denyText = '';
  let saving = false;
  let unlistenNotification: (() => void) | null = null;

  const tzOptions =
    typeof Intl.supportedValuesOf === 'function'
      ? Intl.supportedValuesOf('timeZone')
      : ['UTC', 'America/Los_Angeles', 'America/New_York', 'Europe/London', 'Asia/Tokyo'];

  onMount(async () => {
    if (accountId) {
      await loadForAccount(accountId);
    }
  });

  onDestroy(() => {
    unlistenNotification?.();
    unlistenNotification = null;
  });

  $: if (accountId && accountId !== loadedAccountId) {
    void loadForAccount(accountId);
  }

  async function loadForAccount(nextAccountId: string): Promise<void> {
    loadedAccountId = nextAccountId;
    rules = await listNotificationRules(nextAccountId);
    events = await listNotificationEvents(nextAccountId, 50, 0, null);
    await subscribeNotificationEvents(nextAccountId);
  }

  async function subscribeNotificationEvents(nextAccountId: string): Promise<void> {
    unlistenNotification?.();
    unlistenNotification = await listenEventPayload<NotificationEventPayload>(
      'notification:event',
      async (payload) => {
        if (payload.account_id !== nextAccountId) {
          return;
        }
        events = [
          {
            id: payload.event_id,
            account_id: payload.account_id,
            repo_id: payload.repo_id,
            pr_id: payload.pr_id,
            event_type: payload.event_type,
            actor_id: payload.actor_id,
            server_event_id: payload.server_event_id,
            title: payload.title,
            body: payload.body,
            fired_at: payload.fired_at,
            deduped: payload.deduped,
            seen: false
          },
          ...events.filter((event) => event.id !== payload.event_id)
        ].slice(0, 50);
      }
    );
  }

  function triggerEnabled(kind: string): boolean {
    return rules.find((rule) => rule.kind === kind)?.enabled ?? true;
  }

  async function toggleTrigger(kind: string, enabled: boolean): Promise<void> {
    if (!accountId) {
      return;
    }
    await setNotificationRule(accountId, kind, enabled, '{}');
    rules = await listNotificationRules(accountId);
  }

  function toggleDay(day: number): void {
    if (quietDays.includes(day)) {
      quietDays = quietDays.filter((value) => value !== day);
      return;
    }
    quietDays = [...quietDays, day].sort((left, right) => left - right);
  }

  async function saveQuietHours(): Promise<void> {
    if (!accountId) {
      return;
    }
    saving = true;
    await setQuietHours(
      accountId,
      JSON.stringify({
        start: quietStart,
        end: quietEnd,
        tz: timezone,
        days: quietDays
      })
    );
    saving = false;
  }

  async function saveFocusMode(on: boolean): Promise<void> {
    if (!accountId) {
      return;
    }
    focusMode = on;
    await setFocusMode(accountId, on);
  }

  async function saveRepoFilters(): Promise<void> {
    if (!accountId) {
      return;
    }
    const allow = allowText
      .split(/\s|,/g)
      .map((entry) => entry.trim())
      .filter(Boolean);
    const deny = denyText
      .split(/\s|,/g)
      .map((entry) => entry.trim())
      .filter(Boolean);
    await setPerRepoFilters(accountId, allow, deny);
  }

  async function markSeen(eventId: string): Promise<void> {
    await markNotificationEventSeen(eventId);
    events = events.map((event) => (event.id === eventId ? { ...event, seen: true } : event));
  }
</script>

{#if !accountId}
  <div class="flash flash-warn">Select an account to configure notification settings.</div>
{:else}
  <div class="d-flex flex-column gap-3">
    <section class="Box">
      <div class="Box-header">
        <h2 class="f5 m-0">Triggers</h2>
      </div>
      <div class="Box-body">
        <div class="d-flex flex-column gap-2">
          {#each triggerKinds as kind}
            <label class="d-flex flex-items-center gap-2">
              <input
                type="checkbox"
                checked={triggerEnabled(kind)}
                on:change={(event) => toggleTrigger(kind, (event.currentTarget as HTMLInputElement).checked)}
              />
              <span class="text-mono f6">{kind}</span>
            </label>
          {/each}
        </div>
      </div>
    </section>

    <section class="Box">
      <div class="Box-header">
        <h2 class="f5 m-0">Quiet hours</h2>
      </div>
      <div class="Box-body d-flex flex-column gap-2">
        <div class="d-flex gap-2 flex-items-center">
          <label class="f6">
            Start
            <input class="form-control mt-1" type="time" bind:value={quietStart} />
          </label>
          <label class="f6">
            End
            <input class="form-control mt-1" type="time" bind:value={quietEnd} />
          </label>
        </div>
        <label class="f6">
          Timezone
          <select class="form-select mt-1" bind:value={timezone}>
            {#each tzOptions as tz}
              <option value={tz}>{tz}</option>
            {/each}
          </select>
        </label>
        <div class="d-flex flex-wrap gap-1">
          {#each dayLabels as day}
            <button
              class={`btn btn-sm ${quietDays.includes(day.id) ? 'btn-primary' : ''}`}
              type="button"
              on:click={() => toggleDay(day.id)}
            >
              {day.label}
            </button>
          {/each}
        </div>
        <button class="btn btn-sm" type="button" disabled={saving} on:click={saveQuietHours}>
          {saving ? 'Saving…' : 'Save quiet hours'}
        </button>
      </div>
    </section>

    <section class="Box">
      <div class="Box-header">
        <h2 class="f5 m-0">Focus mode</h2>
      </div>
      <div class="Box-body">
        <label class="d-flex flex-items-center gap-2">
          <input type="checkbox" checked={focusMode} on:change={(event) => saveFocusMode((event.currentTarget as HTMLInputElement).checked)} />
          <span>Suppress all OS notifications</span>
        </label>
      </div>
    </section>

    <section class="Box">
      <div class="Box-header">
        <h2 class="f5 m-0">Per-repo filters</h2>
      </div>
      <div class="Box-body d-flex flex-column gap-2">
        <label class="f6">
          Allow list (owner/name, comma or whitespace separated)
          <textarea class="form-control mt-1" rows={3} bind:value={allowText}></textarea>
        </label>
        <label class="f6">
          Deny list (owner/name, comma or whitespace separated)
          <textarea class="form-control mt-1" rows={3} bind:value={denyText}></textarea>
        </label>
        <button class="btn btn-sm" type="button" on:click={saveRepoFilters}>Save repo filters</button>
      </div>
    </section>

    <section class="Box">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Recent notification events</h2>
        <button
          class="btn btn-sm"
          type="button"
          on:click={async () => accountId && (events = await listNotificationEvents(accountId, 50, 0, null))}
        >
          Refresh
        </button>
      </div>
      <div class="Box-body">
        {#if events.length === 0}
          <p class="color-fg-muted f6 mb-0">No notification events recorded yet.</p>
        {:else}
          <ul class="list-style-none m-0">
            {#each events as event}
              <li class="border-bottom color-border-muted py-2 d-flex flex-items-center gap-2">
                <span class="Label">{event.event_type}</span>
                <span class="flex-auto">{event.title}</span>
                {#if !event.seen}
                  <button class="btn btn-sm" type="button" on:click={() => markSeen(event.id)}>
                    Mark seen
                  </button>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </section>
  </div>
{/if}

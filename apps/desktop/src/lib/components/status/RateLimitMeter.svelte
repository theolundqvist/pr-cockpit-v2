<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { listenEventPayload } from '$lib/ipc/client';
  import type {
    AuthAccount,
    RateLimitBucket,
    RateLimitBypassEventPayload,
    RateLimitPressureEventPayload
  } from '$lib/ipc/bindings';

  export let rateLimits: RateLimitBucket[] = [];
  export let accounts: AuthAccount[] = [];
  export let activeAccountId: string | null = null;
  export let selectedAccountFilter: string | null = null;

  let unlistenPressure: (() => void) | null = null;
  let unlistenBypass: (() => void) | null = null;
  let tickTimer: ReturnType<typeof setInterval> | null = null;
  let nowEpoch = Math.floor(Date.now() / 1000);
  let pulseUntil = 0;
  let throttledAccountId: string | null = null;
  let bypassEvents: Array<{ account_id: string; fired_at: number }> = [];

  $: accountById = new Map(accounts.map((account) => [`${account.host}:${account.login}`, account]));
  $: grouped = groupRateLimits(rateLimits);
  $: allMode = selectedAccountFilter === null;
  $: focusedAccountId = selectedAccountFilter ?? activeAccountId;
  $: focusedResources = focusedAccountId ? grouped.get(focusedAccountId) ?? emptyResources() : emptyResources();
  $: stackedRows = [...grouped.entries()].sort(([left], [right]) => left.localeCompare(right));
  $: bypassCount = bypassEvents.filter(
    (event) =>
      event.fired_at >= nowEpoch - 30 &&
      (selectedAccountFilter === null || event.account_id === selectedAccountFilter)
  ).length;
  $: showPulse = nowEpoch <= pulseUntil;
  $: showThrottleChip =
    showPulse &&
    throttledAccountId !== null &&
    (allMode || throttledAccountId === focusedAccountId);

  onMount(async () => {
    tickTimer = setInterval(() => {
      nowEpoch = Math.floor(Date.now() / 1000);
      bypassEvents = bypassEvents.filter((event) => event.fired_at >= nowEpoch - 30);
    }, 1000);
    unlistenPressure = await listenEventPayload<RateLimitPressureEventPayload>(
      'rate_limit_pressure:<account>',
      (payload) => {
        nowEpoch = Math.floor(Date.now() / 1000);
        throttledAccountId = payload.account_id;
        pulseUntil = nowEpoch + 8;
      }
    );
    unlistenBypass = await listenEventPayload<RateLimitBypassEventPayload>(
      'rate_limit_bypass:<account>',
      (payload) => {
        nowEpoch = Math.floor(Date.now() / 1000);
        bypassEvents = [...bypassEvents, { account_id: payload.account_id, fired_at: nowEpoch }].filter(
          (event) => event.fired_at >= nowEpoch - 30
        );
      }
    );
  });

  onDestroy(() => {
    unlistenPressure?.();
    unlistenBypass?.();
    if (tickTimer) {
      clearInterval(tickTimer);
    }
  });

  function emptyResources(): { graphql: RateLimitBucket | null; core: RateLimitBucket | null } {
    return { graphql: null, core: null };
  }

  function groupRateLimits(
    rows: RateLimitBucket[]
  ): Map<string, { graphql: RateLimitBucket | null; core: RateLimitBucket | null }> {
    const next = new Map<string, { graphql: RateLimitBucket | null; core: RateLimitBucket | null }>();
    for (const row of rows) {
      const bucket = next.get(row.account_id) ?? emptyResources();
      if (row.resource.toLowerCase() === 'graphql') {
        bucket.graphql = row;
      } else if (row.resource.toLowerCase() === 'core') {
        bucket.core = row;
      }
      next.set(row.account_id, bucket);
    }
    return next;
  }

  function ratio(row: RateLimitBucket | null): number {
    if (!row || row.limit_total <= 0) {
      return 0;
    }
    return Math.max(0, Math.min(1, row.remaining / row.limit_total));
  }

  function progressClass(row: RateLimitBucket | null): string {
    const value = ratio(row);
    if (value <= 0.2) {
      return 'color-bg-danger-emphasis';
    }
    if (value <= 0.5) {
      return 'color-bg-attention-emphasis';
    }
    return 'color-bg-success-emphasis';
  }

  function progressWidth(row: RateLimitBucket | null): string {
    return `${Math.round(ratio(row) * 100)}%`;
  }

  function formatReset(resetAt: number | null | undefined): string {
    if (!resetAt) {
      return 'unknown';
    }
    const delta = resetAt - nowEpoch;
    if (delta <= 0) {
      return 'now';
    }
    if (delta >= 3600) {
      return `${Math.ceil(delta / 3600)}h`;
    }
    if (delta >= 60) {
      return `${Math.ceil(delta / 60)}m`;
    }
    return `${delta}s`;
  }

  function accountLabel(accountId: string): string {
    const account = accountById.get(accountId);
    if (!account) {
      return accountId;
    }
    return `@${account.login} · ${account.host}`;
  }

</script>

<section class={`rate-limit-meter ${showPulse ? 'rate-limit-meter--pulse' : ''}`} aria-label="Rate limit meter">
  <div class="d-flex flex-items-center gap-2 mb-1">
    <strong class="f6">Rate limits</strong>
    {#if showThrottleChip}
      <span class="Label Label--danger" data-testid="meter-throttled-chip">background throttled</span>
    {/if}
    {#if bypassCount > 0}
      <span class="Label Label--attention" data-testid="meter-bypass-chip">
        foreground bypassed throttle <span class="Counter">{bypassCount}</span>
      </span>
    {/if}
  </div>

  {#if allMode}
    {#if stackedRows.length === 0}
      <div class="f6 color-fg-muted">No budget data</div>
    {:else}
      <div class="d-flex flex-column gap-2" data-testid="meter-all-accounts">
        {#each stackedRows as [accountId, resources]}
          <div class="Box p-2">
            <div class="f6 text-bold mb-1">{accountLabel(accountId)}</div>
            <div class="f6 color-fg-muted">
              GraphQL: {resources.graphql?.remaining ?? 0}/{resources.graphql?.limit_total ?? 0} · resets in
              {formatReset(resources.graphql?.reset_at)}
            </div>
            <div class="Progress mb-1 mt-1" aria-label={`GraphQL budget for ${accountLabel(accountId)}`}>
              <span
                class={`Progress-item ${progressClass(resources.graphql)}`}
                style={`width: ${progressWidth(resources.graphql)}`}
              ></span>
            </div>
            <div class="f6 color-fg-muted">
              REST: {resources.core?.remaining ?? 0}/{resources.core?.limit_total ?? 0} · resets in
              {formatReset(resources.core?.reset_at)}
            </div>
            <div class="Progress mt-1" aria-label={`REST budget for ${accountLabel(accountId)}`}>
              <span
                class={`Progress-item ${progressClass(resources.core)}`}
                style={`width: ${progressWidth(resources.core)}`}
              ></span>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="f6 color-fg-muted" data-testid="meter-active-account">
      GraphQL: {focusedResources.graphql?.remaining ?? 0}/{focusedResources.graphql?.limit_total ?? 0} points · resets in
      {formatReset(focusedResources.graphql?.reset_at)}
    </div>
    <div class="Progress mb-1 mt-1" aria-label="GraphQL budget">
      <span
        class={`Progress-item ${progressClass(focusedResources.graphql)}`}
        style={`width: ${progressWidth(focusedResources.graphql)}`}
      ></span>
    </div>
    <div class="f6 color-fg-muted">
      REST: {focusedResources.core?.remaining ?? 0}/{focusedResources.core?.limit_total ?? 0} · resets in
      {formatReset(focusedResources.core?.reset_at)}
    </div>
    <div class="Progress mt-1" aria-label="REST budget">
      <span
        class={`Progress-item ${progressClass(focusedResources.core)}`}
        style={`width: ${progressWidth(focusedResources.core)}`}
      ></span>
    </div>
  {/if}
</section>

<style>
  .rate-limit-meter {
    min-width: 280px;
  }

  .rate-limit-meter--pulse {
    animation: meter-pulse 600ms ease-in-out 4;
  }

  @keyframes meter-pulse {
    0% {
      box-shadow: 0 0 0 0 rgb(207 34 46 / 45%);
    }
    100% {
      box-shadow: 0 0 0 10px rgb(207 34 46 / 0%);
    }
  }
</style>

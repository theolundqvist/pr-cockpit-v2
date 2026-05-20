<script lang="ts">
  import { onDestroy } from 'svelte';

  import { listenEventPayload } from '$lib/ipc/client';

  export let accountId: string;
  export let prId: string;
  export let mergeableState: string | null;

  let attempt = 0;
  let remainingSeconds = 0;
  let unlisten: (() => void) | null = null;
  let countdownTimer: ReturnType<typeof setInterval> | null = null;
  let listenerKey = '';

  function clearCountdown(): void {
    if (countdownTimer) {
      clearInterval(countdownTimer);
      countdownTimer = null;
    }
  }

  function startCountdown(): void {
    clearCountdown();
    countdownTimer = setInterval(() => {
      remainingSeconds = Math.max(remainingSeconds - 1, 0);
    }, 1000);
  }

  function resetState(): void {
    clearCountdown();
    attempt = 0;
    remainingSeconds = 0;
  }

  async function ensureListener(): Promise<void> {
    if (mergeableState !== null) {
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      listenerKey = '';
      resetState();
      return;
    }

    const key = `${accountId}:${prId}`;
    if (key === listenerKey && unlisten) {
      return;
    }

    if (unlisten) {
      unlisten();
      unlisten = null;
    }

    listenerKey = key;
    unlisten = await listenEventPayload<{
      account_id: string;
      pr_id: string;
      attempt: number;
      next_sleep_seconds: number;
    }>(`mergeable_backoff:${accountId}:${prId} tick`, (payload) => {
      attempt = payload.attempt;
      remainingSeconds = payload.next_sleep_seconds;
      startCountdown();
    });
  }

  $: void ensureListener();

  onDestroy(() => {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    clearCountdown();
  });
</script>

{#if mergeableState === null}
  <span class="Label Label--attention">
    Computing mergeability… next check in {remainingSeconds}s (attempt {attempt || 1}/∞)
  </span>
{/if}

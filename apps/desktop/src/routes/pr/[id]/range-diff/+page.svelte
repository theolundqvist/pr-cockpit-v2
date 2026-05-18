<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';

  import RangeDiffView from '$lib/components/range-diff/RangeDiffView.svelte';
  import { computeRangeDiff, listPrPushes } from '$lib/ipc/client';
  import type { PrPushView, RangeDiff } from '$lib/ipc/bindings';

  let loading = true;
  let pushes: PrPushView[] = [];
  let rangeDiff: RangeDiff | null = null;
  let errorMessage: string | null = null;
  let selectedOldSha = '';
  let selectedNewSha = '';

  async function loadRangeDiff(): Promise<void> {
    loading = true;
    errorMessage = null;
    rangeDiff = null;
    const prId = $page.params.id ?? '';
    if (!prId) {
      errorMessage = 'Missing pull request id.';
      loading = false;
      return;
    }
    pushes = await listPrPushes(prId);
    if (pushes.length < 2) {
      errorMessage = 'Need at least two recorded pushes.';
      loading = false;
      return;
    }

    const requestedOld = $page.url.searchParams.get('old');
    const requestedNew = $page.url.searchParams.get('new');
    const fallbackOld = pushes[pushes.length - 2];
    const fallbackNew = pushes[pushes.length - 1];

    const requestedOldPush = requestedOld
      ? pushes.find((push) => push.head_sha === requestedOld)
      : null;
    const requestedNewPush = requestedNew
      ? pushes.find((push) => push.head_sha === requestedNew)
      : null;
    const selectedOldPush = requestedOldPush ?? fallbackOld ?? null;
    const selectedNewPush = requestedNewPush ?? fallbackNew ?? null;
    if (!selectedOldPush || !selectedNewPush) {
      errorMessage = 'Need at least two recorded pushes.';
      loading = false;
      return;
    }
    selectedOldSha = selectedOldPush.head_sha;
    selectedNewSha = selectedNewPush.head_sha;

    try {
      rangeDiff = await computeRangeDiff(
        prId,
        selectedNewPush.base_sha,
        selectedOldSha,
        selectedNewSha
      );
    } catch (error) {
      errorMessage = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadRangeDiff();
  });
</script>

<main class="px-3 py-3">
  <header class="mb-3 d-flex flex-items-center flex-justify-between">
    <div>
      <h1 class="f3 m-0">Compared with previous push</h1>
      <p class="f6 color-fg-muted mb-0">
        PR <span class="text-mono">{$page.params.id}</span>
      </p>
    </div>
    <a class="btn" href={`/pr/${$page.params.id}`}>Back to pull request</a>
  </header>

  {#if loading}
    <div class="flash flash-warn">Computing range-diff…</div>
  {:else}
    <RangeDiffView
      {rangeDiff}
      oldSha={selectedOldSha}
      newSha={selectedNewSha}
      {errorMessage}
    />
  {/if}
</main>

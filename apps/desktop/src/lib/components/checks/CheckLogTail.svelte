<script lang="ts">
  import { createEventDispatcher, onDestroy, tick } from 'svelte';

  import { listenEventPayload, startCheckLogStream } from '$lib/ipc/client';

  export let checkRunId: string | null = null;
  export let tailLines = 500;

  type LogChunk = {
    kind: string;
    text: string | null;
    details_url: string | null;
  };

  const dispatch = createEventDispatcher<{ close: void }>();

  let scrollEl: HTMLDivElement | null = null;
  let streamLive = true;
  let loading = false;
  let activeStreamKey = '';
  let unlisten: (() => void) | null = null;
  let textChunks: string[] = [];
  let fallbackDetailsUrl: string | null = null;
  let pinnedToBottom = true;

  $: streamKey = checkRunId && streamLive ? `${checkRunId}:${tailLines}` : '';
  $: void ensureStream(streamKey);
  $: logText = textChunks.join('');

  async function ensureStream(nextKey: string): Promise<void> {
    if (nextKey === activeStreamKey) {
      return;
    }
    await stopStream();
    activeStreamKey = nextKey;
    if (!nextKey || !checkRunId) {
      return;
    }
    textChunks = [];
    fallbackDetailsUrl = null;
    loading = true;
    const handle = await startCheckLogStream(checkRunId, tailLines);
    unlisten = await listenEventPayload<LogChunk>(handle.event_name, async (chunk) => {
      if (chunk.kind === 'chunk' && chunk.text) {
        textChunks = [...textChunks, chunk.text];
        await maybeAutoscroll();
      } else if (chunk.kind === 'tail' && chunk.text) {
        textChunks = [...textChunks, `\n--- tail (${tailLines}) ---\n`, chunk.text, '\n'];
        await maybeAutoscroll();
      } else if (chunk.kind === 'fallback') {
        fallbackDetailsUrl = chunk.details_url ?? null;
      } else if (chunk.kind === 'done') {
        loading = false;
      } else if (chunk.kind === 'error' && chunk.text) {
        textChunks = [...textChunks, `\n[stream error] ${chunk.text}\n`];
        loading = false;
        await maybeAutoscroll();
      }
    });
  }

  async function stopStream(): Promise<void> {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    loading = false;
  }

  async function maybeAutoscroll(): Promise<void> {
    if (!pinnedToBottom || !scrollEl) {
      return;
    }
    await tick();
    if (scrollEl) {
      scrollEl.scrollTop = scrollEl.scrollHeight;
    }
  }

  function onScroll(): void {
    if (!scrollEl) {
      return;
    }
    const distance = scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    pinnedToBottom = distance < 8;
  }

  async function copyToClipboard(): Promise<void> {
    await navigator.clipboard.writeText(logText);
  }

  function closePanel(): void {
    dispatch('close');
  }

  onDestroy(() => {
    void stopStream();
  });
</script>

{#if checkRunId}
  <section class="Box mt-3" aria-label="Check log tail">
    <div class="Box-header d-flex flex-items-center flex-justify-between">
      <div class="d-flex flex-items-center gap-2">
        <span class="text-bold">Check log tail</span>
        <span class="Label Label--secondary">{checkRunId}</span>
        {#if loading}
          <span class="Label Label--attention">streaming</span>
        {/if}
      </div>
      <div class="d-flex flex-items-center gap-2">
        <label class="d-flex flex-items-center gap-1 f6">
          <input type="checkbox" bind:checked={streamLive} />
          Stream live
        </label>
        <button class="btn btn-sm" type="button" on:click={copyToClipboard}>Copy to clipboard</button>
        <button class="btn btn-sm" type="button" on:click={closePanel}>Close</button>
      </div>
    </div>
    <div class="Box-body">
      {#if fallbackDetailsUrl}
        <a class="Label Label--accent" href={fallbackDetailsUrl} target="_blank" rel="noreferrer">
          View full log on GitHub
        </a>
      {:else}
        <div class="log-view text-mono f6" bind:this={scrollEl} on:scroll={onScroll}>
          <pre>{logText}</pre>
        </div>
      {/if}
    </div>
  </section>
{/if}

<style>
  .log-view {
    max-height: 320px;
    overflow: auto;
    background: var(--bgColor-muted, #f6f8fa);
    border: 1px solid var(--borderColor-default, #d0d7de);
    border-radius: 6px;
    padding: 8px;
  }

  pre {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }
</style>

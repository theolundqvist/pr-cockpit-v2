<script lang="ts">
  import type { HighlightKind, HighlightedLine, RangeDiff } from '$lib/ipc/bindings';

  export let rangeDiff: RangeDiff | null = null;
  export let oldSha = '';
  export let newSha = '';
  export let errorMessage: string | null = null;

  let expandedRows = new Set<number>();

  function shortSha(sha: string): string {
    return sha.slice(0, 7);
  }

  function markerForStatus(status: string): string {
    if (status === 'Unchanged') {
      return '=';
    }
    if (status === 'Modified') {
      return '!';
    }
    if (status === 'Added') {
      return '+';
    }
    return '-';
  }

  function modeLabel(mode: string): string {
    return mode === 'LocalGit' ? 'Local git' : 'REST /compare';
  }

  function toggleExpanded(index: number): void {
    const next = new Set(expandedRows);
    if (next.has(index)) {
      next.delete(index);
    } else {
      next.add(index);
    }
    expandedRows = next;
  }

  function segmentsForLine(line: HighlightedLine): Array<{ start: number; end: number; kind: HighlightKind }> {
    if (line.segments.length > 0) {
      return line.segments;
    }
    return [{ start: 0, end: line.text.length, kind: 'Unchanged' }];
  }

  function segmentClass(kind: HighlightKind): string {
    if (kind === 'Added') {
      return 'color-bg-success-emphasis color-fg-on-emphasis';
    }
    if (kind === 'Removed') {
      return 'color-bg-danger-emphasis color-fg-on-emphasis';
    }
    return '';
  }
</script>

{#if errorMessage}
  <div class="flash flash-error">Cannot compute range-diff: {errorMessage}</div>
{:else if !rangeDiff || rangeDiff.commit_pairs.length === 0}
  <div class="Box">
    <div class="Box-body color-fg-muted">No diff between these pushes.</div>
  </div>
{:else}
  <section class="Box">
    <header class="Box-header range-diff-header">
      <div class="text-bold">
        Previous push (<span class="text-mono">{shortSha(oldSha || rangeDiff.old_range.head_sha)}</span>)
      </div>
      <span data-testid="range-diff-mode-badge" class="Label Label--accent">
        {modeLabel(rangeDiff.mode)}
      </span>
      <div class="text-bold text-right">
        Current push (<span class="text-mono">{shortSha(newSha || rangeDiff.new_range.head_sha)}</span>)
      </div>
    </header>

    {#each rangeDiff.commit_pairs as pair, index (index)}
      <div class="Box-row">
        <div class="d-flex flex-items-center gap-2">
          <span class="text-mono text-bold" data-testid="range-diff-marker">
            {markerForStatus(pair.status)}
          </span>
          <div class="range-diff-pair">
            <div class="text-mono">{pair.old?.title ?? '—'}</div>
            <div class="text-mono">{pair.new?.title ?? '—'}</div>
          </div>
          {#if pair.status === 'Modified' && pair.intra_diff}
            <button
              class="btn btn-sm ml-auto"
              type="button"
              aria-expanded={expandedRows.has(index)}
              on:click={() => toggleExpanded(index)}
            >
              {expandedRows.has(index) ? 'Hide intra-line diff' : 'Show intra-line diff'}
            </button>
          {/if}
        </div>

        {#if pair.status === 'Modified' && pair.intra_diff && expandedRows.has(index)}
          <div class="range-intra-container mt-2">
            {#each pair.intra_diff.hunks as hunk}
              <div class="range-intra-grid">
                <div>
                  {#each hunk.old_lines as line}
                    <div class="diff-line-content del range-line-cell">
                      {#each segmentsForLine(line) as segment}
                        <span class={segmentClass(segment.kind)} data-highlight-kind={segment.kind}>
                          {line.text.slice(segment.start, segment.end)}
                        </span>
                      {/each}
                    </div>
                  {/each}
                </div>
                <div>
                  {#each hunk.new_lines as line}
                    <div class="diff-line-content add range-line-cell">
                      {#each segmentsForLine(line) as segment}
                        <span class={segmentClass(segment.kind)} data-highlight-kind={segment.kind}>
                          {line.text.slice(segment.start, segment.end)}
                        </span>
                      {/each}
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
  .range-diff-header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: 8px;
    align-items: center;
  }

  .range-diff-pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 8px;
    width: 100%;
  }

  .range-intra-container {
    border: 1px solid var(--borderColor-muted, #d1d9e0);
    border-radius: 6px;
    overflow: hidden;
  }

  .range-intra-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 1px;
    background: var(--borderColor-muted, #d1d9e0);
  }

  .range-line-cell {
    min-height: 24px;
    overflow-x: auto;
    white-space: pre;
  }
</style>

<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual';

  import { flattenDiffRows, parsePatch, type DiffRow, type ParsedDiffFile, type ParsedDiffLine } from '$lib/diff/parser';
  import { languageFromPath } from '$lib/highlight/language';
  import { HighlightWorkerClient } from '$lib/highlight/worker-client';
  import type { PrFile, ReviewThread } from '$lib/ipc/bindings';

  export let patch = '';
  export let files: PrFile[] = [];
  export let reviewThreads: ReviewThread[] = [];

  let mode: 'unified' | 'side-by-side' = 'unified';
  let scrollElement: HTMLDivElement | null = null;
  const highlighter = new HighlightWorkerClient();
  const highlightByFile = new Map<string, Map<number, { start: number; end: number; kind: string }[]>>();
  const filesWithManualHighlight = new Set<string>();
  let parsedFiles: ParsedDiffFile[] = [];
  let rows: DiffRow[] = [];
  let threadByRowId = new Map<string, ReviewThread[]>();

  $: parsedFiles = parsePatch(patch);
  $: rows = flattenDiffRows(parsedFiles);
  $: threadByRowId = buildThreadIndex(rows, reviewThreads);

  const virtualizer = createVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElement,
    estimateSize: (index) => {
      const row = rows[index];
      if (!row) {
        return 28;
      }
      return row.kind === 'line' ? 24 : 34;
    },
    overscan: 18
  });

  let visibleItems: Array<{ index: number; key: number; start: number; size: number }> = [];

  $: $virtualizer.setOptions({
    count: rows.length,
    getScrollElement: () => scrollElement
  });

  $: visibleItems = $virtualizer.getVirtualItems() as Array<{
    index: number;
    key: number;
    start: number;
    size: number;
  }>;
  $: void requestViewportHighlight(visibleItems, rows, parsedFiles);

  function fileLineCount(file: ParsedDiffFile): number {
    const metadata = files.find((entry) => entry.path === file.path);
    if (metadata) {
      return metadata.additions + metadata.deletions;
    }
    let maxLine = 0;
    for (const hunk of file.hunks) {
      for (const line of hunk.lines) {
        maxLine = Math.max(maxLine, line.rightLine ?? line.leftLine ?? 0);
      }
    }
    return maxLine;
  }

  function shouldHighlight(file: ParsedDiffFile): boolean {
    const lineCount = fileLineCount(file);
    return lineCount <= 5000 || filesWithManualHighlight.has(file.path);
  }

  function enableHighlight(filePath: string): void {
    filesWithManualHighlight.add(filePath);
  }

  function fileSource(file: ParsedDiffFile): string {
    let maxLine = 0;
    for (const hunk of file.hunks) {
      for (const line of hunk.lines) {
        maxLine = Math.max(maxLine, line.rightLine ?? 0);
      }
    }
    const lines = Array.from({ length: Math.max(1, maxLine) }, () => '');
    for (const hunk of file.hunks) {
      for (const line of hunk.lines) {
        if (line.rightLine) {
          lines[line.rightLine - 1] = line.text;
        }
      }
    }
    return lines.join('\n');
  }

  async function requestViewportHighlight(
    virtualRows: Array<{ index: number }> | undefined,
    diffRows: DiffRow[],
    diffFiles: ParsedDiffFile[]
  ): Promise<void> {
    if (!virtualRows || virtualRows.length === 0) {
      return;
    }
    const requests = new Map<string, { start: number; end: number }>();
    for (const virtualRow of virtualRows) {
      const row = diffRows[virtualRow.index];
      if (!row || row.kind !== 'line') {
        continue;
      }
      if (!shouldHighlight(row.file)) {
        continue;
      }
      const lineNumber = row.line.rightLine ?? row.line.leftLine;
      if (!lineNumber) {
        continue;
      }
      const current = requests.get(row.file.path);
      if (!current) {
        requests.set(row.file.path, { start: lineNumber, end: lineNumber });
      } else {
        current.start = Math.min(current.start, lineNumber);
        current.end = Math.max(current.end, lineNumber);
      }
    }

    for (const [filePath, request] of requests) {
      const file = diffFiles.find((candidate) => candidate.path === filePath);
      if (!file) {
        continue;
      }
      const source = fileSource(file);
      const language = languageFromPath(filePath);
      const tokens = await highlighter.requestTokens(language, source, request.start, request.end);
      highlightByFile.set(filePath, tokens);
    }
  }

  function renderHighlightedText(filePath: string, lineNumber: number | null, text: string): Array<{ text: string; className: string }> {
    if (!lineNumber) {
      return [{ text, className: '' }];
    }
    const tokens = highlightByFile.get(filePath)?.get(lineNumber);
    if (!tokens || tokens.length === 0) {
      return [{ text, className: '' }];
    }
    const segments: Array<{ text: string; className: string }> = [];
    let cursor = 0;
    for (const token of tokens) {
      if (token.start > cursor) {
        segments.push({ text: text.slice(cursor, token.start), className: '' });
      }
      segments.push({ text: text.slice(token.start, token.end), className: `syntax-${token.kind}` });
      cursor = token.end;
    }
    if (cursor < text.length) {
      segments.push({ text: text.slice(cursor), className: '' });
    }
    return segments;
  }

  function sideForLine(line: ParsedDiffLine): 'LEFT' | 'RIGHT' | 'BOTH' {
    if (line.kind === 'add') {
      return 'RIGHT';
    }
    if (line.kind === 'del') {
      return 'LEFT';
    }
    return 'BOTH';
  }

  function buildThreadIndex(diffRows: DiffRow[], threads: ReviewThread[]): Map<string, ReviewThread[]> {
    const index = new Map<string, ReviewThread[]>();
    for (const row of diffRows) {
      if (row.kind !== 'line') {
        continue;
      }
      const matched = threads.filter((thread) => {
        if (thread.path !== row.file.path || !thread.line || !thread.side) {
          return false;
        }
        if (thread.side === 'RIGHT') {
          return row.line.rightLine === thread.line;
        }
        if (thread.side === 'LEFT') {
          return row.line.leftLine === thread.line;
        }
        return false;
      });
      if (matched.length > 0) {
        index.set(row.id, matched);
      }
    }
    return index;
  }
</script>

<div class="d-flex flex-items-center gap-2 mb-2">
  <span class="f6 color-fg-muted">Diff mode</span>
  <div class="BtnGroup">
    <button class={`btn btn-sm ${mode === 'unified' ? 'selected' : ''}`} type="button" on:click={() => (mode = 'unified')}>
      Unified
    </button>
    <button class={`btn btn-sm ${mode === 'side-by-side' ? 'selected' : ''}`} type="button" on:click={() => (mode = 'side-by-side')}>
      Side by side
    </button>
  </div>
</div>

<div class="diff-scroll border rounded-2" bind:this={scrollElement}>
  <div class="diff-viewport" style={`height: ${$virtualizer.getTotalSize()}px`}>
    {#each visibleItems as virtualRow (virtualRow.key)}
      {@const row = rows[virtualRow.index]}
      {#if row}
        <div class="diff-row" style={`transform: translateY(${virtualRow.start}px); height: ${virtualRow.size}px`}>
          {#if row.kind === 'file'}
            <div class="diff-file-row">
              <span class="text-mono f6">{row.file.path}</span>
              {#if fileLineCount(row.file) > 5000 && !filesWithManualHighlight.has(row.file.path)}
                <button class="btn btn-sm ml-2" type="button" on:click={() => enableHighlight(row.file.path)}>
                  Highlight anyway
                </button>
              {/if}
            </div>
          {:else if row.kind === 'hunk'}
            <div class="diff-hunk-row text-mono f6 color-fg-muted">{row.hunk.header}</div>
          {:else}
            <div class={`diff-line-row ${mode}`}>
              {#if mode === 'unified'}
                <div class="diff-line-number">{row.line.leftLine ?? ''}</div>
                <div class="diff-line-number">{row.line.rightLine ?? ''}</div>
                <div class={`diff-line-content ${row.line.kind}`}>
                  {#if row.line.kind === 'add'}
                    <span class="color-fg-success">+</span>
                  {:else if row.line.kind === 'del'}
                    <span class="color-fg-danger">-</span>
                  {:else}
                    <span class="color-fg-muted"> </span>
                  {/if}
                  {#each renderHighlightedText(row.file.path, row.line.rightLine ?? row.line.leftLine, row.line.text) as segment}
                    <span class={segment.className}>{segment.text}</span>
                  {/each}
                </div>
              {:else}
                <div class="diff-side-cell">
                  <div class="diff-line-number">{row.line.leftLine ?? ''}</div>
                  <div class={`diff-line-content ${sideForLine(row.line) === 'LEFT' || sideForLine(row.line) === 'BOTH' ? row.line.kind : ''}`}>
                    {#if row.line.kind !== 'add'}
                      {#each renderHighlightedText(row.file.path, row.line.leftLine, row.line.text) as segment}
                        <span class={segment.className}>{segment.text}</span>
                      {/each}
                    {/if}
                  </div>
                </div>
                <div class="diff-side-cell">
                  <div class="diff-line-number">{row.line.rightLine ?? ''}</div>
                  <div class={`diff-line-content ${sideForLine(row.line) === 'RIGHT' || sideForLine(row.line) === 'BOTH' ? row.line.kind : ''}`}>
                    {#if row.line.kind !== 'del'}
                      {#each renderHighlightedText(row.file.path, row.line.rightLine, row.line.text) as segment}
                        <span class={segment.className}>{segment.text}</span>
                      {/each}
                    {/if}
                  </div>
                </div>
              {/if}
            </div>
            {#if threadByRowId.get(row.id)}
              <div class="diff-thread-row">
                {#each threadByRowId.get(row.id) ?? [] as thread}
                  <details class="Box mb-1">
                    <summary class="Box-header d-flex flex-items-center gap-2">
                      <span class="text-bold">Thread {thread.id}</span>
                      <span class="color-fg-muted">{thread.comment_count} comments</span>
                      {#if thread.is_outdated}
                        <span class="Label Label--attention">Outdated</span>
                      {/if}
                    </summary>
                    <div class="Box-body f6 color-fg-muted">
                      Anchor: line {thread.line} side {thread.side} start_line {thread.start_line} start_side {thread.start_side}
                    </div>
                  </details>
                {/each}
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    {/each}
  </div>
</div>

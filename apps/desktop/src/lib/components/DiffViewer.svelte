<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { createEventDispatcher } from 'svelte';

  import Composer from '$lib/components/Composer.svelte';
  import {
    flattenDiffRows,
    parsePatch,
    type DiffRow,
    type ParsedDiffFile,
    type ParsedDiffLine
  } from '$lib/diff/parser';
  import { languageFromPath } from '$lib/highlight/language';
  import { HighlightWorkerClient } from '$lib/highlight/worker-client';
  import { getPrFileBlob } from '$lib/ipc/client';
  import type { PrFile, ReviewThread } from '$lib/ipc/bindings';

  export let patch = '';
  export let files: PrFile[] = [];
  export let reviewThreads: ReviewThread[] = [];
  export let accountId = '';
  export let prId = '';
  export let headSha = '';
  export let pullRequestNodeId = '';

  const dispatch = createEventDispatcher<{ reviewcommentsubmitted: void }>();

  let mode: 'unified' | 'side-by-side' = 'unified';
  let scrollElement: HTMLDivElement | null = null;
  const highlighter = new HighlightWorkerClient();
  const highlightByFile = new Map<string, Map<number, { start: number; end: number; kind: string }[]>>();
  const filesWithManualHighlight = new Set<string>();
  const imageAssets = new Map<string, { left?: string; right?: string }>();
  const assetLoadsInFlight = new Set<string>();
  let parsedFiles: ParsedDiffFile[] = [];
  let displayFiles: ParsedDiffFile[] = [];
  let rows: DiffRow[] = [];
  let threadByRowId = new Map<string, ReviewThread[]>();

  type SelectionAnchor = { filePath: string; side: 'LEFT' | 'RIGHT'; line: number };
  type SelectionRange = {
    filePath: string;
    side: 'LEFT' | 'RIGHT';
    startLine: number;
    endLine: number;
    seedLines: string[];
  };
  let selectionAnchor: SelectionAnchor | null = null;
  let selectionRange: SelectionRange | null = null;
  let commentButtonRowId: string | null = null;
  let inlineComposerRowId: string | null = null;

  $: parsedFiles = parsePatch(patch);
  $: displayFiles = mergePatchAndMetadata(parsedFiles, files);
  $: rows = flattenDiffRows(displayFiles);
  $: threadByRowId = buildThreadIndex(rows, reviewThreads);
  $: void preloadImageAssets(files);

  const virtualizer = createVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElement,
    estimateSize: (index) => {
      const row = rows[index];
      if (!row) {
        return 28;
      }
      if (row.kind === 'file') {
        const metadata = metadataForPath(row.file.path);
        if (metadata?.kind === 'image') {
          return 420;
        }
        if (metadata?.kind === 'binary') {
          return 140;
        }
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
  $: void requestViewportHighlight(visibleItems, rows, displayFiles);

  function metadataForPath(path: string): PrFile | undefined {
    return files.find((entry) => entry.path === path);
  }

  function mergePatchAndMetadata(parsed: ParsedDiffFile[], metadataFiles: PrFile[]): ParsedDiffFile[] {
    const byPath = new Map(parsed.map((file) => [file.path, file]));
    const merged = [...parsed];
    for (const metadata of metadataFiles) {
      if (byPath.has(metadata.path)) {
        continue;
      }
      merged.push({
        oldPath: metadata.previous_path ?? metadata.old_path ?? metadata.path,
        newPath: metadata.path,
        path: metadata.path,
        hunks: []
      });
    }
    return merged;
  }

  function fileLineCount(file: ParsedDiffFile): number {
    const metadata = metadataForPath(file.path);
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
      if (!file || file.hunks.length === 0) {
        continue;
      }
      const source = fileSource(file);
      const language = languageFromPath(filePath);
      const tokens = await highlighter.requestTokens(language, source, request.start, request.end);
      highlightByFile.set(filePath, tokens);
    }
  }

  function renderHighlightedText(
    filePath: string,
    lineNumber: number | null,
    text: string
  ): Array<{ text: string; className: string }> {
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

  function lineForSide(row: DiffRow & { kind: 'line' }, side: 'LEFT' | 'RIGHT'): number | null {
    return side === 'RIGHT' ? row.line.rightLine : row.line.leftLine;
  }

  function collectSelectedLines(
    filePath: string,
    side: 'LEFT' | 'RIGHT',
    startLine: number,
    endLine: number
  ): string[] {
    const selected: string[] = [];
    for (const row of rows) {
      if (row.kind !== 'line' || row.file.path !== filePath) {
        continue;
      }
      const lineNumber = lineForSide(row, side);
      if (!lineNumber || lineNumber < startLine || lineNumber > endLine) {
        continue;
      }
      selected.push(row.line.text);
    }
    return selected;
  }

  function onLineNumberClick(
    row: DiffRow & { kind: 'line' },
    side: 'LEFT' | 'RIGHT',
    event: MouseEvent
  ): void {
    const line = lineForSide(row, side);
    if (!line) {
      return;
    }
    if (
      !event.shiftKey ||
      !selectionAnchor ||
      selectionAnchor.filePath !== row.file.path ||
      selectionAnchor.side !== side
    ) {
      selectionAnchor = { filePath: row.file.path, side, line };
      selectionRange = null;
      commentButtonRowId = null;
      inlineComposerRowId = null;
      return;
    }

    const startLine = Math.min(selectionAnchor.line, line);
    const endLine = Math.max(selectionAnchor.line, line);
    selectionRange = {
      filePath: row.file.path,
      side,
      startLine,
      endLine,
      seedLines: collectSelectedLines(row.file.path, side, startLine, endLine)
    };
    commentButtonRowId = row.id;
    inlineComposerRowId = null;
  }

  function inlinePayloadBase() {
    if (!selectionRange) {
      return {};
    }
    return {
      pr_id: prId,
      pull_request_id: pullRequestNodeId || prId,
      path: selectionRange.filePath,
      side: selectionRange.side,
      line: selectionRange.endLine,
      start_line:
        selectionRange.startLine === selectionRange.endLine ? undefined : selectionRange.startLine,
      start_side:
        selectionRange.startLine === selectionRange.endLine ? undefined : selectionRange.side,
      subject_type: 'LINE',
      target_id: `${selectionRange.filePath}:${selectionRange.side}:${selectionRange.startLine}-${selectionRange.endLine}`
    };
  }

  async function preloadImageAssets(metadataFiles: PrFile[]): Promise<void> {
    for (const file of metadataFiles) {
      if (file.kind !== 'image') {
        continue;
      }
      await ensureImageAsset(file.path, 'right');
      const previousPath = file.previous_path ?? file.old_path;
      if (previousPath) {
        await ensureImageAsset(previousPath, 'left');
      }
    }
  }

  async function ensureImageAsset(path: string, side: 'left' | 'right'): Promise<void> {
    const loadKey = `${path}:${side}`;
    if (assetLoadsInFlight.has(loadKey)) {
      return;
    }
    const existing = imageAssets.get(path);
    if ((side === 'left' && existing?.left) || (side === 'right' && existing?.right)) {
      return;
    }
    assetLoadsInFlight.add(loadKey);
    try {
      const blob = await getPrFileBlob(accountId, prId, headSha, path, side.toUpperCase());
      const next = imageAssets.get(path) ?? {};
      if (blob.local_path) {
        const assetUrl = blob.local_path.startsWith('data:')
          ? blob.local_path
          : convertFileSrc(blob.local_path);
        if (side === 'left') {
          next.left = assetUrl;
        } else {
          next.right = assetUrl;
        }
      }
      imageAssets.set(path, next);
    } catch {
      // Keep renderer resilient when file blob lookup is unavailable.
    } finally {
      assetLoadsInFlight.delete(loadKey);
    }
  }

  function imagePairFor(path: string, previousPath: string | null): { left: string | null; right: string | null } {
    const right = imageAssets.get(path)?.right ?? null;
    const left = previousPath ? imageAssets.get(previousPath)?.left ?? null : null;
    return { left, right };
  }

  function openBinaryLink(path: string): string {
    const encodedPath = encodeURIComponent(path);
    return `https://github.com/search?q=${encodedPath}&type=code`;
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
            {@const metadata = metadataForPath(row.file.path)}
            {@const previousPath = metadata?.previous_path ?? metadata?.old_path ?? null}
            <div class="diff-file-row">
              <span class="text-mono f6">
                {#if metadata?.status === 'renamed' && previousPath}
                  {previousPath}
                  <span class="color-fg-muted octicon octicon-arrow-right" aria-hidden="true"></span>
                  {row.file.path}
                {:else}
                  {row.file.path}
                {/if}
              </span>
              {#if metadata?.rename_similarity !== null && metadata?.rename_similarity !== undefined}
                <span class="Label Label--secondary ml-2">{metadata.rename_similarity}%</span>
              {/if}
              {#if fileLineCount(row.file) > 5000 && !filesWithManualHighlight.has(row.file.path)}
                <button class="btn btn-sm ml-2" type="button" on:click={() => enableHighlight(row.file.path)}>
                  Highlight anyway
                </button>
              {/if}
            </div>
            {#if metadata?.kind === 'binary'}
              <div class="Box mt-1">
                <div class="Box-body d-flex flex-items-center flex-justify-between">
                  <div class="d-flex flex-items-center gap-2">
                    <span class="octicon octicon-file-binary" aria-hidden="true"></span>
                    <div>
                      <p class="mb-1 text-bold">Binary file</p>
                      <p class="mb-0 text-mono f6 color-fg-muted">{metadata.path}</p>
                    </div>
                  </div>
                  <a class="btn btn-sm" href={openBinaryLink(metadata.path)} target="_blank" rel="noreferrer">Open on GitHub</a>
                </div>
              </div>
            {:else if metadata?.kind === 'image'}
              {@const pair = imagePairFor(metadata.path, previousPath)}
              <div class="Box mt-1">
                <div class="Box-body d-flex gap-2 image-diff-grid">
                  {#if pair.left}
                    <div class="image-pane">
                      <p class="f6 color-fg-muted mb-1">Before</p>
                      <img src={pair.left} alt={`Previous ${metadata.path}`} loading="lazy" />
                    </div>
                  {/if}
                  {#if pair.right}
                    <div class="image-pane">
                      <p class="f6 color-fg-muted mb-1">After</p>
                      <img src={pair.right} alt={`Current ${metadata.path}`} loading="lazy" />
                    </div>
                  {/if}
                  {#if !pair.left && !pair.right}
                    <p class="f6 color-fg-muted mb-0">Loading image preview…</p>
                  {/if}
                </div>
              </div>
            {/if}
          {:else if row.kind === 'hunk'}
            <div class="diff-hunk-row text-mono f6 color-fg-muted">{row.hunk.header}</div>
          {:else}
            <div class={`diff-line-row ${mode}`}>
              {#if mode === 'unified'}
                <button class="diff-line-number" type="button" on:click={(event) => onLineNumberClick(row, 'LEFT', event)}>
                  {row.line.leftLine ?? ''}
                </button>
                <button class="diff-line-number" type="button" on:click={(event) => onLineNumberClick(row, 'RIGHT', event)}>
                  {row.line.rightLine ?? ''}
                </button>
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
                  <button class="diff-line-number" type="button" on:click={(event) => onLineNumberClick(row, 'LEFT', event)}>
                    {row.line.leftLine ?? ''}
                  </button>
                  <div class={`diff-line-content ${sideForLine(row.line) === 'LEFT' || sideForLine(row.line) === 'BOTH' ? row.line.kind : ''}`}>
                    {#if row.line.kind !== 'add'}
                      {#each renderHighlightedText(row.file.path, row.line.leftLine, row.line.text) as segment}
                        <span class={segment.className}>{segment.text}</span>
                      {/each}
                    {/if}
                  </div>
                </div>
                <div class="diff-side-cell">
                  <button class="diff-line-number" type="button" on:click={(event) => onLineNumberClick(row, 'RIGHT', event)}>
                    {row.line.rightLine ?? ''}
                  </button>
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
            {#if selectionRange && commentButtonRowId === row.id}
              <div class="diff-selection-actions">
                <button
                  class="btn btn-sm diff-comment-affordance"
                  type="button"
                  aria-label="Add inline review comment"
                  on:click={() => (inlineComposerRowId = row.id)}
                >
                  <span class="octicon octicon-comment-discussion" aria-hidden="true"></span>
                  <span>
                    {selectionRange.startLine === selectionRange.endLine
                      ? `Comment on line ${selectionRange.endLine}`
                      : `Comment on lines ${selectionRange.startLine}-${selectionRange.endLine}`}
                  </span>
                </button>
              </div>
            {/if}
            {#if selectionRange && inlineComposerRowId === row.id}
              <div class="mt-2">
                <Composer
                  accountId={accountId}
                  targetType="review_comment"
                  targetId={`${selectionRange.filePath}:${selectionRange.side}:${selectionRange.startLine}-${selectionRange.endLine}`}
                  submitKind="add_review_comment"
                  payloadBase={inlinePayloadBase()}
                  suggestionSeedLines={selectionRange.seedLines}
                  placeholder="Leave a review comment"
                  submitLabel="Add review comment"
                  on:submitted={() => {
                    inlineComposerRowId = null;
                    dispatch('reviewcommentsubmitted');
                  }}
                />
              </div>
            {/if}
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

<style>
  .diff-selection-actions {
    margin: 4px 0 8px 40px;
  }

  .diff-comment-affordance {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .image-diff-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  }

  .image-pane img {
    display: block;
    max-width: 100%;
    border: 1px solid var(--borderColor-default, #d0d7de);
    border-radius: 6px;
  }
</style>

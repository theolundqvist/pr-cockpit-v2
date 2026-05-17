<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import questionIcon from '@primer/octicons/build/svg/question-16.svg?raw';

  import { setWorktreeManualOverride } from '$lib/ipc/client';
  import type { WorktreeView } from '$lib/ipc/bindings';

  export let worktree: WorktreeView | null = null;
  export let repoPrOptions: Array<{ id: string; number: number; title: string }> = [];

  const dispatch = createEventDispatcher<{ changed: void }>();
  let search = '';
  let selected = '';

  $: selected = worktree?.manual_override_pr_id ?? worktree?.mapped_pr_id ?? '';
  $: filteredOptions = repoPrOptions.filter((candidate) => {
    const query = search.trim().toLowerCase();
    if (!query) {
      return true;
    }
    return (
      candidate.title.toLowerCase().includes(query) ||
      String(candidate.number).includes(query)
    );
  });
  $: signalRows = parseSignals(worktree?.mapping_source);

  async function updateOverride(value: string): Promise<void> {
    if (!worktree) {
      return;
    }
    await setWorktreeManualOverride(worktree.id, value || null);
    dispatch('changed');
  }

  function parseSignals(raw: string | null | undefined): Array<{
    signal: string;
    weight: number;
    confidence: number;
    contribution: number;
  }> {
    if (!raw) {
      return [];
    }
    try {
      const parsed = JSON.parse(raw) as {
        signals?: Array<{
          signal?: string;
          weight?: number;
          confidence?: number;
          contribution?: number;
        }>;
      };
      return (parsed.signals ?? []).map((entry) => ({
        signal: entry.signal ?? 'unknown',
        weight: entry.weight ?? 0,
        confidence: entry.confidence ?? 0,
        contribution: entry.contribution ?? 0
      }));
    } catch {
      return [];
    }
  }
</script>

{#if worktree}
  <div class="d-flex flex-column gap-1">
    <div class="d-flex flex-items-center gap-1">
      <span class="Label Label--secondary">
        {worktree.mapped_pr_number ? `PR #${worktree.mapped_pr_number}` : 'Unmapped'} · confidence{' '}
        {(worktree.mapping_confidence ?? 0).toFixed(2)}
      </span>
      <span class="color-fg-muted d-inline-flex" aria-hidden="true" title={signalRows
          .map(
            (signal) =>
              `${signal.signal}: w=${signal.weight.toFixed(2)}, c=${signal.confidence.toFixed(2)}, contribution=${signal.contribution.toFixed(2)}`
          )
          .join('\n')}>
        {@html questionIcon}
      </span>
    </div>
    <label class="d-flex flex-column gap-1">
      <span class="f6 color-fg-muted">Manual override</span>
      <input class="form-control" placeholder="Search PRs in repo" bind:value={search} />
      <select class="form-select" value={selected} on:change={(event) => updateOverride((event.currentTarget as HTMLSelectElement).value)}>
        <option value="">Unmap</option>
        {#each filteredOptions as candidate}
          <option value={candidate.id}>#{candidate.number} · {candidate.title}</option>
        {/each}
      </select>
    </label>
  </div>
{/if}

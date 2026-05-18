<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';

  import NoOptimismButton from '$lib/components/merge/NoOptimismButton.svelte';
  import { listenEventPayload } from '$lib/ipc/client';
  import type {
    MutationKind,
    SubmittedMutation,
    SuggestionBlock,
    WorktreeView
  } from '$lib/ipc/bindings';

  export let open = false;
  export let prId: string;
  export let expectedHeadSha: string;
  export let suggestions: SuggestionBlock[] = [];
  export let worktree: WorktreeView | null = null;
  export let submit: (
    kind: MutationKind,
    payload: Record<string, unknown>
  ) => Promise<SubmittedMutation | void>;

  const dispatch = createEventDispatcher<{ close: void; submitted: void }>();

  const steps = ['opened', 'assertions_ok', 'patched', 'committed', 'pushed'] as const;
  let selected = new Set<string>();
  let forceWithStash = false;
  let stepState = new Set<string>();
  let unsubscribers: Array<() => void> = [];

  $: if (!open) {
    stepState = new Set<string>();
  }

  $: if (open && selected.size === 0 && suggestions.length > 0) {
    selected = new Set(suggestions.map((suggestion) => suggestion.id));
  }

  $: canSubmit =
    open &&
    selected.size > 0 &&
    Boolean(worktree?.path) &&
    (!worktree?.dirty || forceWithStash);

  $: payload = {
    pr_id: prId,
    suggestion_ids: [...selected],
    expected_head_sha: expectedHeadSha,
    worktree_path: worktree?.path ?? '',
    force_with_stash: forceWithStash,
    target_id: prId
  };

  $: if (open) {
    void wireProgressListeners();
  } else {
    cleanupListeners();
  }

  onDestroy(() => {
    cleanupListeners();
  });

  function cleanupListeners(): void {
    for (const stop of unsubscribers) {
      stop();
    }
    unsubscribers = [];
  }

  async function wireProgressListeners(): Promise<void> {
    if (unsubscribers.length > 0) {
      return;
    }
    const nextUnsubscribers = await Promise.all(
      steps.map((step) =>
        listenEventPayload<{ pr_id: string; step: string }>(`worktree_write:${prId}:${step}`, () => {
          stepState = new Set([...stepState, step]);
        })
      )
    );
    unsubscribers = nextUnsubscribers;
  }

  function toggleSuggestion(suggestionId: string, checked: boolean): void {
    const next = new Set(selected);
    if (checked) {
      next.add(suggestionId);
    } else {
      next.delete(suggestionId);
    }
    selected = next;
  }
</script>

{#if open}
  <div class="conflict-modal-backdrop" role="presentation">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Apply suggestions">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Apply suggestions</h2>
        <button class="btn btn-sm" type="button" on:click={() => dispatch('close')}>Close</button>
      </div>
      <div class="Box-body">
        <p class="f6 color-fg-muted mb-2">
          Selected {selected.size} of {suggestions.length} suggestions
        </p>
        <ul class="list-style-none m-0 mb-3">
          {#each suggestions as suggestion (suggestion.id)}
            <li class="py-1 border-bottom color-border-muted">
              <label class="d-flex flex-items-start gap-2">
                <input
                  type="checkbox"
                  checked={selected.has(suggestion.id)}
                  on:change={(event) =>
                    toggleSuggestion(suggestion.id, (event.currentTarget as HTMLInputElement).checked)}
                />
                <span class="flex-auto">
                  <span class="text-mono f6">{suggestion.path}:{suggestion.start_line}-{suggestion.end_line}</span>
                  <span class="color-fg-muted"> · @{suggestion.suggestion_author_login}</span>
                  <pre class="mt-1 mb-0 f6">{suggestion.body}</pre>
                </span>
              </label>
            </li>
          {/each}
        </ul>

        <div class="Box mb-2">
          <div class="Box-header">Worktree</div>
          <div class="Box-body">
            {#if worktree}
              <p class="f6 mb-1 text-mono">{worktree.path}</p>
              <p class="f6 color-fg-muted mb-1">
                confidence {(worktree.mapping_confidence ?? 0).toFixed(2)} · {worktree.branch}
              </p>
              {#if worktree.dirty}
                <div class="flash flash-warn">
                  Worktree is dirty — commit or stash your changes first, or enable Force with stash.
                </div>
              {/if}
            {:else}
              <div class="flash flash-warn">
                No mapped worktree found for this PR. Map one from settings first.
              </div>
            {/if}
            <label class="d-inline-flex flex-items-center gap-1 mt-2 f6">
              <input type="checkbox" bind:checked={forceWithStash} />
              Force with stash (advanced)
            </label>
          </div>
        </div>

        <div class="mb-2">
          <div class="f6 text-bold mb-1">Progress</div>
          <div class="d-flex flex-wrap gap-1">
            {#each steps as step}
              <span class={`Label ${stepState.has(step) ? 'Label--success' : ''}`}>{step}</span>
            {/each}
          </div>
        </div>

        <div class="d-flex flex-items-center gap-2">
          <NoOptimismButton
            kind="apply_suggestion_batch"
            {payload}
            label="Apply selected suggestions"
            pendingLabel="Applying suggestions…"
            className="btn btn-primary"
            confirmTitle="Confirm batch suggestion apply"
            confirmMessage="This writes to your mapped worktree, commits, and pushes to the PR branch."
            disabled={!canSubmit}
            disabledReason="Select suggestions and provide a clean mapped worktree (or force with stash)."
            {submit}
            onComplete={() => {
              dispatch('submitted');
            }}
          />
          <button class="btn" type="button" on:click={() => dispatch('close')}>Cancel</button>
        </div>
      </div>
    </div>
  </div>
{/if}

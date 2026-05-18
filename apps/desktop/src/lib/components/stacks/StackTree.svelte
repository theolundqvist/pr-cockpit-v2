<script lang="ts">
  import { onDestroy } from 'svelte';

  import type { MergeMethod, StackGraph, StackNode, StackOperationView } from '$lib/ipc/bindings';
  import {
    abortStackOp,
    getStackOp,
    listenEventPayload,
    openExternalUrl,
    resumeStackOp,
    startMergeStack,
    startRebaseStack
  } from '$lib/ipc/client';
  import StackDagWarning from '$lib/components/stacks/StackDagWarning.svelte';

  export let stacks: StackGraph[] = [];
  export let graphiteEnabled = false;

  let expandedStackIds = new Set<string>();
  let actionsStackId: string | null = null;
  let mergeConfirmStack: StackGraph | null = null;
  let mergeMethodByStack = new Map<string, MergeMethod>();
  let activeOpId: string | null = null;
  let activeOperation: StackOperationView | null = null;
  let unlistenStackOp: (() => void) | null = null;

  $: sortedStacks = [...stacks].sort((left, right) => {
    const leftKind = left.kind === 'linear' ? 0 : 1;
    const rightKind = right.kind === 'linear' ? 0 : 1;
    if (leftKind !== rightKind) {
      return leftKind - rightKind;
    }
    return left.stack_id.localeCompare(right.stack_id);
  });

  $: if (!activeOpId && activeOperation?.status === 'succeeded') {
    activeOperation = null;
  }

  function rootNode(stack: StackGraph): StackNode | null {
    return (
      stack.nodes.find((node) => node.position === 0) ??
      [...stack.nodes].sort((left, right) => left.position - right.position)[0] ??
      null
    );
  }

  function stackRows(stack: StackGraph): StackNode[] {
    return [...stack.nodes].sort(
      (left, right) =>
        left.position - right.position ||
        left.pr_number - right.pr_number ||
        left.pr_id.localeCompare(right.pr_id)
    );
  }

  function toggleExpanded(stackId: string): void {
    const next = new Set(expandedStackIds);
    if (next.has(stackId)) {
      next.delete(stackId);
    } else {
      next.add(stackId);
    }
    expandedStackIds = next;
  }

  function toggleActions(stackId: string): void {
    actionsStackId = actionsStackId === stackId ? null : stackId;
  }

  function reviewGlyph(reviewDecision: string | null): string {
    const normalized = (reviewDecision ?? 'REVIEW_REQUIRED').toUpperCase();
    if (normalized === 'APPROVED') {
      return '✓';
    }
    if (normalized === 'CHANGES_REQUESTED') {
      return '✕';
    }
    if (normalized === 'REVIEWED') {
      return '?';
    }
    return '⏱';
  }

  function ciLabelClass(value: string | null): string {
    const normalized = (value ?? '').toUpperCase();
    if (normalized === 'SUCCESS') {
      return 'Label Label--success';
    }
    if (normalized === 'FAILURE' || normalized === 'ERROR') {
      return 'Label Label--danger';
    }
    return 'Label Label--attention';
  }

  function mergeMethodFor(stackId: string): MergeMethod {
    return mergeMethodByStack.get(stackId) ?? 'squash';
  }

  function setMergeMethod(stackId: string, method: MergeMethod): void {
    const next = new Map(mergeMethodByStack);
    next.set(stackId, method);
    mergeMethodByStack = next;
  }

  async function bindOperation(opId: string): Promise<void> {
    activeOpId = opId;
    activeOperation = await getStackOp(opId);
    unlistenStackOp?.();
    unlistenStackOp = await listenEventPayload<{ operation: StackOperationView }>(
      `stack_op:${opId}`,
      (payload) => {
        activeOperation = payload.operation;
        if (payload.operation.status === 'succeeded' || payload.operation.status === 'aborted') {
          activeOpId = null;
        }
      }
    );
  }

  async function handleRebase(stack: StackGraph): Promise<void> {
    const opId = await startRebaseStack(stack.account_id, stack.stack_id);
    await bindOperation(opId);
  }

  async function confirmMergeStack(): Promise<void> {
    if (!mergeConfirmStack) {
      return;
    }
    const opId = await startMergeStack(
      mergeConfirmStack.account_id,
      mergeConfirmStack.stack_id,
      mergeMethodFor(mergeConfirmStack.stack_id)
    );
    mergeConfirmStack = null;
    await bindOperation(opId);
  }

  async function retryActiveOperation(): Promise<void> {
    if (!activeOperation) {
      return;
    }
    await resumeStackOp(activeOperation.id);
    const refreshed = await getStackOp(activeOperation.id);
    if (refreshed) {
      activeOperation = refreshed;
    }
  }

  async function abortActiveOperation(): Promise<void> {
    if (!activeOperation) {
      return;
    }
    await abortStackOp(activeOperation.id);
    const refreshed = await getStackOp(activeOperation.id);
    if (refreshed) {
      activeOperation = refreshed;
    }
  }

  async function openWorktreePath(path: string | null): Promise<void> {
    if (!path) {
      return;
    }
    await openExternalUrl(`file://${path}`);
  }

  onDestroy(() => {
    unlistenStackOp?.();
    unlistenStackOp = null;
  });
</script>

<section class="mt-3" data-testid="stack-tree">
  <h2 class="f6 text-bold mb-2">Stacks</h2>
  {#if sortedStacks.length === 0}
    <p class="f6 color-fg-muted mb-0">No stacked PRs detected.</p>
  {:else}
    {#each sortedStacks as stack (stack.stack_id)}
      <section class="Box mb-2">
        <button
          class="Box-header stack-summary-button"
          type="button"
          on:click={() => toggleExpanded(stack.stack_id)}
          data-testid={`stack-summary-${stack.stack_id}`}
        >
          <span class="text-semibold">{rootNode(stack)?.title ?? stack.stack_id}</span>
          <span class="f6 color-fg-muted">
            {stack.repo_id} · <span class="Label">{stack.nodes.length} PRs</span>
          </span>
        </button>
        {#if expandedStackIds.has(stack.stack_id)}
          <div class="Box-body">
            {#if stack.kind === 'dag'}
              <StackDagWarning warning={stack.warning} edges={stack.edges} />
            {/if}
            <ul class="list-style-none m-0 p-0">
              {#each stackRows(stack) as node (node.pr_id)}
                <li class="stack-row mb-2" data-testid="stack-row">
                  <div class="d-flex flex-column gap-1">
                    <a href={`/pr/${node.pr_id}`} class="Link--primary">
                      <span class="text-mono mr-1">#{node.pr_number}</span>{node.title}
                    </a>
                    <div class="d-flex flex-wrap flex-items-center gap-1">
                      <span class="Label Label--secondary" title={node.review_decision ?? 'REVIEW_REQUIRED'}>
                        {reviewGlyph(node.review_decision)}
                      </span>
                      <span class={ciLabelClass(node.check_rollup_state)}>
                        {node.check_rollup_state ?? 'UNKNOWN'}
                      </span>
                      {#if (node.merge_state_status ?? '').toUpperCase() === 'DIRTY'}
                        <span class="Label Label--danger">conflict</span>
                      {/if}
                      {#each node.blocked_by as blocked}
                        <span class="Label Label--attention" title={blocked.detail}>
                          blocked: {blocked.kind}
                        </span>
                      {/each}
                    </div>
                    <code class="f6 text-mono color-fg-muted" title={`${node.base_sha} → ${node.head_sha}`}>
                      {node.base_sha} → {node.head_sha}
                    </code>
                    {#if node.position === 0}
                      <div>
                        <button
                          class="btn btn-sm"
                          type="button"
                          on:click={() => toggleActions(stack.stack_id)}
                          data-testid={`stack-actions-toggle-${stack.stack_id}`}
                        >
                          Stack actions
                        </button>
                      </div>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>

            {#if actionsStackId === stack.stack_id}
              <div class="border-top color-border-muted pt-2 mt-2" data-testid="stack-actions-panel">
                <div class="d-flex flex-wrap flex-items-center gap-2 mb-2">
                  <button
                    class="btn btn-sm"
                    type="button"
                    on:click={() => handleRebase(stack)}
                    data-testid={`rebase-stack-button-${stack.stack_id}`}
                  >
                    Rebase stack
                  </button>
                  <button
                    class="btn btn-sm btn-primary"
                    type="button"
                    on:click={() => (mergeConfirmStack = stack)}
                    data-testid={`merge-stack-button-${stack.stack_id}`}
                  >
                    Merge stack
                  </button>
                  {#if graphiteEnabled}
                    <span class="Label Label--accent-emphasis" data-testid="graphite-enabled-badge">gt</span>
                  {/if}
                </div>
                <label class="f6 d-flex flex-column gap-1">
                  Merge method
                  <select
                    class="form-select"
                    value={mergeMethodFor(stack.stack_id)}
                    on:change={(event) =>
                      setMergeMethod(stack.stack_id, (event.currentTarget as HTMLSelectElement).value as MergeMethod)}
                  >
                    <option value="merge">merge</option>
                    <option value="squash">squash</option>
                    <option value="rebase">rebase</option>
                  </select>
                </label>
              </div>
            {/if}
          </div>
        {/if}
      </section>
    {/each}
  {/if}
</section>

{#if mergeConfirmStack}
  <div class="conflict-modal-backdrop">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Confirm merge stack">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Confirm merge stack</h2>
        <button class="btn btn-sm" type="button" on:click={() => (mergeConfirmStack = null)}>Close</button>
      </div>
      <div class="Box-body">
        <p class="f6 mb-2">
          Merge stack <code>{mergeConfirmStack.stack_id}</code> using
          <code>{mergeMethodFor(mergeConfirmStack.stack_id)}</code>?
        </p>
        <div class="d-flex gap-1">
          <button class="btn btn-primary" type="button" on:click={confirmMergeStack}>Confirm</button>
          <button class="btn" type="button" on:click={() => (mergeConfirmStack = null)}>Cancel</button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if activeOperation}
  <div class="conflict-modal-backdrop" data-testid="stack-op-modal">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Stack operation status">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">
          {activeOperation.op_kind === 'rebase' ? 'Rebasing stack' : 'Merging stack'} ·
          {activeOperation.status}
        </h2>
        <button class="btn btn-sm" type="button" on:click={() => (activeOperation = null)}>Close</button>
      </div>
      <div class="Box-body">
        {#if activeOperation.status === 'running' || activeOperation.status === 'pending'}
          <p class="f6">
            Step {activeOperation.current_step ?? 0}/{activeOperation.total_steps ?? 0}:
            {activeOperation.current_pr_id ?? 'preparing'}
          </p>
        {:else if activeOperation.status === 'paused_conflict'}
          <section data-testid="stack-op-conflict">
            <p class="flash flash-warn">Conflict detected.</p>
            <p class="f6 mb-1 text-mono">Worktree: {activeOperation.worktree_path}</p>
            <ul class="list-style-none pl-0">
              {#each activeOperation.conflict_files as file}
                <li class="text-mono f6">{file}</li>
              {/each}
            </ul>
            <div class="d-flex gap-1">
              <button
                class="btn btn-sm"
                type="button"
                on:click={() => openWorktreePath(activeOperation?.worktree_path ?? null)}
              >
                Open worktree in editor
              </button>
              <button class="btn btn-sm btn-primary" type="button" on:click={retryActiveOperation}>
                Resume
              </button>
              <button class="btn btn-sm btn-danger" type="button" on:click={abortActiveOperation}>
                Abort
              </button>
            </div>
          </section>
        {:else if activeOperation.status === 'paused_failure'}
          <section data-testid="stack-op-failure">
            <p class="flash flash-error">{activeOperation.last_error ?? 'Operation failed'}</p>
            <div class="d-flex gap-1">
              <button class="btn btn-sm btn-primary" type="button" on:click={retryActiveOperation}>
                Retry
              </button>
              <button class="btn btn-sm btn-danger" type="button" on:click={abortActiveOperation}>
                Abort
              </button>
            </div>
          </section>
        {:else}
          <p class="flash flash-success">Operation {activeOperation.status}.</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .stack-summary-button {
    border: 0;
    width: 100%;
    text-align: left;
    background: transparent;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .stack-row {
    border-left: 2px solid var(--borderColor-muted, #d0d7de);
    padding-left: 8px;
  }
</style>

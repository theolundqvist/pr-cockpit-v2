<script lang="ts">
  import InlineMutationErrorBanner from '$lib/components/InlineMutationErrorBanner.svelte';
  import { discardMutation, listenEventPayload, retryMutation } from '$lib/ipc/client';
  import type {
    MutationFailedEventPayload,
    MutationKind,
    PendingMutationView,
    PrDetailSummary,
    SubmittedMutation
  } from '$lib/ipc/bindings';

  import MergeableBackoffMeter from './MergeableBackoffMeter.svelte';
  import NoOptimismButton from './NoOptimismButton.svelte';

  type BranchProtectionSummary = {
    requires_approving_reviews: boolean;
    required_approving_review_count: number;
    requires_status_checks: boolean;
    required_status_check_contexts: string[];
    requires_strict_status_checks: boolean;
    restricts_pushes: boolean;
    restricts_review_dismissals: boolean;
  };

  type RepoSettings = {
    owner: string;
    name: string;
    merge_commit_allowed: boolean;
    squash_merge_allowed: boolean;
    rebase_merge_allowed: boolean;
    delete_branch_on_merge_default: boolean;
    repo_has_merge_queue: boolean;
  };

  type ViewerPermissions = {
    viewer_can_merge: boolean;
    viewer_can_enable_auto_merge: boolean;
    viewer_can_disable_auto_merge: boolean;
    viewer_can_update_branch: boolean;
    viewer_can_delete_head_ref: boolean;
  };

  export let summary: PrDetailSummary;
  export let repo: RepoSettings;
  export let viewer: ViewerPermissions;
  export let accountId: string;
  export let offline = false;
  export let onSubmit: (
    kind: MutationKind,
    payload: Record<string, unknown>
  ) => Promise<SubmittedMutation | void>;

  let initializedPrId = '';
  let selectedMergeMethod = 'merge';
  let deleteBranchAfterMerge = false;
  let queueEntryId: string | null = null;
  let queuePosition: number | null = null;
  let queueState: string | null = null;
  let queueEstimatedMs: number | null = null;
  let localAutoMergeEnabled = false;
  let localAutoMergeMethod: string | null = null;
  let localAutoMergeEnabledBy: string | null = null;
  let localMergeStateStatus = '';
  let showProtectionDetails = false;
  let showEnableAutoMergeModal = false;
  let enableAutoMergeMethod = 'SQUASH';
  let enableAutoMergeHeadline = '';
  let enableAutoMergeBody = '';
  let enableAutoMergeSubmitting = false;
  let enableAutoMergeError: string | null = null;
  let updateBranchPending = false;
  let updateBranchError: string | null = null;
  let deleteChainPending = false;
  let deleteChainFailure: PendingMutationView | null = null;

  const methodOrder: Array<{
    mergeMethod: 'merge' | 'squash' | 'rebase';
    autoMergeMethod: 'MERGE' | 'SQUASH' | 'REBASE';
    label: string;
    enabled: (repo: RepoSettings) => boolean;
  }> = [
    {
      mergeMethod: 'merge',
      autoMergeMethod: 'MERGE',
      label: 'Create a merge commit',
      enabled: (value) => value.merge_commit_allowed
    },
    {
      mergeMethod: 'squash',
      autoMergeMethod: 'SQUASH',
      label: 'Squash and merge',
      enabled: (value) => value.squash_merge_allowed
    },
    {
      mergeMethod: 'rebase',
      autoMergeMethod: 'REBASE',
      label: 'Rebase and merge',
      enabled: (value) => value.rebase_merge_allowed
    }
  ];

  $: allowedMethods = methodOrder.filter((option) => option.enabled(repo));
  $: if (!allowedMethods.some((option) => option.mergeMethod === selectedMergeMethod)) {
    selectedMergeMethod = allowedMethods[0]?.mergeMethod ?? 'merge';
  }
  $: if (!methodOrder.some((option) => option.autoMergeMethod === enableAutoMergeMethod && option.enabled(repo))) {
    enableAutoMergeMethod = allowedMethods[0]?.autoMergeMethod ?? 'SQUASH';
  }

  $: if (summary.pr_id !== initializedPrId) {
    initializedPrId = summary.pr_id;
    deleteBranchAfterMerge = summary.delete_branch_on_merge_default ?? false;
    queueEntryId = summary.merge_queue_entry_id;
    queuePosition = summary.merge_queue_entry_position;
    queueState = summary.merge_queue_entry_state;
    queueEstimatedMs = summary.merge_queue_entry_estimated_ms;
    localAutoMergeEnabled = summary.auto_merge_enabled ?? false;
    localAutoMergeMethod = summary.auto_merge_method;
    localAutoMergeEnabledBy = summary.auto_merge_enabled_by_login;
    localMergeStateStatus = summary.merge_state_status ?? '';
  }

  $: protectionSummary = parseBranchProtectionSummary(summary.branch_protection_summary_json);
  $: protectionReasons = branchProtectionReasons(protectionSummary);
  $: queueEstimateMinutes =
    queueEstimatedMs && queueEstimatedMs > 0 ? Math.max(1, Math.ceil(queueEstimatedMs / 60_000)) : null;
  $: mergePayload = {
    pr_id: summary.pr_id,
    target_id: summary.pr_id,
    repo_id: summary.repo_id,
    owner: repo.owner,
    repo: repo.name,
    pr_number: summary.pr_number,
    title: summary.title,
    body: summary.body,
    base_ref: summary.base_ref,
    base_sha: summary.base_sha,
    head_ref: summary.head_ref,
    head_sha: summary.head_sha,
    merge_method: selectedMergeMethod,
    expected_head_sha: summary.head_sha
  };
  $: deleteHeadRefPayload = {
    pr_id: summary.pr_id,
    target_id: summary.pr_id,
    owner: repo.owner,
    repo: repo.name,
    head_ref: summary.head_ref
  };

  function branchProtectionReasons(summary: BranchProtectionSummary | null): string[] {
    if (!summary) {
      return [];
    }
    const reasons: string[] = [];
    if (summary.requires_approving_reviews && summary.required_approving_review_count > 0) {
      reasons.push(
        `requires ${summary.required_approving_review_count} approving review${
          summary.required_approving_review_count === 1 ? '' : 's'
        }`
      );
    }
    if (summary.requires_status_checks) {
      reasons.push('status checks required');
    }
    if (summary.requires_strict_status_checks) {
      reasons.push('strict status checks required');
    }
    if (summary.restricts_pushes) {
      reasons.push('pushes restricted');
    }
    if (summary.restricts_review_dismissals) {
      reasons.push('review dismissals restricted');
    }
    return reasons;
  }

  function parseBranchProtectionSummary(value: string | null): BranchProtectionSummary | null {
    if (!value) {
      return null;
    }
    try {
      return JSON.parse(value) as BranchProtectionSummary;
    } catch {
      return null;
    }
  }

  function failureKind(payload: MutationFailedEventPayload): string {
    if (payload.error_kind.kind === 'other') {
      return payload.error_kind.detail;
    }
    return payload.error_kind.kind;
  }

  function buildFailureMutation(
    submitted: SubmittedMutation,
    kind: MutationKind,
    targetId: string,
    payload: MutationFailedEventPayload
  ): PendingMutationView {
    const now = Math.floor(Date.now() / 1000);
    return {
      id: submitted.mutation_id,
      account_id: accountId,
      kind,
      optimism: 'none',
      target_type: 'pull_request',
      target_id: targetId,
      status: 'failed',
      retries: 0,
      created_at: now,
      updated_at: now,
      last_error: failureKind(payload),
      pending_overlay: null,
      requires_connection_confirmation: false
    };
  }

  async function waitForMutationOutcome(
    mutationId: string
  ): Promise<{ kind: 'reconciled' } | { kind: 'failed'; payload: MutationFailedEventPayload }> {
    return new Promise((resolve) => {
      let unlistenReconciled: (() => void) | null = null;
      let unlistenFailed: (() => void) | null = null;
      let done = false;

      const finish = (result: { kind: 'reconciled' } | { kind: 'failed'; payload: MutationFailedEventPayload }) => {
        if (done) {
          return;
        }
        done = true;
        if (unlistenReconciled) {
          unlistenReconciled();
        }
        if (unlistenFailed) {
          unlistenFailed();
        }
        resolve(result);
      };

      void Promise.all([
        listenEventPayload<{ mutation_id: string }>('mutation:reconciled', (payload) => {
          if (payload.mutation_id === mutationId) {
            finish({ kind: 'reconciled' });
          }
        }),
        listenEventPayload<MutationFailedEventPayload>('mutation:failed', (payload) => {
          if (payload.mutation_id === mutationId) {
            finish({ kind: 'failed', payload });
          }
        })
      ]).then(([stopReconciled, stopFailed]) => {
        unlistenReconciled = stopReconciled;
        unlistenFailed = stopFailed;
      });
    });
  }

  async function submitDeleteAfterMerge(): Promise<void> {
    if (
      !viewer.viewer_can_delete_head_ref ||
      summary.head_ref_state === 'DELETED' ||
      !repo.owner ||
      !repo.name
    ) {
      return;
    }
    deleteChainPending = true;
    deleteChainFailure = null;
    const submitted = await onSubmit('delete_head_ref', deleteHeadRefPayload);
    if (!submitted) {
      deleteChainPending = false;
      return;
    }
    const outcome = await waitForMutationOutcome(submitted.mutation_id);
    if (outcome.kind === 'reconciled') {
      deleteChainPending = false;
      return;
    }
    deleteChainPending = false;
    deleteChainFailure = buildFailureMutation(
      submitted,
      'delete_head_ref',
      summary.pr_id,
      outcome.payload
    );
  }

  async function retryDeleteFailure(mutationId: string): Promise<void> {
    await retryMutation(mutationId);
    deleteChainPending = true;
    deleteChainFailure = null;
    const outcome = await waitForMutationOutcome(mutationId);
    if (outcome.kind === 'failed') {
      deleteChainFailure = {
        ...(deleteChainFailure ?? {
          id: mutationId,
          account_id: accountId,
          kind: 'delete_head_ref',
          optimism: 'none',
          target_type: 'pull_request',
          target_id: summary.pr_id,
          status: 'failed',
          retries: 0,
          created_at: Math.floor(Date.now() / 1000),
          updated_at: Math.floor(Date.now() / 1000),
          last_error: null,
          pending_overlay: null,
          requires_connection_confirmation: false
        }),
        last_error: failureKind(outcome.payload)
      };
    }
    deleteChainPending = false;
  }

  async function discardDeleteFailure(mutationId: string): Promise<void> {
    await discardMutation(mutationId);
    deleteChainFailure = null;
  }

  async function handleEnableAutoMerge(): Promise<void> {
    enableAutoMergeSubmitting = true;
    enableAutoMergeError = null;
    const submitted = await onSubmit('enable_auto_merge', {
      pr_id: summary.pr_id,
      target_id: summary.pr_id,
      pull_request_id: summary.pr_id,
      merge_method: enableAutoMergeMethod,
      commit_headline: enableAutoMergeHeadline,
      commit_body: enableAutoMergeBody
    });
    if (!submitted) {
      enableAutoMergeSubmitting = false;
      return;
    }
    const outcome = await waitForMutationOutcome(submitted.mutation_id);
    if (outcome.kind === 'failed') {
      enableAutoMergeError = failureKind(outcome.payload);
      enableAutoMergeSubmitting = false;
      return;
    }
    localAutoMergeEnabled = true;
    localAutoMergeMethod = enableAutoMergeMethod;
    localAutoMergeEnabledBy = localAutoMergeEnabledBy ?? 'you';
    showEnableAutoMergeModal = false;
    enableAutoMergeSubmitting = false;
  }

  async function handleUpdateBranch(): Promise<void> {
    updateBranchPending = true;
    updateBranchError = null;
    localMergeStateStatus = 'UPDATING';
    const submitted = await onSubmit('update_branch', {
      pr_id: summary.pr_id,
      target_id: summary.pr_id,
      owner: repo.owner,
      repo: repo.name,
      pr_number: summary.pr_number,
      merge_state_status: summary.merge_state_status
    });
    if (!submitted) {
      updateBranchPending = false;
      return;
    }
    const outcome = await waitForMutationOutcome(submitted.mutation_id);
    if (outcome.kind === 'failed') {
      localMergeStateStatus = summary.merge_state_status ?? 'BEHIND';
      updateBranchError = failureKind(outcome.payload);
      updateBranchPending = false;
      return;
    }
    localMergeStateStatus = 'CLEAN';
    updateBranchPending = false;
  }
</script>

<div class="Box mt-2">
  <div class="Box-header d-flex flex-items-center flex-justify-between">
    <span>Merge box</span>
    <MergeableBackoffMeter accountId={accountId} prId={summary.pr_id} mergeableState={summary.mergeable_state} />
  </div>
  <div class="Box-body">
    {#if protectionReasons.length > 0}
      <div class="flash flash-warn mb-2">
        <div class="d-flex flex-items-center flex-justify-between gap-2">
          <span>Branch protection: {protectionReasons.join(' · ')}</span>
          <button class="btn btn-sm" type="button" on:click={() => (showProtectionDetails = !showProtectionDetails)}>
            {showProtectionDetails ? 'Hide details' : 'Show details'}
          </button>
        </div>
        {#if showProtectionDetails}
          <pre class="conflict-diff-block mt-2">{JSON.stringify(protectionSummary, null, 2)}</pre>
        {/if}
      </div>
    {/if}

    <fieldset class="mb-2">
      <legend class="f6 text-bold mb-1">Merge method</legend>
      {#each methodOrder as option}
        <label class="d-flex flex-items-center gap-1 mb-1">
          <input
            type="radio"
            name="merge-method"
            value={option.mergeMethod}
            bind:group={selectedMergeMethod}
            disabled={!option.enabled(repo)}
            title={!option.enabled(repo) ? 'Disabled by repository merge settings' : ''}
          />
          <span>{option.label}</span>
        </label>
      {/each}
    </fieldset>

    {#if localAutoMergeEnabled}
      <div class="flash flash-success mb-2 d-flex flex-items-center flex-justify-between">
        <span>
          Auto-merge enabled by @{localAutoMergeEnabledBy ?? 'unknown'} ({(localAutoMergeMethod ?? 'SQUASH').toLowerCase()})
        </span>
        <NoOptimismButton
          kind="disable_auto_merge"
          payload={{
            pr_id: summary.pr_id,
            target_id: summary.pr_id,
            pull_request_id: summary.pr_id
          }}
          label="Disable auto-merge"
          className="btn btn-sm"
          confirmTitle="Disable auto-merge"
          confirmMessage="Disable auto-merge for this pull request?"
          disabled={offline || !viewer.viewer_can_disable_auto_merge}
          disabledReason={offline ? 'requires connection' : 'insufficient permissions'}
          submit={onSubmit}
          onComplete={() => {
            localAutoMergeEnabled = false;
            localAutoMergeMethod = null;
            localAutoMergeEnabledBy = null;
          }}
        />
      </div>
    {:else}
      <button
        class="btn btn-sm mb-2"
        type="button"
        disabled={offline || !viewer.viewer_can_enable_auto_merge}
        title={offline ? 'requires connection' : ''}
        on:click={() => (showEnableAutoMergeModal = true)}
      >
        Enable auto-merge
      </button>
    {/if}

    {#if repo.repo_has_merge_queue}
      <div class="mb-2">
        {#if queueEntryId}
          <p class="f6 mb-1">
            In queue — position {queuePosition ?? '—'}
            {#if queueState}
              · {queueState.toLowerCase()}
            {/if}
            {#if queueEstimateMinutes !== null}
              · est {queueEstimateMinutes} min
            {/if}
          </p>
          <div class="d-flex flex-wrap gap-1">
            <NoOptimismButton
              kind="dequeue_merge_queue"
              payload={{
                pr_id: summary.pr_id,
                target_id: summary.pr_id,
                merge_queue_entry_id: queueEntryId
              }}
              label="Remove from queue"
              className="btn btn-sm"
              confirmTitle="Remove from merge queue"
              confirmMessage="Remove this pull request from the merge queue?"
              disabled={offline}
              disabledReason="requires connection"
              submit={onSubmit}
              onComplete={() => {
                queueEntryId = null;
                queuePosition = null;
                queueState = null;
                queueEstimatedMs = null;
              }}
            />
            <NoOptimismButton
              kind="reorder_merge_queue"
              payload={{
                pr_id: summary.pr_id,
                target_id: summary.pr_id,
                merge_queue_entry_id: queueEntryId,
                mode: 'TOP'
              }}
              label="Move to top"
              className="btn btn-sm"
              confirmTitle="Move queue entry"
              confirmMessage="Move this pull request to the top of the merge queue?"
              disabled={offline}
              disabledReason="requires connection"
              submit={onSubmit}
              onComplete={() => {
                queuePosition = 1;
                queueState = queueState ?? 'QUEUED';
              }}
            />
            <NoOptimismButton
              kind="reorder_merge_queue"
              payload={{
                pr_id: summary.pr_id,
                target_id: summary.pr_id,
                merge_queue_entry_id: queueEntryId,
                mode: 'BOTTOM'
              }}
              label="Move to bottom"
              className="btn btn-sm"
              confirmTitle="Move queue entry"
              confirmMessage="Move this pull request to the bottom of the merge queue?"
              disabled={offline}
              disabledReason="requires connection"
              submit={onSubmit}
              onComplete={() => {
                queuePosition = Math.max(queuePosition ?? 1, 5);
                queueState = queueState ?? 'QUEUED';
              }}
            />
          </div>
        {:else}
          <NoOptimismButton
            kind="enqueue_merge_queue"
            payload={{
              pr_id: summary.pr_id,
              target_id: summary.pr_id,
              pull_request_id: summary.pr_id,
              expected_head_oid: summary.head_sha
            }}
            label="Add to merge queue"
            className="btn btn-sm"
            confirmTitle="Add to merge queue"
            confirmMessage="Add this pull request to the merge queue?"
            disabled={offline}
            disabledReason="requires connection"
            submit={onSubmit}
            onComplete={() => {
              queueEntryId = `local-${summary.pr_id}`;
              queuePosition = queuePosition ?? 1;
              queueState = 'QUEUED';
            }}
          />
        {/if}
      </div>
    {/if}

    {#if viewer.viewer_can_update_branch && localMergeStateStatus.toUpperCase() === 'BEHIND'}
      <div class="mb-2">
        <button
          class="btn btn-sm btn-primary"
          type="button"
          on:click={handleUpdateBranch}
          disabled={offline || updateBranchPending || !repo.owner || !repo.name}
          title={offline ? 'requires connection' : ''}
        >
          {#if updateBranchPending}
            <span class="anim-rotate d-inline-block mr-1">⟳</span>Updating…
          {:else}
            Update branch
          {/if}
        </button>
      </div>
    {/if}

    {#if updateBranchError}
      <div class="flash flash-error mb-2">{updateBranchError}</div>
    {/if}

    <label class="d-flex flex-items-center gap-1 mb-2">
      <input type="checkbox" bind:checked={deleteBranchAfterMerge} />
      <span>Delete branch after merge</span>
    </label>

    <div class="d-flex flex-wrap gap-1">
      <NoOptimismButton
        kind="merge"
        payload={mergePayload}
        label={deleteBranchAfterMerge ? 'Merge and delete branch' : 'Merge'}
        className="btn btn-sm btn-primary"
        confirmTitle={deleteBranchAfterMerge ? 'Merge and delete branch' : 'Confirm merge'}
        confirmMessage={
          deleteBranchAfterMerge
            ? 'Merge and delete branch?'
            : 'Merging requires a live connection. Continue?'
        }
        disabled={offline || !viewer.viewer_can_merge || allowedMethods.length === 0}
        disabledReason={
          offline
            ? 'requires connection'
            : !repo.owner || !repo.name
              ? 'repository identity unavailable'
              : 'merge is blocked'
        }
        submit={onSubmit}
        onComplete={async () => {
          if (deleteBranchAfterMerge) {
            await submitDeleteAfterMerge();
          }
        }}
      />
      {#if viewer.viewer_can_update_branch && localMergeStateStatus.toUpperCase() !== 'BEHIND'}
        <button
          class="btn btn-sm"
          type="button"
          on:click={handleUpdateBranch}
          disabled={offline || updateBranchPending || !repo.owner || !repo.name}
          title={offline ? 'requires connection' : ''}
        >
          {#if updateBranchPending}
            <span class="anim-rotate d-inline-block mr-1">⟳</span>Updating…
          {:else}
            Update branch
          {/if}
        </button>
      {/if}
    </div>

    {#if deleteChainPending}
      <div class="mt-2 f6 color-fg-muted">
        <span class="anim-rotate d-inline-block mr-1">⟳</span>Deleting branch after merge…
      </div>
    {/if}
    {#if deleteChainFailure}
      <InlineMutationErrorBanner
        mutation={deleteChainFailure}
        onRetry={retryDeleteFailure}
        onDiscard={discardDeleteFailure}
      />
    {/if}
  </div>
</div>

{#if showEnableAutoMergeModal}
  <div class="conflict-modal-backdrop" role="presentation">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Enable auto-merge">
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">Enable auto-merge</h2>
        <button class="btn btn-sm" type="button" on:click={() => (showEnableAutoMergeModal = false)}>
          Close
        </button>
      </div>
      <div class="Box-body">
        <fieldset class="mb-2">
          <legend class="f6 text-bold mb-1">Merge method</legend>
          {#each methodOrder.filter((option) => option.enabled(repo)) as option}
            <label class="d-flex flex-items-center gap-1 mb-1">
              <input type="radio" name="auto-merge-method" value={option.autoMergeMethod} bind:group={enableAutoMergeMethod} />
              <span>{option.label}</span>
            </label>
          {/each}
        </fieldset>
        <label class="d-block mb-2">
          <span class="f6 d-block mb-1">Commit headline</span>
          <input class="form-control" bind:value={enableAutoMergeHeadline} placeholder="Auto-merge commit headline" />
        </label>
        <label class="d-block mb-2">
          <span class="f6 d-block mb-1">Commit body</span>
          <textarea class="form-control" rows={4} bind:value={enableAutoMergeBody}></textarea>
        </label>
        {#if enableAutoMergeError}
          <div class="flash flash-error mb-2">{enableAutoMergeError}</div>
        {/if}
        <div class="d-flex gap-1">
          <button
            class="btn btn-primary"
            type="button"
            disabled={enableAutoMergeSubmitting}
            on:click={handleEnableAutoMerge}
          >
            {#if enableAutoMergeSubmitting}
              <span class="anim-rotate d-inline-block mr-1">⟳</span>Enabling…
            {:else}
              Enable auto-merge
            {/if}
          </button>
          <button
            class="btn"
            type="button"
            disabled={enableAutoMergeSubmitting}
            on:click={() => (showEnableAutoMergeModal = false)}
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

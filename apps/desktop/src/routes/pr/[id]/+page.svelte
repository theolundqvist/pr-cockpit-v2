<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';

  import Composer from '$lib/components/Composer.svelte';
  import CheckLogTail from '$lib/components/checks/CheckLogTail.svelte';
  import ChecksRail from '$lib/components/checks/ChecksRail.svelte';
  import DiffViewer from '$lib/components/DiffViewer.svelte';
  import HardConflictModal from '$lib/components/HardConflictModal.svelte';
  import InlineMutationErrorBanner from '$lib/components/InlineMutationErrorBanner.svelte';
  import MergeBox from '$lib/components/merge/MergeBox.svelte';
  import NoOptimismButton from '$lib/components/merge/NoOptimismButton.svelte';
  import PendingAffordance from '$lib/components/PendingAffordance.svelte';
  import SuggestionBatchModal from '$lib/components/suggestions/SuggestionBatchModal.svelte';
  import ServerAdjustedChip from '$lib/components/ServerAdjustedChip.svelte';
  import SyncErrorsTray from '$lib/components/SyncErrorsTray.svelte';
  import WorktreeBadge from '$lib/components/worktree/WorktreeBadge.svelte';
  import WorktreeMappingChip from '$lib/components/worktree/WorktreeMappingChip.svelte';
  import {
    checksRollupBadge,
    getPrDetailBundle,
    invalidatePrDetail,
    type PrDetailBundle
  } from '$lib/data/pr-detail';
  import {
    discardMutation,
    listPrPushes,
    listPendingMutations,
    listenEventPayload,
    retryMutation,
    submitMutation
  } from '$lib/ipc/client';
  import type {
    HardConflictPayload,
    MutationFailedEventPayload,
    MutationKind,
    NetState,
    PendingOverlay,
    PendingMutationView,
    PrPushView,
    SuggestionBlock,
    SubmittedMutation
  } from '$lib/ipc/bindings';
  import { reduceConversationTimeline } from '$lib/timeline/reducer';
  import { formatRelative } from '$lib/utils/time';
  import { inboxStore, refreshAccountData, worktreesStore } from '$lib/state/cockpit';
  import type { PageData } from './$types';

  export let data: PageData;

  const connectionRequiredKinds: MutationKind[] = [
    'merge',
    'close_pr',
    'reopen_pr',
    'enable_auto_merge',
    'disable_auto_merge',
    'apply_suggestion',
    'apply_suggestion_batch'
  ];

  let bundle: PrDetailBundle = data.bundle;
  let activeTab: 'conversation' | 'files' | 'checks' = 'conversation';
  let refreshing = false;
  let unlisten: Array<() => void> = [];
  let pendingMutations: PendingMutationView[] = [];
  let pushHistory: PrPushView[] = [];
  let syncTrayOpen = false;
  let conflict: HardConflictPayload | null = null;
  let networkState: NetState = { state: 'online' };
  let titleDraft = bundle.summary.title;
  let bodyDraft = bundle.summary.body;
  let editingTitle = false;
  let editingBody = false;
  let addingLabel = '';
  let setAssigneesValue = bundle.metadata.assignees.map((assignee) => assignee.login ?? '').join(',');
  let requestReviewValue = '';
  let milestoneValue = bundle.metadata.milestones[0]?.title ?? '';
  let projectValue = bundle.metadata.projects[0]?.project_title ?? '';
  let showReviewModal = false;
  let showSuggestionBatchModal = false;
  let activeCheckLogRunId: string | null = null;
  let reviewBody = '';
  let confirmModal:
    | {
        kind: MutationKind;
        title: string;
        message: string;
        payload: Record<string, unknown>;
      }
    | null = null;

  $: checkBadge = checksRollupBadge(bundle.checks);
  $: conversation = reduceConversationTimeline(
    bundle.summary,
    bundle.timeline,
    bundle.review_threads,
    bundle.metadata
  );
  $: optimismByMutationId = new Map(pendingMutations.map((entry) => [entry.id, entry.optimism]));
  $: failedMutations = pendingMutations.filter(
    (entry) => entry.status === 'failed' && entry.last_error !== 'network'
  );
  $: failedByTarget = failedMutations.reduce(
    (acc, entry) => {
      if (!acc.has(entry.target_id)) {
        acc.set(entry.target_id, entry);
      }
      return acc;
    },
    new Map<string, PendingMutationView>()
  );
  $: queuedMutationCount = pendingMutations.filter((entry) => entry.status === 'pending').length;
  $: offline = networkState.state === 'offline';
  $: latestPushPair = (() => {
    if (pushHistory.length < 2) {
      return null;
    }
    const oldPush = pushHistory[pushHistory.length - 2];
    const newPush = pushHistory[pushHistory.length - 1];
    if (!oldPush || !newPush) {
      return null;
    }
    return { old: oldPush, new: newPush };
  })();
  $: forcePushCount = Math.max(pushHistory.length - 1, 0);
  $: currentWorktree =
    $worktreesStore.find((worktree) => worktree.mapped_pr_id === data.prId) ?? null;
  $: repoPrOptions = $inboxStore
    .filter((row) => row.repo_id === bundle.summary.repo_id)
    .map((row) => ({ id: row.pr_id, number: row.pr_number, title: row.title }));
  $: activeInboxRow =
    $inboxStore.find(
      (row) => row.pr_id === data.prId && row.account_id === data.activeAccountId
    ) ?? null;
  $: suggestionBlocks = bundle.suggestion_blocks ?? [];
  $: checkAnnotations = bundle.check_annotations ?? [];
  $: openSuggestionBlocks = suggestionBlocks.filter((suggestion) => !suggestion.is_outdated);
  $: suggestionBlocksByComment = openSuggestionBlocks.reduce((acc, suggestion) => {
    const existing = acc.get(suggestion.comment_id) ?? [];
    existing.push(suggestion);
    acc.set(suggestion.comment_id, existing);
    return acc;
  }, new Map<string, SuggestionBlock[]>());

  onMount(async () => {
    await Promise.all([
      initSubscription(data.prId, data.activeAccountId),
      refreshPending(),
      refreshPushHistory()
    ]);
    window.addEventListener('command:composer-focus', onCommandComposerFocus as EventListener);
    window.addEventListener(
      'command:composer-identity-switch',
      onCommandComposerIdentitySwitch as EventListener
    );
    window.addEventListener(
      'command:suggestion-batch-open',
      onCommandSuggestionBatchOpen as EventListener
    );
    window.addEventListener('command:pr-refresh', onCommandPrRefresh as EventListener);
  });

  function overlayOptimism(overlay: PendingOverlay | null): PendingMutationView['optimism'] | null {
    if (!overlay) {
      return null;
    }
    return optimismByMutationId.get(overlay.mutation_id) ?? null;
  }

  function failureFor(targetId: string): PendingMutationView | null {
    return failedByTarget.get(targetId) ?? null;
  }

  function requiresConnection(kind: MutationKind): boolean {
    return connectionRequiredKinds.includes(kind);
  }

  async function refreshPending(): Promise<void> {
    pendingMutations = await listPendingMutations(data.activeAccountId, true);
  }

  async function refreshPushHistory(): Promise<void> {
    pushHistory = await listPrPushes(data.prId);
  }

  async function refreshBundle(): Promise<void> {
    refreshing = true;
    invalidatePrDetail(data.activeAccountId, data.prId);
    const nextBundle = await getPrDetailBundle(data.activeAccountId, data.prId, true);
    if (nextBundle) {
      bundle = nextBundle;
      titleDraft = nextBundle.summary.title;
      bodyDraft = nextBundle.summary.body;
    }
    await refreshPushHistory();
    refreshing = false;
  }

  async function initSubscription(prId: string, accountId: string): Promise<void> {
    const listeners = await Promise.all([
      listenEventPayload(`pr:${prId} changed`, async () => {
        await refreshBundle();
      }),
      listenEventPayload<MutationFailedEventPayload>('mutation:failed', async (payload) => {
        if (payload.error_kind.kind === 'network') {
          await refreshPending();
          return;
        }
        await refreshPending();
      }),
      listenEventPayload('mutation:submitted', refreshPending),
      listenEventPayload('mutation:applied', refreshPending),
      listenEventPayload('mutation:reconciled', refreshPending),
      listenEventPayload('mutation:rolled-back', refreshPending),
      listenEventPayload<{ mutation_id: string; conflict: HardConflictPayload }>(
        'mutation:hard-conflict',
        async (payload) => {
          conflict = payload.conflict;
          await refreshPending();
        }
      ),
      listenEventPayload<{ account_id: string; state: NetState }>(
        `network:${accountId} changed`,
        (payload) => {
          networkState = payload.state;
        }
      ),
      listenEventPayload('worktree:discovery completed', async () => {
        await refreshAccountData(accountId);
      })
    ]);
    unlisten = listeners;
  }

  async function submit(
    kind: MutationKind,
    payload: Record<string, unknown>
  ): Promise<SubmittedMutation | void> {
    if (offline && requiresConnection(kind)) {
      return;
    }
    const submitted = await submitMutation(data.activeAccountId, kind, JSON.stringify(payload));
    await refreshPending();
    return submitted;
  }

  function submitWithConfirmation(
    kind: MutationKind,
    payload: Record<string, unknown>,
    title: string,
    message: string
  ): void {
    confirmModal = { kind, payload, title, message };
  }

  async function confirmAndSubmit(): Promise<void> {
    if (!confirmModal) {
      return;
    }
    await submit(confirmModal.kind, confirmModal.payload);
    confirmModal = null;
  }

  async function onRetry(mutationId: string): Promise<void> {
    await retryMutation(mutationId);
    await refreshPending();
  }

  async function onDiscard(mutationId: string): Promise<void> {
    await discardMutation(mutationId);
    await refreshPending();
  }

  async function onReviewSubmit(): Promise<void> {
    await submit('submit_review', {
      pr_id: data.prId,
      body: reviewBody,
      target_id: data.prId
    });
    reviewBody = '';
    showReviewModal = false;
  }

  async function onCommandComposerFocus(event: Event): Promise<void> {
    const detail = (event as CustomEvent<{ ensureConversationTab?: boolean }>).detail;
    if (detail?.ensureConversationTab) {
      activeTab = 'conversation';
      await tick();
    }
    const textarea = document.querySelector<HTMLTextAreaElement>('[data-testid="composer-textarea"]');
    textarea?.focus();
  }

  async function onCommandComposerIdentitySwitch(): Promise<void> {
    activeTab = 'conversation';
    await tick();
    const select = document.querySelector<HTMLSelectElement>('[data-testid="composer-posting-identity-select"]');
    if (!select) {
      return;
    }
    select.focus();
    select.click();
  }

  async function onCommandSuggestionBatchOpen(): Promise<void> {
    activeTab = 'conversation';
    await tick();
    showSuggestionBatchModal = true;
  }

  function onCommandPrRefresh(): void {
    void refreshBundle();
  }

  onDestroy(() => {
    for (const stop of unlisten) {
      stop();
    }
    unlisten = [];
    window.removeEventListener('command:composer-focus', onCommandComposerFocus as EventListener);
    window.removeEventListener(
      'command:composer-identity-switch',
      onCommandComposerIdentitySwitch as EventListener
    );
    window.removeEventListener(
      'command:suggestion-batch-open',
      onCommandSuggestionBatchOpen as EventListener
    );
    window.removeEventListener('command:pr-refresh', onCommandPrRefresh as EventListener);
  });

  function openCheckLogTail(checkRunId: string): void {
    activeCheckLogRunId = checkRunId;
  }
</script>

<main class="pr-detail px-3 py-3">
  <header class="Box mb-3">
    <div class="Box-header">
      <div class="d-flex flex-items-start flex-justify-between gap-3">
        <div class="flex-auto">
          <div class="d-flex flex-items-center gap-2">
            <PendingAffordance
              overlay={bundle.summary.pending_overlay}
              optimism={overlayOptimism(bundle.summary.pending_overlay)}
            />
            {#if editingTitle}
              <input class="form-control width-full" bind:value={titleDraft} />
              <button
                class="btn btn-sm"
                type="button"
                on:click={async () => {
                  await submit('update_pr_title', { pr_id: data.prId, title: titleDraft, target_id: data.prId });
                  editingTitle = false;
                }}
              >
                Save
              </button>
              <button class="btn btn-sm" type="button" on:click={() => (editingTitle = false)}>Cancel</button>
            {:else}
              <h1 class="f3 m-0">
                {bundle.summary.title} <span class="color-fg-muted">#{bundle.summary.pr_number}</span>
              </h1>
              <button class="btn btn-sm" type="button" on:click={() => (editingTitle = true)}>Edit title</button>
            {/if}
          </div>
          <div class="mt-2 d-flex flex-wrap gap-1">
            <span class={`State ${bundle.summary.state === 'open' ? 'State--open' : ''}`}>{bundle.summary.state}</span>
            {#if bundle.summary.draft}
              <span class="Label Label--secondary">Draft</span>
            {/if}
            {#if bundle.summary.mergeable_state}
              <span class="Label">{bundle.summary.mergeable_state}</span>
            {/if}
            <span class={`Label ${checkBadge.statusClass}`}>{checkBadge.label}</span>
            {#if offline}
              <span class="Label Label--attention">Offline — queued: {queuedMutationCount}</span>
            {/if}
            {#if currentWorktree}
              <WorktreeBadge worktree={currentWorktree} />
            {/if}
            <button class="btn btn-sm" type="button" on:click={() => (syncTrayOpen = !syncTrayOpen)}>
              Sync errors ({failedMutations.length})
            </button>
          </div>
          {#if currentWorktree}
            <div class="mt-2">
              <WorktreeMappingChip
                worktree={currentWorktree}
                {repoPrOptions}
                on:changed={() => refreshAccountData(data.activeAccountId)}
              />
            </div>
          {/if}
        </div>
        <div class="text-right f6 color-fg-muted">
          <div>{bundle.summary.base_ref} ← {bundle.summary.head_ref}</div>
          <div>Updated {formatRelative(bundle.summary.updated_at)}</div>
        </div>
      </div>
      <div class="mt-2 d-flex flex-wrap gap-1">
        {#each bundle.metadata.labels as label}
          <span
            class="IssueLabel"
            style={`--label-r: ${parseInt(label.label_color.slice(0, 2), 16)}; --label-g: ${parseInt(label.label_color.slice(2, 4), 16)}; --label-b: ${parseInt(label.label_color.slice(4, 6), 16)}; background-color: #${label.label_color};`}
          >
            {label.label_name}
          </span>
          <PendingAffordance overlay={label.pending_overlay} optimism={overlayOptimism(label.pending_overlay)} />
        {/each}
      </div>
      <InlineMutationErrorBanner
        mutation={failureFor(data.prId)}
        onRetry={onRetry}
        onDiscard={onDiscard}
      />
      {#if latestPushPair}
        <div class="flash flash-warn mt-2">
          Force-pushed {forcePushCount} {forcePushCount === 1 ? 'time' : 'times'} — <a
            href={`/pr/${data.prId}/range-diff?old=${latestPushPair.old.head_sha}&new=${latestPushPair.new.head_sha}`}
            >view range-diff</a
          >
        </div>
      {/if}
      {#if pendingMutations.some((entry) => entry.status === 'pending')}
        <div class="mt-2 d-flex flex-wrap gap-1">
          {#each pendingMutations.filter((entry) => entry.status === 'pending') as entry (entry.id)}
            <PendingAffordance
              overlay={{ mutation_id: entry.id, kind: entry.kind }}
              optimism={entry.optimism}
            />
          {/each}
        </div>
      {/if}
    </div>
  </header>

  <div class="pr-layout">
    <section class="pr-main">
      {#if openSuggestionBlocks.length >= 1}
        <div class="flash flash-warn mb-2">
          <div class="d-flex flex-items-center flex-justify-between gap-2">
            <span>{openSuggestionBlocks.length} pending suggestions</span>
            <button
              class="btn btn-sm btn-primary"
              type="button"
              data-command-open-batch
              data-testid="suggestion-batch-open"
              on:click={() => (showSuggestionBatchModal = true)}
            >
              Apply {openSuggestionBlocks.length}
            </button>
          </div>
        </div>
      {/if}
      <div class="UnderlineNav mb-2">
        <nav class="UnderlineNav-body" aria-label="Pull request sections">
          <button
            class={`UnderlineNav-item btn-link ${activeTab === 'conversation' ? 'selected' : ''}`}
            data-command-tab="conversation"
            on:click={() => (activeTab = 'conversation')}
          >
            Conversation
          </button>
          <button
            class={`UnderlineNav-item btn-link ${activeTab === 'files' ? 'selected' : ''}`}
            data-command-tab="files"
            on:click={() => (activeTab = 'files')}
          >
            Files
          </button>
          <button
            class={`UnderlineNav-item btn-link ${activeTab === 'checks' ? 'selected' : ''}`}
            data-command-tab="checks"
            on:click={() => (activeTab = 'checks')}
          >
            Checks
          </button>
        </nav>
      </div>

      {#if refreshing}
        <div class="flash flash-warn mb-2">Refreshing from event stream…</div>
      {/if}

      {#if activeTab === 'conversation'}
        <div class="Box mb-2">
          <div class="Box-body">
            {#if editingBody}
              <textarea class="form-control width-full mb-2" rows={4} bind:value={bodyDraft}></textarea>
              <div class="d-flex gap-1">
                <button
                  class="btn btn-sm"
                  type="button"
                  on:click={async () => {
                    await submit('update_pr_description', {
                      pr_id: data.prId,
                      body: bodyDraft,
                      target_id: data.prId
                    });
                    editingBody = false;
                  }}
                >
                  Save description
                </button>
                <button class="btn btn-sm" type="button" on:click={() => (editingBody = false)}>
                  Cancel
                </button>
              </div>
            {:else}
              <article class="markdown-body mb-1" aria-label="Pull request description">
                <p>{bundle.summary.body}</p>
              </article>
              <ServerAdjustedChip visible={bundle.summary.body_server_adjusted} />
              <div class="mt-2">
                <button class="btn btn-sm" type="button" on:click={() => (editingBody = true)}>
                  Edit description
                </button>
              </div>
            {/if}
          </div>
        </div>

        <Composer
          accountId={data.activeAccountId}
          targetType="pr_comment"
          targetId={data.prId}
          submitKind="add_comment"
          payloadBase={{ pr_id: data.prId }}
          submitLabel="Add comment"
          on:submitted={refreshPending}
        />

        <div class="Box mt-2 mb-2">
          <div class="Box-header d-flex flex-items-center flex-justify-between">
            <span>Review</span>
            <button class="btn btn-sm" type="button" on:click={() => (showReviewModal = true)}>
              Submit review
            </button>
          </div>
        </div>

        <div class="Box">
          <div class="Box-body">
            <ul class="list-style-none m-0">
              {#each conversation as item}
                <li class="mb-2 border rounded-2" data-command-thread-id={item.kind === 'thread' ? item.id : undefined}>
                  <div class="p-2 border-bottom color-border-muted d-flex flex-items-center flex-justify-between">
                    <strong class="f6">{item.title}</strong>
                    <span class="f6 color-fg-muted">{formatRelative(item.createdAt)}</span>
                  </div>
                  <div class="p-2">
                    {#if item.kind === 'comment' || item.kind === 'review'}
                      <div class="d-flex flex-items-center gap-1 mb-1">
                        <div class="f6 color-fg-muted">@{item.author}</div>
                        <PendingAffordance
                          overlay={item.pendingOverlay}
                          optimism={overlayOptimism(item.pendingOverlay)}
                        />
                      </div>
                      <article class="markdown-body" aria-label={item.kind}>
                        {@html item.html}
                      </article>
                      <ServerAdjustedChip visible={item.bodyServerAdjusted} />
                      {#if suggestionBlocksByComment.get(item.id)?.length}
                        <div class="mt-2 d-flex flex-column gap-2">
                          {#each suggestionBlocksByComment.get(item.id) ?? [] as suggestion}
                            <div class="Box" data-command-suggestion-id={suggestion.id}>
                              <div class="Box-header d-flex flex-items-center flex-justify-between">
                                <span class="f6 text-mono">
                                  {suggestion.path}:{suggestion.start_line}-{suggestion.end_line}
                                </span>
                                <NoOptimismButton
                                  kind="apply_suggestion"
                                  payload={{
                                    owner: activeInboxRow?.repo_owner ?? '',
                                    repo: activeInboxRow?.repo_name ?? '',
                                    pr_number: bundle.summary.pr_number,
                                    pr_id: data.prId,
                                    review_comment_id: suggestion.comment_id,
                                    expected_head_sha: bundle.summary.head_sha,
                                    target_id: suggestion.comment_id
                                  }}
                                  label="Apply suggestion"
                                  pendingLabel="Applying…"
                                  className="btn btn-sm btn-primary"
                                  confirmTitle="Confirm suggestion apply"
                                  confirmMessage="Apply this suggestion directly to the PR branch?"
                                  disabled={suggestion.is_outdated || !activeInboxRow}
                                  disabledReason={
                                    suggestion.is_outdated
                                      ? 'Suggestion is outdated and can no longer be applied.'
                                      : 'Repository owner/name is unavailable.'
                                  }
                                  {submit}
                                  onComplete={refreshBundle}
                                />
                              </div>
                              <div class="Box-body">
                                <pre class="m-0 f6">{suggestion.body}</pre>
                              </div>
                            </div>
                          {/each}
                        </div>
                      {/if}
                      {#if item.reviewState}
                        <div class="mt-1"><span class="Label">{item.reviewState}</span></div>
                      {/if}
                      <div class="mt-2 d-flex flex-wrap gap-1">
                        <button
                          class="btn btn-sm"
                          type="button"
                          on:click={() =>
                            submit('add_reaction', {
                              pr_id: data.prId,
                              comment_id: item.id,
                              reaction: '+1',
                              target_id: item.id
                            })}
                        >
                          Add reaction
                        </button>
                        <button
                          class="btn btn-sm"
                          type="button"
                          on:click={() =>
                            submit('remove_reaction', {
                              pr_id: data.prId,
                              comment_id: item.id,
                              reaction: '+1',
                              target_id: item.id
                            })}
                        >
                          Remove reaction
                        </button>
                        <button
                          class="btn btn-sm"
                          type="button"
                          on:click={() =>
                            submit('edit_comment', {
                              pr_id: data.prId,
                              comment_id: item.id,
                              body: `${item.title} (edited)`,
                              target_id: item.id
                            })}
                        >
                          Edit
                        </button>
                        <button
                          class="btn btn-sm btn-danger"
                          type="button"
                          on:click={() =>
                            submit('delete_comment', {
                              pr_id: data.prId,
                              comment_id: item.id,
                              target_id: item.id
                            })}
                        >
                          Delete
                        </button>
                      </div>
                      <InlineMutationErrorBanner
                        mutation={failureFor(item.id)}
                        onRetry={onRetry}
                        onDiscard={onDiscard}
                      />
                    {:else if item.kind === 'thread'}
                      <div class="d-flex flex-items-center gap-1">
                        <p class="f6 mb-1">{item.path} · line {item.line} · side {item.side}</p>
                        <PendingAffordance
                          overlay={item.pendingOverlay}
                          optimism={overlayOptimism(item.pendingOverlay)}
                        />
                      </div>
                      <p class="f6 color-fg-muted mb-0">{item.commentCount} comments</p>
                      {#if item.isOutdated}
                        <span class="Label Label--attention mt-1">Outdated</span>
                      {/if}
                      <div class="mt-2 d-flex gap-1">
                        <button
                          class="btn btn-sm"
                          type="button"
                          data-command-thread-id={item.id}
                          data-command-thread-focus
                          data-command-action="resolve-thread"
                          on:click={() =>
                            submit('resolve_thread', {
                              pr_id: data.prId,
                              thread_id: item.id,
                              target_id: item.id
                            })}
                        >
                          Resolve
                        </button>
                        <button
                          class="btn btn-sm"
                          type="button"
                          data-command-thread-id={item.id}
                          data-command-action="unresolve-thread"
                          on:click={() =>
                            submit('unresolve_thread', {
                              pr_id: data.prId,
                              thread_id: item.id,
                              target_id: item.id
                            })}
                        >
                          Unresolve
                        </button>
                      </div>
                      <InlineMutationErrorBanner
                        mutation={failureFor(item.id)}
                        onRetry={onRetry}
                        onDiscard={onDiscard}
                      />
                    {:else if item.kind === 'event'}
                      <p class="f6 mb-0">{item.details}</p>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        </div>

        <div class="Box mt-2">
          <div class="Box-header">Pull request actions</div>
          <div class="Box-body d-flex flex-wrap gap-1">
            <button
              class="btn btn-sm"
              type="button"
              on:click={() => submit('convert_to_draft', { pr_id: data.prId, target_id: data.prId })}
            >
              Convert to draft
            </button>
            <button
              class="btn btn-sm"
              type="button"
              on:click={() =>
                submit('mark_ready_for_review', { pr_id: data.prId, target_id: data.prId })}
            >
              Mark ready
            </button>
            <button
              class="btn btn-sm btn-danger"
              type="button"
              on:click={() =>
                submitWithConfirmation(
                  'close_pr',
                  { pr_id: data.prId, target_id: data.prId },
                  'Confirm close',
                  'Close this pull request?'
                )}
              disabled={offline}
              title={offline ? 'requires connection' : ''}
            >
              Close PR
            </button>
            <button
              class="btn btn-sm"
              type="button"
              on:click={() =>
                submitWithConfirmation(
                  'reopen_pr',
                  { pr_id: data.prId, target_id: data.prId },
                  'Confirm reopen',
                  'Reopen this pull request?'
                )}
              disabled={offline}
              title={offline ? 'requires connection' : ''}
            >
              Reopen PR
            </button>
          </div>
        </div>
        <MergeBox
          summary={bundle.summary}
          repo={{
            owner: activeInboxRow?.repo_owner ?? '',
            name: activeInboxRow?.repo_name ?? '',
            merge_commit_allowed: bundle.summary.merge_commit_allowed ?? false,
            squash_merge_allowed: bundle.summary.squash_merge_allowed ?? false,
            rebase_merge_allowed: bundle.summary.rebase_merge_allowed ?? false,
            delete_branch_on_merge_default: bundle.summary.delete_branch_on_merge_default ?? false,
            repo_has_merge_queue: bundle.summary.repo_has_merge_queue ?? false
          }}
          viewer={{
            viewer_can_merge: bundle.summary.viewer_can_merge ?? false,
            viewer_can_enable_auto_merge: bundle.summary.viewer_can_enable_auto_merge ?? false,
            viewer_can_disable_auto_merge: bundle.summary.viewer_can_disable_auto_merge ?? false,
            viewer_can_update_branch: bundle.summary.viewer_can_update_branch ?? false,
            viewer_can_delete_head_ref: bundle.summary.viewer_can_delete_head_ref ?? false
          }}
          accountId={data.activeAccountId}
          {offline}
          onSubmit={submit}
        />
      {:else if activeTab === 'files'}
        <div class="Box mb-2">
          <div class="Box-header">Viewed files</div>
          <ul class="Box-body list-style-none m-0">
            {#each bundle.files as file}
              <li class="d-flex flex-items-center gap-2 py-1">
                <label class="d-flex flex-items-center gap-2 flex-auto">
                  <input
                    type="checkbox"
                    data-command-file-path={file.path}
                    checked={file.is_viewed}
                    on:change={(event) => {
                      const checked = (event.currentTarget as HTMLInputElement).checked;
                      const payload = {
                        pr_id: data.prId,
                        path: file.path,
                        head_sha: bundle.summary.head_sha,
                        status: file.status,
                        is_binary: file.is_binary ? 1 : 0,
                        additions: file.additions,
                        deletions: file.deletions,
                        previous_viewed_by_account_id: file.viewed_by_account_id,
                        previous_viewed_at_head_sha: file.viewed_at_head_sha,
                        target_id: file.path
                      };
                      if (checked) {
                        return submit('mark_file_viewed', payload);
                      }
                      return submit('unmark_file_viewed', payload);
                    }}
                  />
                  <span class="text-mono f6">{file.path}</span>
                </label>
                <PendingAffordance
                  overlay={file.pending_overlay}
                  optimism={overlayOptimism(file.pending_overlay)}
                />
                <InlineMutationErrorBanner
                  mutation={failureFor(file.path)}
                  onRetry={onRetry}
                  onDiscard={onDiscard}
                />
              </li>
            {/each}
          </ul>
        </div>
        <DiffViewer
          patch={bundle.patch}
          files={bundle.files}
          reviewThreads={bundle.review_threads}
          checkAnnotations={checkAnnotations}
          accountId={data.activeAccountId}
          prId={data.prId}
          headSha={bundle.summary.head_sha}
          pullRequestNodeId={bundle.summary.pr_id}
          on:reviewcommentsubmitted={refreshBundle}
          on:openlogtail={(event) => openCheckLogTail(event.detail.checkRunId)}
        />
      {:else}
        <ChecksRail
          accountId={data.activeAccountId}
          prId={data.prId}
          owner={activeInboxRow?.repo_owner ?? ''}
          repo={activeInboxRow?.repo_name ?? ''}
          headSha={bundle.summary.head_sha}
          checks={bundle.checks}
          pendingMutations={pendingMutations}
          on:openlogtail={(event) => openCheckLogTail(event.detail.checkRunId)}
        />
      {/if}

      <CheckLogTail checkRunId={activeCheckLogRunId} on:close={() => (activeCheckLogRunId = null)} />
    </section>

    <aside class="pr-rail">
      <div class="Box mb-2">
        <div class="Box-header">Reviewers</div>
        <div class="Box-body">
          {#if bundle.metadata.requested_reviewers.length === 0}
            <p class="f6 color-fg-muted mb-0">None yet</p>
          {:else}
            <ul class="list-style-none m-0">
              {#each bundle.metadata.requested_reviewers as reviewer}
                <li class="f6 mb-1 d-flex flex-items-center gap-1">
                  <span>{reviewer.login ?? reviewer.user_id}</span>
                  <PendingAffordance
                    overlay={reviewer.pending_overlay}
                    optimism={overlayOptimism(reviewer.pending_overlay)}
                  />
                  <button
                    class="btn btn-sm"
                    type="button"
                    on:click={() =>
                      submit('remove_review_request', {
                        pr_id: data.prId,
                        reviewer: reviewer.login ?? reviewer.user_id,
                        target_id: data.prId
                      })}
                  >
                    Remove
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="mt-2 d-flex gap-1">
            <input
              class="form-control"
              placeholder="reviewer login"
              bind:value={requestReviewValue}
            />
            <button
              class="btn btn-sm"
              type="button"
              on:click={async () => {
                await submit('request_review', {
                  pr_id: data.prId,
                  reviewers: requestReviewValue.split(',').map((item) => item.trim()).filter(Boolean),
                  target_id: data.prId
                });
                requestReviewValue = '';
              }}
            >
              Request
            </button>
          </div>
        </div>
      </div>

      <div class="Box mb-2">
        <div class="Box-header">Assignees</div>
        <div class="Box-body">
          {#if bundle.metadata.assignees.length === 0}
            <p class="f6 color-fg-muted mb-0">None</p>
          {:else}
            <ul class="list-style-none m-0">
              {#each bundle.metadata.assignees as assignee}
                <li class="f6 mb-1 d-flex flex-items-center gap-1">
                  <span>{assignee.login ?? assignee.user_id}</span>
                  <PendingAffordance
                    overlay={assignee.pending_overlay}
                    optimism={overlayOptimism(assignee.pending_overlay)}
                  />
                </li>
              {/each}
            </ul>
          {/if}
          <div class="mt-2 d-flex gap-1">
            <input
              class="form-control"
              bind:value={setAssigneesValue}
              placeholder="comma separated logins"
            />
            <button
              class="btn btn-sm"
              type="button"
              on:click={() =>
                submit('set_assignees', {
                  pr_id: data.prId,
                  assignees: setAssigneesValue.split(',').map((item) => item.trim()).filter(Boolean),
                  target_id: data.prId
                })}
            >
              Set
            </button>
          </div>
        </div>
      </div>

      <div class="Box mb-2">
        <div class="Box-header">Labels</div>
        <div class="Box-body d-flex flex-wrap gap-1">
          {#each bundle.metadata.labels as label}
            <button
              class="Label"
              type="button"
              on:click={() =>
                submit('remove_label', {
                  pr_id: data.prId,
                  label_name: label.label_name,
                  target_id: data.prId
                })}
            >
              {label.label_name} ×
            </button>
          {/each}
        </div>
        <div class="Box-body border-top color-border-muted d-flex gap-1">
          <input class="form-control" bind:value={addingLabel} placeholder="new label" />
          <button
            class="btn btn-sm"
            type="button"
            on:click={async () => {
              await submit('add_label', {
                pr_id: data.prId,
                label_name: addingLabel,
                target_id: data.prId
              });
              addingLabel = '';
            }}
          >
            Add
          </button>
        </div>
      </div>

      <div class="Box mb-2">
        <div class="Box-header">Projects</div>
        <div class="Box-body">
          {#if bundle.metadata.projects.length === 0}
            <p class="f6 color-fg-muted mb-0">No projects</p>
          {:else}
            {#each bundle.metadata.projects as project}
              <p class="f6 mb-1 d-flex flex-items-center gap-1">
                <span>{project.project_title} · {project.status ?? 'No status'}</span>
                <PendingAffordance
                  overlay={project.pending_overlay}
                  optimism={overlayOptimism(project.pending_overlay)}
                />
              </p>
            {/each}
          {/if}
          <div class="mt-2 d-flex gap-1">
            <input class="form-control" bind:value={projectValue} placeholder="project title" />
            <button
              class="btn btn-sm"
              type="button"
              on:click={() =>
                submit('set_project', {
                  pr_id: data.prId,
                  project_title: projectValue,
                  target_id: data.prId
                })}
            >
              Set project
            </button>
          </div>
        </div>
      </div>

      <div class="Box">
        <div class="Box-header">Milestones</div>
        <div class="Box-body">
          {#if bundle.metadata.milestones.length === 0}
            <p class="f6 color-fg-muted mb-0">No milestones</p>
          {:else}
            {#each bundle.metadata.milestones as milestone}
              <p class="f6 mb-1 d-flex flex-items-center gap-1">
                <span>{milestone.title}</span>
                <PendingAffordance
                  overlay={milestone.pending_overlay}
                  optimism={overlayOptimism(milestone.pending_overlay)}
                />
              </p>
            {/each}
          {/if}
          <div class="mt-2 d-flex gap-1">
            <input class="form-control" bind:value={milestoneValue} placeholder="milestone title" />
            <button
              class="btn btn-sm"
              type="button"
              on:click={() =>
                submit('set_milestone', {
                  pr_id: data.prId,
                  title: milestoneValue,
                  target_id: data.prId
                })}
            >
              Set milestone
            </button>
          </div>
        </div>
      </div>
    </aside>
  </div>

  <SuggestionBatchModal
    open={showSuggestionBatchModal}
    prId={data.prId}
    expectedHeadSha={bundle.summary.head_sha}
    suggestions={openSuggestionBlocks}
    worktree={currentWorktree}
    {submit}
    on:close={() => (showSuggestionBatchModal = false)}
    on:submitted={async () => {
      showSuggestionBatchModal = false;
      await refreshBundle();
    }}
  />

  <SyncErrorsTray
    open={syncTrayOpen}
    failedMutations={failedMutations}
    onClose={() => (syncTrayOpen = false)}
    onRetry={onRetry}
    onDiscard={onDiscard}
  />

  <HardConflictModal
    conflict={conflict}
    onClose={() => (conflict = null)}
    onDiscard={async (mutationId) => {
      await onDiscard(mutationId);
      conflict = null;
    }}
    onRefreshAndRetry={async (mutationId) => {
      await refreshBundle();
      await onRetry(mutationId);
      conflict = null;
    }}
  />

  {#if showReviewModal}
    <div class="conflict-modal-backdrop" role="presentation">
      <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label="Submit review">
        <div class="Box-header d-flex flex-items-center flex-justify-between">
          <h2 class="f5 m-0">Submit review</h2>
          <button class="btn btn-sm" type="button" on:click={() => (showReviewModal = false)}>Close</button>
        </div>
        <div class="Box-body">
          <textarea class="form-control width-full" rows={5} bind:value={reviewBody}></textarea>
          <div class="mt-2 d-flex flex-justify-end">
            <button class="btn btn-primary" type="button" on:click={onReviewSubmit}>Submit review</button>
          </div>
        </div>
      </div>
    </div>
  {/if}

  {#if confirmModal}
    <div class="conflict-modal-backdrop" role="presentation">
      <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label={confirmModal.title}>
        <div class="Box-header d-flex flex-items-center flex-justify-between">
          <h2 class="f5 m-0">{confirmModal.title}</h2>
          <button class="btn btn-sm" type="button" on:click={() => (confirmModal = null)}>Close</button>
        </div>
        <div class="Box-body">
          <p class="f6 mb-2">{confirmModal.message}</p>
          <div class="d-flex gap-1">
            <button class="btn btn-primary" type="button" on:click={confirmAndSubmit}>Confirm</button>
            <button class="btn" type="button" on:click={() => (confirmModal = null)}>Cancel</button>
          </div>
        </div>
      </div>
    </div>
  {/if}
</main>

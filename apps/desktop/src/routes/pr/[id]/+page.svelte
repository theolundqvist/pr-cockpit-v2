<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import DiffViewer from '$lib/components/DiffViewer.svelte';
  import { checksRollupBadge, getPrDetailBundle, invalidatePrDetail, type PrDetailBundle } from '$lib/data/pr-detail';
  import { listenEvent } from '$lib/ipc/client';
  import { reduceConversationTimeline } from '$lib/timeline/reducer';
  import { formatRelative } from '$lib/utils/time';
  import type { PageData } from './$types';

  export let data: PageData;

  let bundle: PrDetailBundle = data.bundle;
  let activeTab: 'conversation' | 'files' | 'checks' = 'conversation';
  let refreshing = false;
  let unlisten: (() => void) | null = null;

  $: checkBadge = checksRollupBadge(bundle.checks);
  $: conversation = reduceConversationTimeline(bundle.summary, bundle.timeline, bundle.review_threads, bundle.metadata);

  onMount(async () => {
    await initSubscription(data.prId, data.activeAccountId);
  });

  async function initSubscription(prId: string, accountId: string): Promise<void> {
    if (unlisten) {
      return;
    }
    unlisten = await listenEvent(`pr:${prId} changed`, async () => {
      refreshing = true;
      invalidatePrDetail(accountId, prId);
      const nextBundle = await getPrDetailBundle(accountId, prId, true);
      if (nextBundle) {
        bundle = nextBundle;
      }
      refreshing = false;
    });
  }

  onDestroy(() => {
    unlisten?.();
    unlisten = null;
  });
</script>

<main class="pr-detail px-3 py-3">
  <header class="Box mb-3">
    <div class="Box-header">
      <div class="d-flex flex-items-start flex-justify-between gap-3">
        <div class="flex-auto">
          <h1 class="f3 m-0">{bundle.summary.title} <span class="color-fg-muted">#{bundle.summary.pr_number}</span></h1>
          <div class="mt-2 d-flex flex-wrap gap-1">
            <span class={`State ${bundle.summary.state === 'open' ? 'State--open' : ''}`}>{bundle.summary.state}</span>
            {#if bundle.summary.draft}
              <span class="Label Label--secondary">Draft</span>
            {/if}
            {#if bundle.summary.mergeable_state}
              <span class="Label">{bundle.summary.mergeable_state}</span>
            {/if}
            <span class={`Label ${checkBadge.statusClass}`}>{checkBadge.label}</span>
          </div>
        </div>
        <div class="text-right f6 color-fg-muted">
          <div>{bundle.summary.base_ref} ← {bundle.summary.head_ref}</div>
          <div>Updated {formatRelative(bundle.summary.updated_at)}</div>
        </div>
      </div>
      <div class="mt-2 d-flex flex-wrap gap-1">
        {#each bundle.metadata.labels as label}
          <span class="IssueLabel" style={`--label-r: ${parseInt(label.label_color.slice(0, 2), 16)}; --label-g: ${parseInt(label.label_color.slice(2, 4), 16)}; --label-b: ${parseInt(label.label_color.slice(4, 6), 16)}; background-color: #${label.label_color};`}>
            {label.label_name}
          </span>
        {/each}
      </div>
    </div>
  </header>

  <div class="pr-layout">
    <section class="pr-main">
      <div class="UnderlineNav mb-2">
        <nav class="UnderlineNav-body" aria-label="Pull request sections">
          <button class={`UnderlineNav-item btn-link ${activeTab === 'conversation' ? 'selected' : ''}`} on:click={() => (activeTab = 'conversation')}>
            Conversation
          </button>
          <button class={`UnderlineNav-item btn-link ${activeTab === 'files' ? 'selected' : ''}`} on:click={() => (activeTab = 'files')}>
            Files
          </button>
          <button class={`UnderlineNav-item btn-link ${activeTab === 'checks' ? 'selected' : ''}`} on:click={() => (activeTab = 'checks')}>
            Checks
          </button>
        </nav>
      </div>

      {#if refreshing}
        <div class="flash flash-warn mb-2">Refreshing from event stream…</div>
      {/if}

      {#if activeTab === 'conversation'}
        <div class="Box">
          <div class="Box-body">
            <article class="markdown-body mb-3" aria-label="Pull request description">
              <p>{bundle.summary.body}</p>
            </article>
            <ul class="list-style-none m-0">
              {#each conversation as item}
                <li class="mb-2 border rounded-2">
                  <div class="p-2 border-bottom color-border-muted d-flex flex-items-center flex-justify-between">
                    <strong class="f6">{item.title}</strong>
                    <span class="f6 color-fg-muted">{formatRelative(item.createdAt)}</span>
                  </div>
                  <div class="p-2">
                    {#if item.kind === 'comment' || item.kind === 'review'}
                      <div class="f6 color-fg-muted mb-1">@{item.author}</div>
                      <article class="markdown-body" aria-label={item.kind}>
                        {@html item.html}
                      </article>
                      {#if item.reviewState}
                        <div class="mt-1"><span class="Label">{item.reviewState}</span></div>
                      {/if}
                    {:else if item.kind === 'thread'}
                      <p class="f6 mb-1">{item.path} · line {item.line} · side {item.side}</p>
                      <p class="f6 color-fg-muted mb-0">{item.commentCount} comments</p>
                      {#if item.isOutdated}
                        <span class="Label Label--attention mt-1">Outdated</span>
                      {/if}
                    {:else if item.kind === 'event'}
                      <p class="f6 mb-0">{item.details}</p>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        </div>
      {:else if activeTab === 'files'}
        <DiffViewer patch={bundle.patch} files={bundle.files} reviewThreads={bundle.review_threads} />
      {:else}
        <div class="Box">
          <div class="Box-header">Check runs</div>
          <ul class="Box-body list-style-none m-0">
            {#if bundle.checks.runs.length === 0}
              <li class="f6 color-fg-muted">No checks found for this PR.</li>
            {:else}
              {#each bundle.checks.runs as run}
                <li class="d-flex flex-items-center flex-justify-between py-2 border-bottom color-border-muted">
                  <span>{run.name}</span>
                  <span class="Label">{run.conclusion ?? run.status}</span>
                </li>
              {/each}
            {/if}
          </ul>
        </div>
      {/if}
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
                <li class="f6 mb-1">{reviewer.login ?? reviewer.user_id}</li>
              {/each}
            </ul>
          {/if}
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
                <li class="f6 mb-1">{assignee.login ?? assignee.user_id}</li>
              {/each}
            </ul>
          {/if}
        </div>
      </div>

      <div class="Box mb-2">
        <div class="Box-header">Labels</div>
        <div class="Box-body d-flex flex-wrap gap-1">
          {#each bundle.metadata.labels as label}
            <span class="Label">{label.label_name}</span>
          {/each}
        </div>
      </div>

      <div class="Box mb-2">
        <div class="Box-header">Projects</div>
        <div class="Box-body">
          {#if bundle.metadata.projects.length === 0}
            <p class="f6 color-fg-muted mb-0">No projects</p>
          {:else}
            {#each bundle.metadata.projects as project}
              <p class="f6 mb-1">{project.project_title} · {project.status ?? 'No status'}</p>
            {/each}
          {/if}
        </div>
      </div>

      <div class="Box">
        <div class="Box-header">Milestones</div>
        <div class="Box-body">
          {#if bundle.metadata.milestones.length === 0}
            <p class="f6 color-fg-muted mb-0">No milestones</p>
          {:else}
            {#each bundle.metadata.milestones as milestone}
              <p class="f6 mb-1">{milestone.title}</p>
            {/each}
          {/if}
          <p class="f6 color-fg-muted mb-0">Linked issues (placeholder)</p>
        </div>
      </div>
    </aside>
  </div>
</main>

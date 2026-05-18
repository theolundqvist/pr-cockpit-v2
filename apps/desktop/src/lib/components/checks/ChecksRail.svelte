<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import PendingAffordance from '$lib/components/PendingAffordance.svelte';
  import { submitMutation } from '$lib/ipc/client';
  import type { CheckRunSummary, PendingMutationView, PrCheckSummary } from '$lib/ipc/bindings';

  export let accountId = '';
  export let prId = '';
  export let owner = '';
  export let repo = '';
  export let headSha = '';
  export let checks: PrCheckSummary = {
    total_runs: 0,
    successful_runs: 0,
    failed_runs: 0,
    pending_runs: 0,
    runs: []
  };
  export let pendingMutations: PendingMutationView[] = [];

  const dispatch = createEventDispatcher<{ openlogtail: { checkRunId: string } }>();

  type SuiteGroup = {
    id: string;
    appName: string;
    status: string;
    conclusion: string | null;
    runs: CheckRunSummary[];
  };

  $: suites = groupSuites(checks.runs ?? []);

  function groupSuites(runs: CheckRunSummary[]): SuiteGroup[] {
    const bySuite = new Map<string, SuiteGroup>();
    for (const run of runs) {
      const suiteId = run.check_suite_id ?? run.id;
      const existing = bySuite.get(suiteId);
      if (existing) {
        existing.runs.push(run);
        continue;
      }
      bySuite.set(suiteId, {
        id: suiteId,
        appName: run.app_name ?? 'checks',
        status: run.check_suite_status ?? run.status,
        conclusion: run.check_suite_conclusion ?? run.conclusion ?? null,
        runs: [run]
      });
    }
    return [...bySuite.values()].sort((left, right) => left.id.localeCompare(right.id));
  }

  function pendingFor(targetId: string): PendingMutationView | null {
    return (
      pendingMutations.find((entry) => entry.target_id === targetId && entry.status === 'pending') ?? null
    );
  }

  async function rerunCheckRun(run: CheckRunSummary): Promise<void> {
    if (!run.rest_id) {
      return;
    }
    await submitMutation(
      accountId,
      'rerun_check_run',
      JSON.stringify({
        pr_id: prId,
        owner,
        repo,
        check_run_id: run.id,
        check_run_rest_id: run.rest_id,
        head_sha: headSha,
        target_type: 'check_run',
        target_id: run.id,
        idempotency_key: `rerun:${run.id}:${headSha}`
      })
    );
  }

  async function rerunCheckSuite(suiteId: string): Promise<void> {
    await submitMutation(
      accountId,
      'rerun_check_suite',
      JSON.stringify({
        pr_id: prId,
        owner,
        repo,
        check_suite_id: suiteId,
        head_sha: headSha,
        target_type: 'check_suite',
        target_id: suiteId,
        idempotency_key: `rerun_suite:${suiteId}:${headSha}`
      })
    );
  }

  function conclusionLabel(run: CheckRunSummary): string {
    return run.conclusion ?? run.status;
  }

  function isFailed(run: CheckRunSummary): boolean {
    const conclusion = run.conclusion?.toLowerCase();
    return run.status.toLowerCase() === 'completed' && conclusion !== 'success' && conclusion !== 'neutral';
  }
</script>

<div class="Box">
  <div class="Box-header d-flex flex-items-center flex-justify-between">
    <span class="text-bold">Checks</span>
    <span class="color-fg-muted f6">{checks.total_runs} runs</span>
  </div>
  <div class="Box-body">
    {#if checks.runs.length === 0}
      <p class="f6 color-fg-muted mb-0">No checks found for this PR.</p>
    {:else}
      {#each suites as suite}
        {@const suitePending = pendingFor(suite.id)}
        <section class="mb-3">
          <div class="d-flex flex-items-center flex-justify-between mb-1">
            <div class="d-flex flex-items-center gap-1">
              <span class="text-bold">{suite.appName}</span>
              <span class="Label">{suite.conclusion ?? suite.status}</span>
              {#if suitePending}
                <span class="Label Label--attention">queued</span>
                <PendingAffordance
                  overlay={{ mutation_id: suitePending.id, kind: suitePending.kind }}
                  optimism={suitePending.optimism}
                />
              {/if}
            </div>
            <button class="btn btn-sm" type="button" on:click={() => rerunCheckSuite(suite.id)}>
              <span class="octicon octicon-sync" aria-hidden="true"></span>
              Rerun suite
            </button>
          </div>
          <ul class="list-style-none m-0">
            {#each suite.runs as run}
              {@const runPending = pendingFor(run.id)}
              <li class="d-flex flex-items-center flex-justify-between py-1">
                <div class="d-flex flex-items-center gap-1">
                  <span>{run.name}</span>
                  <span class="Label Label--secondary">{conclusionLabel(run)}</span>
                  {#if runPending}
                    <span class="Label Label--attention">queued</span>
                    <PendingAffordance
                      overlay={{ mutation_id: runPending.id, kind: runPending.kind }}
                      optimism={runPending.optimism}
                    />
                  {/if}
                </div>
                <div class="d-flex flex-items-center gap-1">
                  {#if isFailed(run)}
                    <button
                      class="btn btn-sm"
                      type="button"
                      title="Open log tail"
                      on:click={() => dispatch('openlogtail', { checkRunId: run.id })}
                    >
                      <span class="octicon octicon-terminal" aria-hidden="true"></span>
                    </button>
                  {/if}
                  <button class="btn btn-sm" type="button" disabled={!run.rest_id} on:click={() => rerunCheckRun(run)}>
                    <span class="octicon octicon-sync" aria-hidden="true"></span>
                    Rerun
                  </button>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
</div>

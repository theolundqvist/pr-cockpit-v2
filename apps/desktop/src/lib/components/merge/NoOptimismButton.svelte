<script lang="ts">
  import { onDestroy } from 'svelte';

  import InlineMutationErrorBanner from '$lib/components/InlineMutationErrorBanner.svelte';
  import { discardMutation, listenEventPayload, retryMutation } from '$lib/ipc/client';
  import type {
    MutationFailedEventPayload,
    MutationKind,
    PendingMutationView,
    SubmittedMutation
  } from '$lib/ipc/bindings';

  export let kind: MutationKind;
  export let payload: Record<string, unknown>;
  export let label = 'Submit';
  export let pendingLabel = 'Submitting…';
  export let className = 'btn btn-sm';
  export let confirmTitle = 'Confirm action';
  export let confirmMessage = 'Continue?';
  export let disabled = false;
  export let disabledReason = '';
  export let submit: (
    kind: MutationKind,
    payload: Record<string, unknown>
  ) => Promise<SubmittedMutation | void>;
  export let onComplete: (submitted: SubmittedMutation) => Promise<void> | void = async () => {};

  let confirmOpen = false;
  let status: 'idle' | 'submitting' | 'success' = 'idle';
  let failedMutation: PendingMutationView | null = null;
  let activeMutationId: string | null = null;
  let successTimer: ReturnType<typeof setTimeout> | null = null;

  onDestroy(() => {
    if (successTimer) {
      clearTimeout(successTimer);
      successTimer = null;
    }
  });

  function mutationFailureKind(payload: MutationFailedEventPayload): string {
    if (payload.error_kind.kind === 'other') {
      return payload.error_kind.detail;
    }
    return payload.error_kind.kind;
  }

  async function waitForMutationOutcome(
    mutationId: string
  ): Promise<{ kind: 'reconciled' } | { kind: 'failed'; payload: MutationFailedEventPayload }> {
    return new Promise((resolve) => {
      let unlistenReconciled: (() => void) | null = null;
      let unlistenFailed: (() => void) | null = null;
      let settled = false;

      const finish = (result: { kind: 'reconciled' } | { kind: 'failed'; payload: MutationFailedEventPayload }) => {
        if (settled) {
          return;
        }
        settled = true;
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

  async function runSubmission(submitted: SubmittedMutation): Promise<void> {
    activeMutationId = submitted.mutation_id;
    const outcome = await waitForMutationOutcome(submitted.mutation_id);
    if (outcome.kind === 'reconciled') {
      failedMutation = null;
      status = 'success';
      await onComplete(submitted);
      if (successTimer) {
        clearTimeout(successTimer);
      }
      successTimer = setTimeout(() => {
        status = 'idle';
      }, 1200);
      return;
    }

    status = 'idle';
    failedMutation = {
      id: submitted.mutation_id,
      account_id: '',
      kind,
      optimism: 'none',
      target_type: 'pull_request',
      target_id: String(payload.target_id ?? payload.pr_id ?? ''),
      status: 'failed',
      retries: 0,
      created_at: Math.floor(Date.now() / 1000),
      updated_at: Math.floor(Date.now() / 1000),
      last_error: mutationFailureKind(outcome.payload),
      pending_overlay: null,
      requires_connection_confirmation: false
    };
  }

  async function confirmAndSubmit(): Promise<void> {
    status = 'submitting';
    failedMutation = null;
    confirmOpen = false;

    const submitted = await submit(kind, payload);
    if (!submitted) {
      status = 'idle';
      return;
    }
    await runSubmission(submitted);
  }

  async function retryFailed(mutationId: string): Promise<void> {
    status = 'submitting';
    failedMutation = null;
    await retryMutation(mutationId);
    await runSubmission({
      mutation_id: mutationId,
      deduped: false,
      requires_confirmation: false,
      optimism_level: 'none',
      projected_changes: []
    });
  }

  async function discardFailed(mutationId: string): Promise<void> {
    await discardMutation(mutationId);
    if (activeMutationId === mutationId) {
      activeMutationId = null;
    }
    failedMutation = null;
    status = 'idle';
  }
</script>

<button
  class={className}
  type="button"
  disabled={disabled || status === 'submitting'}
  title={disabled ? disabledReason : ''}
  on:click={() => (confirmOpen = true)}
>
  {#if status === 'submitting'}
    <span class="anim-rotate d-inline-block mr-1">⟳</span>{pendingLabel}
  {:else}
    {label}
  {/if}
</button>

{#if status === 'success'}
  <span class="Label Label--success ml-1">Confirmed</span>
{/if}

{#if failedMutation}
  <InlineMutationErrorBanner mutation={failedMutation} onRetry={retryFailed} onDiscard={discardFailed} />
{/if}

{#if confirmOpen}
  <div class="conflict-modal-backdrop" role="presentation">
    <div class="conflict-modal Box" role="dialog" aria-modal="true" aria-label={confirmTitle}>
      <div class="Box-header d-flex flex-items-center flex-justify-between">
        <h2 class="f5 m-0">{confirmTitle}</h2>
        <button class="btn btn-sm" type="button" on:click={() => (confirmOpen = false)}>Close</button>
      </div>
      <div class="Box-body">
        <p class="f6 mb-2">{confirmMessage}</p>
        <div class="d-flex gap-1">
          <button class="btn btn-primary" type="button" on:click={confirmAndSubmit}>Confirm</button>
          <button class="btn" type="button" on:click={() => (confirmOpen = false)}>Cancel</button>
        </div>
      </div>
    </div>
  </div>
{/if}

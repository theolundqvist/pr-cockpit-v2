import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  commands,
  type AccountLocator,
  type AuthAccount,
  type Draft,
  type InitInboxResponse,
  type IpcError,
  type ListDraftsInput,
  type MutationFailedEventPayload,
  type MutationHardConflictEventPayload,
  type MutationKind,
  type NetState,
  type PendingMutationView,
  type PrDetailSummary,
  type SubmittedMutation
} from '$lib/ipc/bindings';
import {
  MOCK_ACCOUNTS,
  MOCK_CHECKS,
  MOCK_FILES,
  MOCK_INIT_INBOX,
  MOCK_PR_DETAIL,
  MOCK_PR_METADATA,
  MOCK_SUBSCRIPTIONS,
  MOCK_THREADS,
  MOCK_TIMELINE,
  mockInboxForAccount,
  mockPatch,
  mockStatus
} from '$lib/mock/fixtures';

type CommandResult<T> = { status: 'ok'; data: T } | { status: 'error'; error: IpcError };
type EventCallback<T> = (payload: T) => Promise<void> | void;

let mockActiveAccountId = MOCK_INIT_INBOX.active_account_id ?? 'github.com:fixture-user';
let mockMutationSeq = 0;
let mockDraftSeq = 0;
let mockNetState: NetState = { state: 'online' };
const mockPendingMutations = new Map<string, PendingMutationView>();
const mockDrafts = new Map<string, Draft>();
const mockEventListeners = new Map<string, Set<EventCallback<unknown>>>();

const cautiousKinds = new Set<MutationKind>([
  'submit_review',
  'set_project',
  'convert_to_draft',
  'mark_ready_for_review',
  'update_branch'
]);
const noneKinds = new Set<MutationKind>([
  'merge',
  'enable_auto_merge',
  'disable_auto_merge',
  'close_pr',
  'reopen_pr'
]);

function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function unwrap<T>(result: Promise<CommandResult<T>>): Promise<T> {
  const resolved = await result;
  if (resolved.status === 'error') {
    throw new Error(`${resolved.error.code}: ${resolved.error.message}`);
  }
  return resolved.data;
}

export function toAccountId(locator: Pick<AccountLocator, 'host' | 'login'>): string {
  return `${locator.host}:${locator.login}`;
}

function nowEpoch(): number {
  return Math.floor(Date.now() / 1000);
}

function optimismForKind(kind: MutationKind): 'full' | 'cautious' | 'none' {
  if (noneKinds.has(kind)) {
    return 'none';
  }
  if (cautiousKinds.has(kind)) {
    return 'cautious';
  }
  return 'full';
}

function pendingTargetId(payload: Record<string, unknown>): string {
  const candidate =
    (payload.target_id as string | undefined) ??
    (payload.comment_id as string | undefined) ??
    (payload.thread_id as string | undefined) ??
    (payload.pr_id as string | undefined) ??
    (payload.file_path as string | undefined);
  return candidate ?? `target:${mockMutationSeq}`;
}

function pendingTargetType(kind: MutationKind): string {
  if (kind.includes('comment') || kind.includes('reaction')) {
    return 'comment';
  }
  if (kind.includes('thread')) {
    return 'thread';
  }
  if (kind.includes('label') || kind.includes('assignee') || kind.includes('review')) {
    return 'metadata';
  }
  if (kind.includes('file_viewed')) {
    return 'file';
  }
  return 'pull_request';
}

async function emitMockEvent<T>(eventName: string, payload: T): Promise<void> {
  const listeners = mockEventListeners.get(eventName);
  if (!listeners || listeners.size === 0) {
    return;
  }
  for (const listener of listeners) {
    await listener(payload);
  }
}

function registerMockListener<T>(eventName: string, callback: EventCallback<T>): UnlistenFn {
  const listeners = mockEventListeners.get(eventName) ?? new Set<EventCallback<unknown>>();
  listeners.add(callback as EventCallback<unknown>);
  mockEventListeners.set(eventName, listeners);
  return () => {
    const current = mockEventListeners.get(eventName);
    if (!current) {
      return;
    }
    current.delete(callback as EventCallback<unknown>);
    if (current.size === 0) {
      mockEventListeners.delete(eventName);
    }
  };
}

async function settleMockMutation(mutation: PendingMutationView): Promise<void> {
  if (mockNetState.state !== 'online') {
    return;
  }
  const current = mockPendingMutations.get(mutation.id);
  if (!current) {
    return;
  }
  await emitMockEvent('mutation:applied', { mutation_id: mutation.id });
  mockPendingMutations.delete(mutation.id);
  await emitMockEvent('mutation:reconciled', { mutation_id: mutation.id });
}

async function drainMockQueue(accountId: string): Promise<void> {
  const queue = [...mockPendingMutations.values()]
    .filter((entry) => entry.account_id === accountId && entry.status === 'pending')
    .sort((left, right) => left.created_at - right.created_at);
  for (const queued of queue) {
    await settleMockMutation(queued);
  }
}

export async function getInitInbox(): Promise<InitInboxResponse> {
  if (typeof window !== 'undefined' && window.__INBOX_SEED__) {
    return window.__INBOX_SEED__;
  }
  if (isTauriRuntime()) {
    return unwrap(commands.ipcInitInbox());
  }
  return {
    ...MOCK_INIT_INBOX,
    inbox: mockInboxForAccount(mockActiveAccountId),
    status: mockStatus(mockActiveAccountId)
  };
}

export async function listAccounts() {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcAccountsList());
  }
  const accounts = {
    ...MOCK_ACCOUNTS,
    accounts: MOCK_ACCOUNTS.accounts.map((account) => ({
      ...account,
      is_active: toAccountId(account) === mockActiveAccountId
    })),
    active:
      MOCK_ACCOUNTS.accounts.find((account) => toAccountId(account) === mockActiveAccountId) ?? null
  };
  return accounts;
}

export async function switchAccount(account: AccountLocator): Promise<AuthAccount> {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcAccountSwitch(account));
  }
  const matched = MOCK_ACCOUNTS.accounts.find(
    (candidate) => candidate.host === account.host && candidate.login === account.login
  );
  if (!matched) {
    throw new Error('AccountNotFound');
  }
  mockActiveAccountId = toAccountId(matched);
  return { ...matched, is_active: true };
}

export async function listInbox(accountId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcInboxList({ account_id: accountId }));
  }
  return mockInboxForAccount(accountId);
}

export async function listRepoSubscriptions(accountId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcRepoSubscriptions({ account_id: accountId }));
  }
  return MOCK_SUBSCRIPTIONS.filter((subscription) => subscription.account_id === accountId);
}

export async function getSystemStatus(accountId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcSystemStatus({ account_id: accountId }));
  }
  return mockStatus(accountId);
}

export async function getPrSummary(
  accountId: string,
  prId: string
): Promise<PrDetailSummary | null> {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrDetailSummary({ account_id: accountId, pr_id: prId }));
  }
  return prId === 'pr_1' ? MOCK_PR_DETAIL : null;
}

export async function getPrMetadata(accountId: string, prId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrMetadata({ account_id: accountId, pr_id: prId }));
  }
  return prId === 'pr_1'
    ? MOCK_PR_METADATA
    : { labels: [], assignees: [], requested_reviewers: [], projects: [], milestones: [] };
}

export async function getPrTimeline(accountId: string, prId: string) {
  if (isTauriRuntime()) {
    return unwrap(
      commands.ipcPrTimeline({ account_id: accountId, pr_id: prId, limit: null, offset: null })
    );
  }
  return prId === 'pr_1' ? MOCK_TIMELINE : { items: [], next_offset: null };
}

export async function getReviewThreads(accountId: string, prId: string) {
  if (isTauriRuntime()) {
    return unwrap(
      commands.ipcPrReviewThreads({
        account_id: accountId,
        pr_id: prId,
        limit: null,
        offset: null
      })
    );
  }
  return prId === 'pr_1' ? MOCK_THREADS : { threads: [], next_offset: null };
}

export async function getCheckSummary(accountId: string, prId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrCheckSummary({ account_id: accountId, pr_id: prId }));
  }
  return prId === 'pr_1'
    ? MOCK_CHECKS
    : { total_runs: 0, successful_runs: 0, failed_runs: 0, pending_runs: 0, runs: [] };
}

export async function getPrFiles(accountId: string, prId: string, headSha: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrFiles({ account_id: accountId, pr_id: prId, head_sha: headSha }));
  }
  return prId === 'pr_1' ? MOCK_FILES : { files: [], tree: [] };
}

export async function getPrPatch(accountId: string, prId: string, headSha: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrPatch({ account_id: accountId, pr_id: prId, head_sha: headSha }));
  }
  return prId === 'pr_1' ? mockPatch() : { patch_blob_sha: null, patch: null };
}

export async function getPrFileBlob(
  accountId: string,
  prId: string,
  headSha: string,
  path: string,
  side: string | null = null
) {
  if (isTauriRuntime()) {
    return unwrap(
      commands.getPrFileBlob({
        account_id: accountId,
        pr_id: prId,
        head_sha: headSha,
        path,
        side
      })
    );
  }
  return {
    sha256: `mock-${path}`,
    local_path: path.endsWith('.png')
      ? 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVQImWP4DwQACfsD/aeKoiUAAAAASUVORK5CYII='
      : '',
    mime_type: path.endsWith('.png') ? 'image/png' : 'application/octet-stream'
  };
}

export async function renderCommentHtml(body: string, repo: string | null) {
  if (isTauriRuntime()) {
    return unwrap(commands.renderPreview({ body, ctx: { repo } }));
  }
  return {
    html: `<p>${body.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</p>`,
    cache_hit: false,
    content_hash: 'mock',
    cache_key: 'mock',
    renderer_version: 'mock'
  };
}

export async function renderPreview(body: string, repo: string | null) {
  return renderCommentHtml(body, repo);
}

export async function submitMutation(accountId: string, kind: MutationKind, payloadJson: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.submitMutation(accountId, kind, payloadJson));
  }

  const parsed = JSON.parse(payloadJson) as Record<string, unknown>;
  const mutationId = `mock-mutation-${++mockMutationSeq}`;
  const createdAt = nowEpoch();
  const optimism = optimismForKind(kind);
  const mutation: PendingMutationView = {
    id: mutationId,
    account_id: accountId,
    kind,
    optimism,
    target_type: pendingTargetType(kind),
    target_id: pendingTargetId(parsed),
    status: 'pending',
    retries: 0,
    created_at: createdAt,
    updated_at: createdAt,
    last_error: null,
    pending_overlay: { mutation_id: mutationId, kind: optimism },
    requires_connection_confirmation: mockNetState.state === 'offline' && optimism === 'none'
  };
  mockPendingMutations.set(mutationId, mutation);
  await emitMockEvent('mutation:submitted', { mutation });
  if (mockNetState.state === 'online') {
    await settleMockMutation(mutation);
  }
  return {
    mutation_id: mutationId,
    deduped: false,
    requires_confirmation: mutation.requires_connection_confirmation,
    optimism_level: optimism,
    projected_changes: [kind, ...Object.keys(parsed).slice(0, 2)]
  };
}

export async function submitReviewComment(
  accountId: string,
  payload: Record<string, unknown>
): Promise<SubmittedMutation> {
  if (isTauriRuntime()) {
    return unwrap(
      commands.submitReviewComment({
        account_id: accountId,
        payload_json: JSON.stringify(payload)
      })
    );
  }
  return submitMutation(accountId, 'add_review_comment', JSON.stringify(payload));
}

export async function listPendingMutations(
  accountId: string,
  includePending: boolean | null = null
): Promise<PendingMutationView[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listPendingMutations(accountId, includePending));
  }
  return [...mockPendingMutations.values()].filter((row) => {
    if (row.account_id !== accountId) {
      return false;
    }
    if (includePending === false) {
      return row.status === 'failed';
    }
    return true;
  });
}

export async function retryMutation(mutationId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.retryMutation(mutationId));
    return;
  }
  const mutation = mockPendingMutations.get(mutationId);
  if (!mutation) {
    return;
  }
  mutation.retries += 1;
  mutation.updated_at = nowEpoch();
  mutation.status = 'pending';
  mutation.last_error = null;
  mockPendingMutations.set(mutationId, mutation);
  if (mockNetState.state === 'offline') {
    return;
  }
  await settleMockMutation(mutation);
}

export async function discardMutation(mutationId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.discardMutation(mutationId));
    return;
  }
  if (!mockPendingMutations.has(mutationId)) {
    return;
  }
  mockPendingMutations.delete(mutationId);
  await emitMockEvent('mutation:rolled-back', { mutation_id: mutationId });
}

export async function listDrafts(input: ListDraftsInput): Promise<Draft[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listDrafts(input));
  }
  return [...mockDrafts.values()].filter((draft) => {
    if (draft.account_id !== input.account_id) {
      return false;
    }
    if (input.target_type && draft.target_type !== input.target_type) {
      return false;
    }
    if (input.target_id && draft.target_id !== input.target_id) {
      return false;
    }
    return true;
  });
}

export async function saveDraft(input: {
  id?: string | null;
  account_id: string;
  target_type: string;
  target_id: string;
  body: string;
}): Promise<Draft> {
  if (isTauriRuntime()) {
    return unwrap(
      commands.saveDraft({
        id: input.id ?? '',
        account_id: input.account_id,
        target_type: input.target_type,
        target_id: input.target_id,
        body: input.body
      })
    );
  }
  const id = input.id && input.id.length > 0 ? input.id : `draft-${++mockDraftSeq}`;
  const existing = mockDrafts.get(id);
  const timestamp = nowEpoch();
  const draft: Draft = {
    id,
    account_id: input.account_id,
    target_type: input.target_type,
    target_id: input.target_id,
    body: input.body,
    created_at: existing?.created_at ?? timestamp,
    updated_at: timestamp
  };
  mockDrafts.set(id, draft);
  return draft;
}

export async function deleteDraft(draftId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.deleteDraft(draftId));
    return;
  }
  mockDrafts.delete(draftId);
}

export async function setMockNetworkState(accountId: string, state: NetState): Promise<void> {
  mockNetState = state;
  await emitMockEvent(`network:${accountId} changed`, { account_id: accountId, state });
  if (state.state === 'online') {
    await drainMockQueue(accountId);
  }
}

export async function failMockMutation(
  mutationId: string,
  reason: MutationFailedEventPayload['error_kind']['kind'] = 'server'
): Promise<void> {
  const mutation = mockPendingMutations.get(mutationId);
  if (!mutation) {
    return;
  }
  mutation.status = 'failed';
  mutation.updated_at = nowEpoch();
  mutation.last_error = reason;
  mockPendingMutations.set(mutationId, mutation);
  await emitMockEvent('mutation:failed', {
    mutation_id: mutationId,
    error_kind: reason === 'other' ? { kind: 'other', detail: 'mock failure' } : { kind: reason },
    retryable: reason !== 'conflict',
    hard_conflict: null
  } satisfies MutationFailedEventPayload);
}

export async function emitMockHardConflict(
  payload: MutationHardConflictEventPayload
): Promise<void> {
  await emitMockEvent('mutation:hard-conflict', payload);
}

export async function listenEventPayload<T>(
  eventName: string,
  callback: EventCallback<T>
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return registerMockListener(eventName, callback);
  }
  return listen(eventName, async (event) => callback(event.payload as T));
}

export async function listenEvent(
  eventName: string,
  callback: () => Promise<void> | void
): Promise<UnlistenFn> {
  return listenEventPayload(eventName, () => callback());
}

declare global {
  interface Window {
    __INBOX_SEED__?: InitInboxResponse | null;
    __M2_DEBUG__?: {
      setOffline: () => Promise<void>;
      setOnline: () => Promise<void>;
      pendingCount: () => number;
    };
  }
}

if (typeof window !== 'undefined' && !window.__M2_DEBUG__) {
  window.__M2_DEBUG__ = {
    setOffline: async () =>
      setMockNetworkState(mockActiveAccountId, {
        state: 'offline',
        error_kind: { kind: 'network' }
      }),
    setOnline: async () => setMockNetworkState(mockActiveAccountId, { state: 'online' }),
    pendingCount: () => mockPendingMutations.size
  };
}

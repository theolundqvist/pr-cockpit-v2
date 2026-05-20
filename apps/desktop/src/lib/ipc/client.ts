import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openExternal } from '@tauri-apps/plugin-shell';

import {
  commands,
  type AccountLocator,
  type AuthAccount,
  type CheckAnnotationView,
  type Draft,
  type InitInboxResponse,
  type IpcError,
  type ListDraftsInput,
  type NotificationEventPayload,
  type NotificationEventRow,
  type NotificationRule,
  type MutationFailedEventPayload,
  type MutationHardConflictEventPayload,
  type MutationKind,
  type NetState,
  type RateLimitBucket,
  type PendingMutationView,
  type PrCheckSummary,
  type PrDetailSummary,
  type PrPushView,
  type SavedReply,
  type StreamHandle,
  type RangeDiff,
  type SuggestionBlock,
  type SubmittedMutation,
  type SystemStatusResponse,
  type ImageUploadResult,
  type CleanupError,
  type WorktreeView,
  type CleanupOutcome,
  type RediscoverSummary,
  type GraphiteIntegrationStatus,
  type RelaySettingsSnapshot,
  type MergeMethod,
  type StackGraph,
  type StackOperationView
} from '$lib/ipc/bindings';
import {
  MOCK_ACCOUNTS,
  MOCK_CHECK_ANNOTATIONS,
  MOCK_CLEANUP_OUTCOME,
  MOCK_CHECKS,
  MOCK_FILES,
  MOCK_INIT_INBOX,
  MOCK_REDISCOVER_SUMMARY,
  MOCK_PR_DETAIL,
  MOCK_PR_METADATA,
  MOCK_PR_PUSHES,
  MOCK_STACKS_BY_SCOPE,
  MOCK_SUBSCRIPTIONS,
  MOCK_THREADS,
  MOCK_TIMELINE,
  MOCK_WORKTREE_ROOTS,
  MOCK_WORKTREES,
  mockRangeDiff,
  mockInboxForAccount,
  mockPatch,
  mockStatus
} from '$lib/mock/fixtures';

type CommandResult<T> = { status: 'ok'; data: T } | { status: 'error'; error: IpcError };
type EventCallback<T> = (payload: T) => Promise<void> | void;

let mockActiveAccountId = MOCK_INIT_INBOX.active_account_id ?? 'github.com:fixture-user';
let mockAccountsState: AuthAccount[] = MOCK_ACCOUNTS.accounts.map((account) => ({ ...account }));
let mockMutationSeq = 0;
let mockDraftSeq = 0;
let mockNetState: NetState = { state: 'online' };
let mockPrDetail: PrDetailSummary = { ...MOCK_PR_DETAIL };
let mockChecksState: PrCheckSummary = {
  ...MOCK_CHECKS,
  runs: MOCK_CHECKS.runs.map((run) => ({ ...run }))
};
let mockAutoSettleMutations = true;
const mockPendingMutations = new Map<string, PendingMutationView>();
const mockMutationInputs = new Map<
  string,
  { account_id: string; kind: MutationKind; payload: Record<string, unknown> }
>();
const mockMutationCalls: Array<{
  mutation_id: string;
  account_id: string;
  kind: MutationKind;
  payload: Record<string, unknown>;
}> = [];
const mockDrafts = new Map<string, Draft>();
let mockSavedReplySeq = 0;
const mockSavedReplies = new Map<string, SavedReply[]>();
type MockImageUploadStep =
  | { kind: 'success'; result: ImageUploadResult }
  | { kind: 'error'; message: string };
const mockImageUploadQueue: MockImageUploadStep[] = [];
const mockImageUploadInvocations: Array<{ account_id: string; mime: string; size_bytes: number }> =
  [];
const mockExternalOpenInvocations: string[] = [];
const mockEventListeners = new Map<string, Set<EventCallback<unknown>>>();
let mockWorktreeRoots = [...MOCK_WORKTREE_ROOTS];
let mockWorktrees = [...MOCK_WORKTREES];
const mockStacksByScope = new Map<string, StackGraph[]>(
  Object.entries(MOCK_STACKS_BY_SCOPE).map(([scope, stacks]) => [
    scope,
    stacks.map((stack) => ({
      ...stack,
      nodes: stack.nodes.map((node) => ({ ...node, blocked_by: [...node.blocked_by] })),
      edges: stack.edges.map((edge) => ({ ...edge })),
      warning: stack.warning
        ? { reason: stack.warning.reason, diamond_pr_ids: [...stack.warning.diamond_pr_ids] }
        : null
    }))
  ])
);
const mockStackOps = new Map<string, StackOperationView>();
let mockStackOpSeq = 0;
const mockStackInvocations: Array<{ command: string; payload: Record<string, unknown> }> = [];
const GRAPHITE_STATUS_STORAGE_KEY = '__mock_graphite_status__';
const RELAY_SETTINGS_STORAGE_KEY = '__mock_relay_settings__';
let mockGraphiteStatus: GraphiteIntegrationStatus = readStoredGraphiteStatus();
let mockRelaySettings: RelaySettingsSnapshot = readStoredRelaySettings();
const mockAppliedSuggestionCommentIds = new Set<string>();
const mockSuggestionBlocks: SuggestionBlock[] = [
  {
    id: 'comment_3:0',
    pr_id: 'pr_1',
    comment_id: 'comment_3',
    path: 'src/generated/huge_fixture.rs',
    body: 'let x = 1;',
    start_line: 120,
    end_line: 120,
    side: 'RIGHT',
    original_commit_sha: MOCK_PR_DETAIL.head_sha,
    suggestion_author_login: 'fixture-user-03',
    is_outdated: false
  },
  {
    id: 'comment_6:0',
    pr_id: 'pr_1',
    comment_id: 'comment_6',
    path: 'src/generated/huge_fixture.rs',
    body: 'let y = 2;',
    start_line: 128,
    end_line: 128,
    side: 'RIGHT',
    original_commit_sha: MOCK_PR_DETAIL.head_sha,
    suggestion_author_login: 'fixture-user-06',
    is_outdated: false
  },
  {
    id: 'comment_9:0',
    pr_id: 'pr_1',
    comment_id: 'comment_9',
    path: 'src/generated/huge_fixture.rs',
    body: 'let z = 3;',
    start_line: 136,
    end_line: 136,
    side: 'RIGHT',
    original_commit_sha: MOCK_PR_DETAIL.head_sha,
    suggestion_author_login: 'fixture-user-09',
    is_outdated: false
  }
];
const mockCheckAnnotations: CheckAnnotationView[] = MOCK_CHECK_ANNOTATIONS.map((annotation) => ({
  ...annotation
}));
const mockNotificationRules = new Map<string, NotificationRule[]>();
const mockNotificationEvents = new Map<string, NotificationEventRow[]>();
const mockNotificationSettings = new Map<
  string,
  {
    quiet_hours_json: string | null;
    focus_mode: boolean;
    allow: string[];
    deny: string[];
  }
>();
const mockNotificationInvocations: Array<{ command: string; payload: unknown }> = [];
const mockMutationInvocations: Array<{
  account_id: string;
  kind: MutationKind;
  payload: Record<string, unknown>;
}> = [];
const mockRateLimitOverrides = new Map<
  string,
  Map<string, { remaining: number; limit_total: number; used?: number; reset_at: number }>
>();
const mockEndpointTestInvocations: Array<{
  host: string;
  api_url: string;
  graphql_url: string;
}> = [];
let mockRangeDiffModeOverride: 'local' | 'rest' | null = null;
const RANGE_DIFF_MODE_STORAGE_KEY = '__range_diff_mode_override__';

function readStoredRangeDiffMode(): 'local' | 'rest' | null {
  if (typeof window === 'undefined') {
    return null;
  }
  try {
    const raw = window.localStorage.getItem(RANGE_DIFF_MODE_STORAGE_KEY);
    if (raw === 'local' || raw === 'rest') {
      return raw;
    }
  } catch {
    return null;
  }
  return null;
}

function writeStoredRangeDiffMode(mode: 'local' | 'rest' | null): void {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    if (mode) {
      window.localStorage.setItem(RANGE_DIFF_MODE_STORAGE_KEY, mode);
    } else {
      window.localStorage.removeItem(RANGE_DIFF_MODE_STORAGE_KEY);
    }
  } catch {
    // noop in constrained browser environments
  }
}

function readStoredGraphiteStatus(): GraphiteIntegrationStatus {
  if (typeof window === 'undefined') {
    return {
      detected_version: null,
      enabled: false
    };
  }
  try {
    const raw = window.localStorage.getItem(GRAPHITE_STATUS_STORAGE_KEY);
    if (!raw) {
      return {
        detected_version: null,
        enabled: false
      };
    }
    const parsed = JSON.parse(raw) as GraphiteIntegrationStatus;
    return {
      detected_version: parsed.detected_version ? { ...parsed.detected_version } : null,
      enabled: Boolean(parsed.enabled)
    };
  } catch {
    return {
      detected_version: null,
      enabled: false
    };
  }
}

function writeStoredGraphiteStatus(status: GraphiteIntegrationStatus): void {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    window.localStorage.setItem(
      GRAPHITE_STATUS_STORAGE_KEY,
      JSON.stringify({
        detected_version: status.detected_version,
        enabled: status.enabled
      })
    );
  } catch {
    // noop in constrained browser environments
  }
}

function readStoredRelaySettings(): RelaySettingsSnapshot {
  if (typeof window === 'undefined') {
    return { enabled: false, has_forward_secret: false, local_url: null };
  }
  try {
    const raw = window.localStorage.getItem(RELAY_SETTINGS_STORAGE_KEY);
    if (!raw) {
      return { enabled: false, has_forward_secret: false, local_url: null };
    }
    const parsed = JSON.parse(raw) as RelaySettingsSnapshot;
    return {
      enabled: Boolean(parsed.enabled),
      has_forward_secret: Boolean(parsed.has_forward_secret),
      local_url: parsed.local_url ?? null
    };
  } catch {
    return { enabled: false, has_forward_secret: false, local_url: null };
  }
}

function writeStoredRelaySettings(settings: RelaySettingsSnapshot): void {
  if (typeof window === 'undefined') {
    return;
  }
  try {
    window.localStorage.setItem(RELAY_SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // noop in constrained browser environments
  }
}

function recomputeMockRelayUrl(): string | null {
  if (!mockRelaySettings.enabled || !mockRelaySettings.has_forward_secret) {
    return null;
  }
  return 'http://127.0.0.1:49811/webhook';
}

const cautiousKinds = new Set<MutationKind>([
  'submit_review',
  'set_project',
  'convert_to_draft',
  'mark_ready_for_review',
  'update_branch',
  'rerun_check_run',
  'rerun_check_suite'
]);
const noneKinds = new Set<MutationKind>([
  'merge',
  'delete_head_ref',
  'enqueue_merge_queue',
  'dequeue_merge_queue',
  'reorder_merge_queue',
  'enable_auto_merge',
  'disable_auto_merge',
  'close_pr',
  'reopen_pr',
  'apply_suggestion',
  'apply_suggestion_batch'
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

async function unwrapTypedError<T, E>(
  result: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: E }>
): Promise<T> {
  const resolved = await result;
  if (resolved.status === 'error') {
    throw new Error(`IPC typed error: ${JSON.stringify(resolved.error)}`);
  }
  return resolved.data;
}

export function toAccountId(locator: Pick<AccountLocator, 'host' | 'login'>): string {
  return `${locator.host}:${locator.login}`;
}

function nowEpoch(): number {
  return Math.floor(Date.now() / 1000);
}

const notificationKinds = [
  'review_requested',
  'changes_requested',
  'approved',
  'mention',
  'ci_fail',
  'ci_recover',
  'merge_conflict',
  'mutation_failure'
] as const;

function ensureMockNotificationRules(accountId: string): NotificationRule[] {
  const existing = mockNotificationRules.get(accountId);
  if (existing) {
    return existing;
  }
  const generated = notificationKinds.map((kind) => ({
    id: `${accountId}:${kind}`,
    account_id: accountId,
    kind,
    enabled: true,
    config_json: '{}',
    updated_at: nowEpoch()
  }));
  mockNotificationRules.set(accountId, generated);
  return generated;
}

function ensureMockNotificationSettings(accountId: string): {
  quiet_hours_json: string | null;
  focus_mode: boolean;
  allow: string[];
  deny: string[];
} {
  const existing = mockNotificationSettings.get(accountId);
  if (existing) {
    return existing;
  }
  const generated = {
    quiet_hours_json: null,
    focus_mode: false,
    allow: [] as string[],
    deny: [] as string[]
  };
  mockNotificationSettings.set(accountId, generated);
  return generated;
}

function ensureMockSavedReplies(accountId: string): SavedReply[] {
  const existing = mockSavedReplies.get(accountId);
  if (existing) {
    return existing;
  }
  const seeded =
    accountId === toAccountId(MOCK_ACCOUNTS.accounts[0]!)
      ? ([
          {
            id: ++mockSavedReplySeq,
            account_id: accountId,
            name: 'Friendly follow-up',
            body: 'Thanks for the update! Could you add one regression test for this path?',
            sort_order: 0,
            created_at: nowEpoch(),
            updated_at: nowEpoch()
          },
          {
            id: ++mockSavedReplySeq,
            account_id: accountId,
            name: 'Needs clarification',
            body: 'Can you clarify the behavior change in the PR description?',
            sort_order: 1,
            created_at: nowEpoch(),
            updated_at: nowEpoch()
          }
        ] satisfies SavedReply[])
      : [];
  mockSavedReplies.set(accountId, seeded);
  return seeded;
}

function sortSavedReplies(rows: SavedReply[]): SavedReply[] {
  return [...rows].sort(
    (left, right) => left.sort_order - right.sort_order || left.name.localeCompare(right.name)
  );
}

function mockImageHash(bytes: number[]): string {
  let hash = 0;
  for (const value of bytes) {
    hash = (hash * 31 + value) >>> 0;
  }
  return `mock-${hash.toString(16).padStart(8, '0')}`;
}

function pushMockNotificationInvocation(command: string, payload: unknown): void {
  mockNotificationInvocations.push({ command, payload });
}

function eventRowFromPayload(payload: NotificationEventPayload): NotificationEventRow {
  return {
    id: payload.event_id,
    account_id: payload.account_id,
    repo_id: payload.repo_id,
    pr_id: payload.pr_id,
    event_type: payload.event_type,
    actor_id: payload.actor_id,
    server_event_id: payload.server_event_id,
    title: payload.title,
    body: payload.body,
    fired_at: payload.fired_at,
    deduped: payload.deduped,
    seen: false
  };
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

function stackScopeKey(accountId: string, repoId: string): string {
  return `${accountId}:${repoId}`;
}

function cloneStackGraphs(stacks: StackGraph[]): StackGraph[] {
  return stacks.map((stack) => ({
    ...stack,
    nodes: stack.nodes.map((node) => ({ ...node, blocked_by: [...node.blocked_by] })),
    edges: stack.edges.map((edge) => ({ ...edge })),
    warning: stack.warning
      ? { reason: stack.warning.reason, diamond_pr_ids: [...stack.warning.diamond_pr_ids] }
      : null
  }));
}

function cloneStackOperation(operation: StackOperationView): StackOperationView {
  return {
    ...operation,
    conflict_files: [...operation.conflict_files]
  };
}

function pushMockStackInvocation(command: string, payload: Record<string, unknown>): void {
  mockStackInvocations.push({ command, payload });
}

function latestMockStackOpId(): string | null {
  let latest: StackOperationView | null = null;
  for (const operation of mockStackOps.values()) {
    if (!latest || operation.updated_at >= latest.updated_at) {
      latest = operation;
    }
  }
  return latest?.id ?? null;
}

async function emitMockStacksChanged(
  accountId: string,
  repoId: string,
  stacks: StackGraph[]
): Promise<void> {
  const payload = {
    account_id: accountId,
    repo_id: repoId,
    stacks: cloneStackGraphs(stacks)
  };
  await emitMockEvent(`stacks:${accountId}:${repoId} changed`, payload);
  await emitMockEvent('stacks:<account_id>:<repo_id> changed', payload);
}

async function emitMockStackOperation(operation: StackOperationView): Promise<void> {
  mockStackOps.set(operation.id, cloneStackOperation(operation));
  const payload = { operation: cloneStackOperation(operation) };
  await emitMockEvent(`stack_op:${operation.id}`, payload);
  await emitMockEvent('stack_op:<op_id>', payload);
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
  const mutationInput = mockMutationInputs.get(mutation.id);
  if (mutationInput?.kind === 'apply_suggestion_batch') {
    const prId = (mutationInput.payload.pr_id as string | undefined) ?? 'unknown';
    for (const step of ['opened', 'assertions_ok', 'patched', 'committed', 'pushed']) {
      await emitMockEvent(`worktree_write:${prId}:${step}`, { pr_id: prId, step });
    }
  }
  if (mutationInput) {
    applyMockMutationSideEffects(mutationInput.kind, mutationInput.payload);
  }
  mockPendingMutations.delete(mutation.id);
  mockMutationInputs.delete(mutation.id);
  await emitMockEvent('mutation:reconciled', { mutation_id: mutation.id });
  const prId =
    (mutationInput?.payload.pr_id as string | undefined) ??
    (mutation.target_type === 'pull_request' ? mutation.target_id : undefined);
  if (prId) {
    await emitMockEvent(`pr:${prId} changed`, { pr_id: prId });
  }
}

async function drainMockQueue(accountId: string): Promise<void> {
  if (!mockAutoSettleMutations) {
    return;
  }
  const queue = [...mockPendingMutations.values()]
    .filter((entry) => entry.account_id === accountId && entry.status === 'pending')
    .sort((left, right) => left.created_at - right.created_at);
  for (const queued of queue) {
    await settleMockMutation(queued);
  }
}

function optionalString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function applyMockMutationSideEffects(kind: MutationKind, payload: Record<string, unknown>): void {
  const prId = payload.pr_id as string | undefined;
  if (!prId || prId !== mockPrDetail.pr_id) {
    return;
  }
  switch (kind) {
    case 'merge':
      mockPrDetail = {
        ...mockPrDetail,
        state: 'closed',
        mergeable_state: 'merged',
        merge_state_status: 'merged'
      };
      break;
    case 'delete_head_ref':
      mockPrDetail = {
        ...mockPrDetail,
        head_ref_state: 'DELETED'
      };
      break;
    case 'enable_auto_merge':
      mockPrDetail = {
        ...mockPrDetail,
        auto_merge_enabled: true,
        auto_merge_method: optionalString(payload.merge_method) ?? 'SQUASH',
        auto_merge_commit_headline: optionalString(payload.commit_headline),
        auto_merge_commit_body: optionalString(payload.commit_body),
        auto_merge_enabled_by_login: mockPrDetail.auto_merge_enabled_by_login ?? 'fixture-user'
      };
      break;
    case 'disable_auto_merge':
      mockPrDetail = {
        ...mockPrDetail,
        auto_merge_enabled: false,
        auto_merge_method: null,
        auto_merge_commit_headline: null,
        auto_merge_commit_body: null,
        auto_merge_enabled_by_login: null
      };
      break;
    case 'enqueue_merge_queue':
      mockPrDetail = {
        ...mockPrDetail,
        merge_queue_entry_id: optionalString(payload.merge_queue_entry_id) ?? `mqe-${Date.now()}`,
        merge_queue_entry_position: 1,
        merge_queue_entry_state: 'QUEUED',
        merge_queue_entry_estimated_ms: 600000
      };
      break;
    case 'dequeue_merge_queue':
      mockPrDetail = {
        ...mockPrDetail,
        merge_queue_entry_id: null,
        merge_queue_entry_position: null,
        merge_queue_entry_state: null,
        merge_queue_entry_estimated_ms: null
      };
      break;
    case 'reorder_merge_queue': {
      const mode = optionalString(payload.mode) ?? 'TOP';
      mockPrDetail = {
        ...mockPrDetail,
        merge_queue_entry_position: mode === 'TOP' ? 1 : 8
      };
      break;
    }
    case 'update_branch':
      mockPrDetail = {
        ...mockPrDetail,
        merge_state_status: 'clean'
      };
      break;
    case 'apply_suggestion': {
      const commentId = optionalString(payload.review_comment_id);
      if (commentId) {
        mockAppliedSuggestionCommentIds.add(commentId);
      }
      break;
    }
    case 'apply_suggestion_batch': {
      const suggestionIds = (payload.suggestion_ids as string[] | undefined) ?? [];
      for (const suggestionId of suggestionIds) {
        const commentId = suggestionId.split(':')[0];
        if (commentId) {
          mockAppliedSuggestionCommentIds.add(commentId);
        }
      }
      mockPrDetail = {
        ...mockPrDetail,
        head_sha: `${mockPrDetail.head_sha.slice(0, 30)}${Date.now().toString().slice(-10)}`
      };
      break;
    }
    case 'rerun_check_run': {
      const runId = optionalString(payload.check_run_id) ?? optionalString(payload.target_id);
      if (!runId) {
        break;
      }
      mockChecksState = {
        ...mockChecksState,
        runs: mockChecksState.runs.map((run) =>
          run.id === runId
            ? {
                ...run,
                status: 'queued',
                conclusion: null,
                completed_at: null
              }
            : run
        ),
        pending_runs: Math.max(mockChecksState.pending_runs + 1, 1)
      };
      break;
    }
    case 'rerun_check_suite': {
      const suiteId = optionalString(payload.check_suite_id) ?? optionalString(payload.target_id);
      if (!suiteId) {
        break;
      }
      mockChecksState = {
        ...mockChecksState,
        runs: mockChecksState.runs.map((run) =>
          run.check_suite_id === suiteId
            ? {
                ...run,
                status: 'queued',
                conclusion: null,
                completed_at: null
              }
            : run
        ),
        pending_runs: Math.max(mockChecksState.pending_runs + 1, 1)
      };
      break;
    }
    default:
      break;
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
    accounts: mockAccountsState.map((account) => ({
      ...account,
      is_active: toAccountId(account) === mockActiveAccountId
    })),
    active:
      mockAccountsState.find((account) => toAccountId(account) === mockActiveAccountId) ?? null
  };
  return accounts;
}

export async function switchAccount(account: AccountLocator): Promise<AuthAccount> {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcAccountSwitch(account));
  }
  const matched = mockAccountsState.find(
    (candidate) => candidate.host === account.host && candidate.login === account.login
  );
  if (!matched) {
    throw new Error('AccountNotFound');
  }
  mockActiveAccountId = toAccountId(matched);
  return { ...matched, is_active: true };
}

export async function savePatToken(host: string, token: string): Promise<AuthAccount> {
  if (isTauriRuntime()) {
    return unwrap(commands.authSavePatToken({ host, token }));
  }
  const normalizedHost = host.trim().toLowerCase();
  if (!token.trim()) {
    throw new Error('InvalidToken');
  }
  const fallbackLogin =
    normalizedHost === 'github.enterprise.test' ? 'octo-enterprise' : 'fixture-user';
  const existing =
    mockAccountsState.find((account) => account.host === normalizedHost) ??
    ({
      host: normalizedHost,
      login: fallbackLogin,
      api_base_url:
        normalizedHost === 'github.com'
          ? 'https://api.github.com'
          : `https://${normalizedHost}/api/v3`,
      graphql_url:
        normalizedHost === 'github.com'
          ? 'https://api.github.com/graphql'
          : `https://${normalizedHost}/api/graphql`,
      token_kind: 'pat',
      scopes: ['repo'],
      created_at: nowEpoch(),
      updated_at: nowEpoch(),
      is_active: false
    } satisfies AuthAccount);
  if (!mockAccountsState.some((account) => toAccountId(account) === toAccountId(existing))) {
    mockAccountsState = [...mockAccountsState, existing];
  }
  return existing;
}

export async function testEndpoints(host: string) {
  const normalizedHost = host.trim().toLowerCase();
  const apiUrl =
    normalizedHost === 'github.com' ? 'https://api.github.com' : `https://${normalizedHost}/api/v3`;
  const graphqlUrl =
    normalizedHost === 'github.com'
      ? 'https://api.github.com/graphql'
      : `https://${normalizedHost}/api/graphql`;
  mockEndpointTestInvocations.push({
    host: normalizedHost,
    api_url: apiUrl,
    graphql_url: graphqlUrl
  });
  if (isTauriRuntime()) {
    return unwrap(commands.authTestEndpoints(host));
  }
  return {
    api_ok: true,
    graphql_ok: true,
    api_latency_ms: normalizedHost === 'github.com' ? 35 : 47,
    graphql_latency_ms: normalizedHost === 'github.com' ? 52 : 63
  };
}

export async function listInbox(accountId: string | null) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcInboxList({ account_id_filter: accountId }));
  }
  return mockInboxForAccount(accountId);
}

export async function listRepoSubscriptions(accountId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcRepoSubscriptions({ account_id: accountId }));
  }
  return MOCK_SUBSCRIPTIONS.filter((subscription) => subscription.account_id === accountId);
}

function applyMockRateLimitOverrides(status: SystemStatusResponse): SystemStatusResponse {
  const nextRateLimits: RateLimitBucket[] = status.rate_limits.map((bucket) => {
    const override = mockRateLimitOverrides.get(bucket.account_id)?.get(bucket.resource);
    if (!override) {
      return bucket;
    }
    return {
      ...bucket,
      remaining: override.remaining,
      used: override.used ?? Math.max(0, override.limit_total - override.remaining),
      limit_total: override.limit_total,
      reset_at: override.reset_at
    };
  });
  return { ...status, rate_limits: nextRateLimits };
}

function setMockRateLimit(
  accountId: string,
  resource: string,
  remaining: number,
  limitTotal: number,
  resetAt: number = nowEpoch() + 3600
): void {
  const byResource = mockRateLimitOverrides.get(accountId) ?? new Map();
  byResource.set(resource, {
    remaining,
    limit_total: limitTotal,
    used: Math.max(0, limitTotal - remaining),
    reset_at: resetAt
  });
  mockRateLimitOverrides.set(accountId, byResource);
}

export async function getSystemStatus(accountId: string | null) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcSystemStatus({ account_id_filter: accountId }));
  }
  return applyMockRateLimitOverrides(mockStatus(accountId));
}

export async function getPrSummary(
  accountId: string,
  prId: string
): Promise<PrDetailSummary | null> {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrDetailSummary({ account_id: accountId, pr_id: prId }));
  }
  return prId === 'pr_1' ? { ...mockPrDetail } : null;
}

export async function listPrPushes(prId: string): Promise<PrPushView[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listPrPushes(prId));
  }
  return MOCK_PR_PUSHES[prId] ?? [];
}

export async function computeRangeDiff(
  prId: string,
  baseSha: string,
  oldHeadSha: string,
  newHeadSha: string
): Promise<RangeDiff> {
  if (isTauriRuntime()) {
    return unwrap(commands.computeRangeDiff(prId, baseSha, oldHeadSha, newHeadSha));
  }
  const mappedWorktreeExists = mockWorktrees.some(
    (worktree) =>
      worktree.mapped_pr_id === prId &&
      ((worktree.mapping_confidence ?? 0) >= 0.75 || worktree.manual_override_pr_id === prId)
  );
  const selectedMode =
    mockRangeDiffModeOverride === 'local'
      ? 'LocalGit'
      : mockRangeDiffModeOverride === 'rest'
        ? 'RestCompare'
        : mappedWorktreeExists
          ? 'LocalGit'
          : 'RestCompare';
  return mockRangeDiff(selectedMode, baseSha, oldHeadSha, newHeadSha);
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

export async function getSuggestionBlocks(
  accountId: string,
  prId: string
): Promise<SuggestionBlock[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listSuggestionBlocks({ account_id: accountId, pr_id: prId }));
  }
  if (prId !== 'pr_1') {
    return [];
  }
  return mockSuggestionBlocks
    .filter((suggestion) => suggestion.pr_id === prId)
    .filter((suggestion) => !mockAppliedSuggestionCommentIds.has(suggestion.comment_id));
}

export async function getCheckSummary(accountId: string, prId: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcPrCheckSummary({ account_id: accountId, pr_id: prId }));
  }
  return prId === 'pr_1'
    ? mockChecksState
    : { total_runs: 0, successful_runs: 0, failed_runs: 0, pending_runs: 0, runs: [] };
}

export async function getCheckAnnotations(
  accountId: string,
  prId: string
): Promise<CheckAnnotationView[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listCheckAnnotations({ account_id: accountId, pr_id: prId }));
  }
  if (prId !== 'pr_1') {
    return [];
  }
  return mockCheckAnnotations.map((annotation) => ({
    ...annotation,
    is_outdated:
      annotation.is_outdated ||
      Boolean(
        annotation.check_run_head_sha && annotation.check_run_head_sha !== mockPrDetail.head_sha
      )
  }));
}

export async function getCheckAnnotationsForFile(
  accountId: string,
  prId: string,
  path: string
): Promise<CheckAnnotationView[]> {
  if (isTauriRuntime()) {
    return unwrap(
      commands.listCheckAnnotationsForFile({
        account_id: accountId,
        pr_id: prId,
        path
      })
    );
  }
  if (prId !== 'pr_1') {
    return [];
  }
  return mockCheckAnnotations
    .filter((annotation) => annotation.anchor_path === path)
    .map((annotation) => ({
      ...annotation,
      is_outdated:
        annotation.is_outdated ||
        Boolean(
          annotation.check_run_head_sha && annotation.check_run_head_sha !== mockPrDetail.head_sha
        )
    }));
}

export async function startCheckLogStream(
  checkRunId: string,
  tailLines = 500
): Promise<StreamHandle> {
  if (isTauriRuntime()) {
    return unwrap(
      commands.startCheckLogStream({
        check_run_id: checkRunId,
        tail_lines: tailLines
      })
    );
  }
  const eventName = `check_log:${checkRunId}:chunk`;
  queueMicrotask(async () => {
    const run = mockChecksState.runs.find((entry) => entry.id === checkRunId);
    const detailsUrl = run?.details_url ?? null;
    const isActions = detailsUrl?.includes('/actions/runs/') ?? false;
    if (!isActions) {
      await emitMockEvent(eventName, {
        kind: 'fallback',
        text: null,
        details_url: detailsUrl
      });
      await emitMockEvent(eventName, { kind: 'done', text: null, details_url: null });
      return;
    }
    const lines = Array.from(
      { length: 40 },
      (_value, index) => `log line ${index + 1} for ${checkRunId}\n`
    );
    for (const line of lines) {
      await emitMockEvent(eventName, { kind: 'chunk', text: line, details_url: null });
    }
    const tail = lines.slice(-Math.min(lines.length, tailLines)).join('');
    await emitMockEvent(eventName, { kind: 'tail', text: tail, details_url: null });
    await emitMockEvent(eventName, { kind: 'done', text: null, details_url: null });
  });
  return { event_name: eventName };
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

export async function listWorktrees(accountId: string): Promise<WorktreeView[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listWorktrees(accountId));
  }
  return mockWorktrees.filter((worktree) => worktree.account_id === accountId);
}

export async function setWorktreeManualOverride(
  worktreeId: string,
  prId: string | null
): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setWorktreeManualOverride(worktreeId, prId));
    return;
  }
  mockWorktrees = mockWorktrees.map((worktree) =>
    worktree.id === worktreeId
      ? {
          ...worktree,
          manual_override_pr_id: prId,
          mapped_pr_id: prId,
          mapping_confidence: prId ? 1 : 0.87,
          mapping_source: prId ? '{"manual":true}' : worktree.mapping_source
        }
      : worktree
  );
}

export async function setWorktreeRoots(roots: string[]): Promise<string[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.setWorktreeRoots(roots));
  }
  mockWorktreeRoots = roots;
  return mockWorktreeRoots;
}

export async function listWorktreeRoots(): Promise<string[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listWorktreeRoots());
  }
  return mockWorktreeRoots;
}

export async function rediscoverWorktrees(): Promise<RediscoverSummary> {
  if (isTauriRuntime()) {
    return unwrap(commands.rediscoverWorktrees());
  }
  return {
    ...MOCK_REDISCOVER_SUMMARY,
    roots: mockWorktreeRoots,
    discovered: mockWorktrees.length,
    watched: mockWorktrees.length
  };
}

export async function cleanupWorktree(worktreeId: string, force: boolean): Promise<CleanupOutcome> {
  if (isTauriRuntime()) {
    return unwrapTypedError<CleanupOutcome, CleanupError>(
      commands.cleanupWorktree(worktreeId, force)
    );
  }
  if (!force) {
    throw new Error('ForceRequired');
  }
  return { ...MOCK_CLEANUP_OUTCOME, worktree_id: worktreeId };
}

function findMockStack(stackId: string): StackGraph | null {
  for (const stacks of mockStacksByScope.values()) {
    const match = stacks.find((stack) => stack.stack_id === stackId);
    if (match) {
      return match;
    }
  }
  return null;
}

export async function listStacks(accountId: string, repoId: string): Promise<StackGraph[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listStacks({ account_id: accountId, repo_id: repoId }));
  }
  return cloneStackGraphs(mockStacksByScope.get(stackScopeKey(accountId, repoId)) ?? []);
}

export async function startRebaseStack(accountId: string, stackId: string): Promise<string> {
  if (isTauriRuntime()) {
    return unwrap(commands.startRebaseStack({ account_id: accountId, stack_id: stackId }));
  }
  pushMockStackInvocation('start_rebase_stack', { account_id: accountId, stack_id: stackId });
  const stack = findMockStack(stackId);
  const totalSteps = stack?.nodes.length ?? 1;
  const opId = `stack-op-${++mockStackOpSeq}`;
  const operation: StackOperationView = {
    id: opId,
    stack_id: stackId,
    account_id: accountId,
    op_kind: 'rebase',
    status: 'running',
    current_pr_id: stack?.nodes[0]?.pr_id ?? null,
    current_step: totalSteps > 0 ? 1 : null,
    total_steps: totalSteps,
    worktree_path: '/tmp/mock-worktree',
    conflict_files: [],
    last_error: null,
    started_at: nowEpoch(),
    updated_at: nowEpoch(),
    finished_at: null
  };
  await emitMockStackOperation(operation);
  return opId;
}

export async function startMergeStack(
  accountId: string,
  stackId: string,
  method: MergeMethod
): Promise<string> {
  if (isTauriRuntime()) {
    return unwrap(commands.startMergeStack({ account_id: accountId, stack_id: stackId, method }));
  }
  pushMockStackInvocation('start_merge_stack', {
    account_id: accountId,
    stack_id: stackId,
    method
  });
  const stack = findMockStack(stackId);
  const orderedNodes = [...(stack?.nodes ?? [])].sort(
    (left, right) => left.position - right.position
  );
  for (let index = 0; index < orderedNodes.length; index += 1) {
    const current = orderedNodes[index];
    if (!current) {
      continue;
    }
    pushMockStackInvocation('merge_mutation', {
      account_id: accountId,
      stack_id: stackId,
      pr_id: current.pr_id,
      method
    });
    const next = orderedNodes[index + 1];
    if (next) {
      pushMockStackInvocation('update_pull_request_base', {
        account_id: accountId,
        stack_id: stackId,
        pull_request_id: next.pr_id,
        base_ref_name: current.base_ref
      });
    }
  }
  const totalSteps = stack?.nodes.length ?? 1;
  const opId = `stack-op-${++mockStackOpSeq}`;
  const operation: StackOperationView = {
    id: opId,
    stack_id: stackId,
    account_id: accountId,
    op_kind: 'merge',
    status: 'running',
    current_pr_id: stack?.nodes[0]?.pr_id ?? null,
    current_step: totalSteps > 0 ? 1 : null,
    total_steps: totalSteps,
    worktree_path: '/tmp/mock-worktree',
    conflict_files: [],
    last_error: null,
    started_at: nowEpoch(),
    updated_at: nowEpoch(),
    finished_at: null
  };
  await emitMockStackOperation(operation);
  return opId;
}

export async function resumeStackOp(opId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.resumeStackOp({ op_id: opId }));
    return;
  }
  pushMockStackInvocation('resume_stack_op', { op_id: opId });
  const existing = mockStackOps.get(opId);
  if (!existing) {
    return;
  }
  await emitMockStackOperation({
    ...existing,
    status: 'running',
    updated_at: nowEpoch(),
    finished_at: null
  });
}

export async function abortStackOp(opId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.abortStackOp({ op_id: opId }));
    return;
  }
  pushMockStackInvocation('abort_stack_op', { op_id: opId });
  const existing = mockStackOps.get(opId);
  if (!existing) {
    return;
  }
  await emitMockStackOperation({
    ...existing,
    status: 'aborted',
    updated_at: nowEpoch(),
    finished_at: nowEpoch()
  });
}

export async function getStackOp(opId: string): Promise<StackOperationView | null> {
  if (isTauriRuntime()) {
    return unwrap(commands.getStackOp({ op_id: opId }));
  }
  const existing = mockStackOps.get(opId);
  return existing ? cloneStackOperation(existing) : null;
}

export async function getGraphiteStatus(): Promise<GraphiteIntegrationStatus> {
  if (isTauriRuntime()) {
    return unwrap(commands.getGraphiteStatus());
  }
  return {
    detected_version: mockGraphiteStatus.detected_version,
    enabled: mockGraphiteStatus.enabled
  };
}

export async function setGraphiteEnabled(enabled: boolean): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setGraphiteEnabled(enabled));
    return;
  }
  mockGraphiteStatus = { ...mockGraphiteStatus, enabled };
  writeStoredGraphiteStatus(mockGraphiteStatus);
}

export async function getRelaySettings(): Promise<RelaySettingsSnapshot> {
  if (isTauriRuntime()) {
    return unwrap(commands.getRelaySettings());
  }
  mockRelaySettings = {
    ...mockRelaySettings,
    local_url: recomputeMockRelayUrl()
  };
  return { ...mockRelaySettings };
}

export async function setRelayEnabled(enabled: boolean): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setRelayEnabled(enabled));
    return;
  }
  mockRelaySettings = {
    ...mockRelaySettings,
    enabled,
    local_url: recomputeMockRelayUrl()
  };
  writeStoredRelaySettings(mockRelaySettings);
}

export async function setRelayForwardSecret(secret: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setRelayForwardSecret(secret));
    return;
  }
  mockRelaySettings = {
    ...mockRelaySettings,
    has_forward_secret: secret.trim().length > 0,
    local_url: recomputeMockRelayUrl()
  };
  writeStoredRelaySettings(mockRelaySettings);
}

export async function relayLocalUrl(): Promise<string> {
  if (isTauriRuntime()) {
    return unwrap(commands.relayLocalUrl());
  }
  return recomputeMockRelayUrl() ?? '';
}

export async function emitMockStackChanged(
  accountId: string,
  repoId: string,
  stacks: StackGraph[]
): Promise<void> {
  const cloned = cloneStackGraphs(stacks);
  mockStacksByScope.set(stackScopeKey(accountId, repoId), cloned);
  await emitMockStacksChanged(accountId, repoId, cloned);
}

export async function emitMockStackOperationEvent(operation: StackOperationView): Promise<void> {
  await emitMockStackOperation(cloneStackOperation(operation));
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

export async function listNotificationRules(accountId: string): Promise<NotificationRule[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listNotificationRules(accountId));
  }
  return ensureMockNotificationRules(accountId);
}

export async function setNotificationRule(
  accountId: string,
  kind: string,
  enabled: boolean,
  configJson: string
): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setNotificationRule(accountId, kind, enabled, configJson));
    return;
  }
  pushMockNotificationInvocation('set_notification_rule', {
    account_id: accountId,
    kind,
    enabled,
    config_json: configJson
  });
  const rules = ensureMockNotificationRules(accountId);
  const next = rules.map((rule) =>
    rule.kind === kind
      ? { ...rule, enabled, config_json: configJson, updated_at: nowEpoch() }
      : rule
  );
  mockNotificationRules.set(accountId, next);
}

export async function setQuietHours(accountId: string, json: string | null): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setQuietHours(accountId, json));
    return;
  }
  pushMockNotificationInvocation('set_quiet_hours', { account_id: accountId, json });
  const settings = ensureMockNotificationSettings(accountId);
  settings.quiet_hours_json = json;
}

export async function setFocusMode(accountId: string, on: boolean): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setFocusMode(accountId, on));
    return;
  }
  pushMockNotificationInvocation('set_focus_mode', { account_id: accountId, on });
  const settings = ensureMockNotificationSettings(accountId);
  settings.focus_mode = on;
}

export async function setPerRepoFilters(
  accountId: string,
  allow: string[],
  deny: string[]
): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.setPerRepoFilters(accountId, allow, deny));
    return;
  }
  pushMockNotificationInvocation('set_per_repo_filters', {
    account_id: accountId,
    allow,
    deny
  });
  const settings = ensureMockNotificationSettings(accountId);
  settings.allow = [...allow];
  settings.deny = [...deny];
}

export async function listNotificationEvents(
  accountId: string,
  limit: number | null = null,
  offset: number | null = null,
  since: number | null = null
): Promise<NotificationEventRow[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listNotificationEvents(accountId, limit, offset, since));
  }
  const events = mockNotificationEvents.get(accountId) ?? [];
  return events
    .filter((event) => (since == null ? true : event.fired_at >= since))
    .slice(offset ?? 0, (offset ?? 0) + (limit ?? 50));
}

export async function markNotificationEventSeen(eventId: string): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.markNotificationEventSeen(eventId));
    return;
  }
  pushMockNotificationInvocation('mark_notification_event_seen', { event_id: eventId });
  for (const [accountId, events] of mockNotificationEvents.entries()) {
    const next = events.map((event) => (event.id === eventId ? { ...event, seen: true } : event));
    mockNotificationEvents.set(accountId, next);
  }
}

export async function notifDebugSimulateEvent(
  accountId: string,
  payload: {
    kind: string;
    repo_id?: string | null;
    repo_full_name?: string | null;
    pr_id?: string | null;
    actor_id?: string | null;
    server_event_id?: string | null;
    title: string;
    body: string;
  }
): Promise<NotificationEventPayload | null> {
  if (isTauriRuntime()) {
    return unwrap(commands.notifDebugSimulateEvent(accountId, JSON.stringify(payload)));
  }
  pushMockNotificationInvocation('__notif_debug__simulate_event', {
    account_id: accountId,
    payload
  });
  const event: NotificationEventPayload = {
    event_id: payload.server_event_id ?? `mock-notif-${Date.now()}`,
    account_id: accountId,
    repo_id: payload.repo_id ?? null,
    pr_id: payload.pr_id ?? null,
    event_type: payload.kind,
    actor_id: payload.actor_id ?? 'mock-actor',
    server_event_id: payload.server_event_id ?? `mock-event-${Date.now()}`,
    title: payload.title,
    body: payload.body,
    fired_at: nowEpoch(),
    deduped: false
  };
  const existing = mockNotificationEvents.get(accountId) ?? [];
  mockNotificationEvents.set(accountId, [eventRowFromPayload(event), ...existing]);
  await emitMockEvent('notification:event', event);
  return event;
}

export async function submitMutation(accountId: string, kind: MutationKind, payloadJson: string) {
  if (isTauriRuntime()) {
    return unwrap(commands.submitMutation(accountId, kind, payloadJson));
  }

  const parsed = JSON.parse(payloadJson) as Record<string, unknown>;
  mockMutationInvocations.push({
    account_id: accountId,
    kind,
    payload: parsed
  });
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
  mockMutationInputs.set(mutationId, {
    account_id: accountId,
    kind,
    payload: parsed
  });
  mockMutationCalls.push({
    mutation_id: mutationId,
    account_id: accountId,
    kind,
    payload: parsed
  });
  await emitMockEvent('mutation:submitted', { mutation });
  if (
    kind === 'apply_suggestion_batch' &&
    optionalString(parsed.expected_head_sha) &&
    optionalString(parsed.expected_head_sha) !== mockPrDetail.head_sha
  ) {
    queueMicrotask(() => {
      void failMockMutation(mutationId, 'conflict');
    });
    return {
      mutation_id: mutationId,
      deduped: false,
      requires_confirmation: mutation.requires_connection_confirmation,
      optimism_level: optimism,
      projected_changes: [kind]
    };
  }
  if (mockNetState.state === 'online' && mockAutoSettleMutations) {
    queueMicrotask(() => {
      void settleMockMutation(mutation);
    });
  }
  return {
    mutation_id: mutationId,
    deduped: false,
    requires_confirmation: mutation.requires_connection_confirmation,
    optimism_level: optimism,
    projected_changes: [kind, ...Object.keys(parsed).slice(0, 2)]
  };
}

function toRateLimitEventSnapshot() {
  const status = applyMockRateLimitOverrides(mockStatus(null));
  return status.rate_limits.map((bucket) => ({
    account_id: bucket.account_id,
    resource: bucket.resource,
    remaining: bucket.remaining,
    used: bucket.used ?? Math.max(0, bucket.limit_total - bucket.remaining),
    limit_total: bucket.limit_total,
    reset_at_epoch: bucket.reset_at
  }));
}

export async function emitMockRateLimitPressure(accountId: string): Promise<void> {
  const payload = {
    account_id: accountId,
    snapshot: toRateLimitEventSnapshot()
  };
  await emitMockEvent(`rate_limit_pressure:${accountId}`, payload);
  await emitMockEvent('rate_limit_pressure:<account>', payload);
}

export async function emitMockRateLimitBypass(accountId: string): Promise<void> {
  const payload = {
    account_id: accountId,
    snapshot: toRateLimitEventSnapshot()
  };
  await emitMockEvent(`rate_limit_bypass:${accountId}`, payload);
  await emitMockEvent('rate_limit_bypass:<account>', payload);
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
  if (mockNetState.state === 'offline' || !mockAutoSettleMutations) {
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
  mockMutationInputs.delete(mutationId);
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

export async function listSavedReplies(accountId: string): Promise<SavedReply[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.listSavedReplies(accountId));
  }
  return sortSavedReplies(ensureMockSavedReplies(accountId));
}

export async function createSavedReply(
  accountId: string,
  name: string,
  body: string
): Promise<SavedReply> {
  if (isTauriRuntime()) {
    return unwrap(commands.createSavedReply(accountId, name, body));
  }
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error('SavedReplyNameRequired: saved reply name is required');
  }
  const rows = ensureMockSavedReplies(accountId);
  if (rows.some((row) => row.name.toLowerCase() === trimmed.toLowerCase())) {
    throw new Error(`SavedReplyNameTaken: saved reply "${trimmed}" already exists`);
  }
  const sort_order = rows.length;
  const timestamp = nowEpoch();
  const created: SavedReply = {
    id: ++mockSavedReplySeq,
    account_id: accountId,
    name: trimmed,
    body,
    sort_order,
    created_at: timestamp,
    updated_at: timestamp
  };
  mockSavedReplies.set(accountId, [...rows, created]);
  return created;
}

export async function updateSavedReply(
  id: number,
  name: string,
  body: string
): Promise<SavedReply> {
  if (isTauriRuntime()) {
    return unwrap(commands.updateSavedReply(id, name, body));
  }
  const trimmed = name.trim();
  if (!trimmed) {
    throw new Error('SavedReplyNameRequired: saved reply name is required');
  }
  for (const [accountId, rows] of mockSavedReplies.entries()) {
    const target = rows.find((row) => row.id === id);
    if (!target) {
      continue;
    }
    if (rows.some((row) => row.id !== id && row.name.toLowerCase() === trimmed.toLowerCase())) {
      throw new Error(`SavedReplyNameTaken: saved reply "${trimmed}" already exists`);
    }
    const updated: SavedReply = {
      ...target,
      name: trimmed,
      body,
      updated_at: nowEpoch()
    };
    mockSavedReplies.set(
      accountId,
      rows.map((row) => (row.id === id ? updated : row))
    );
    return updated;
  }
  throw new Error('SavedReplyNotFound: saved reply not found');
}

export async function deleteSavedReply(id: number): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.deleteSavedReply(id));
    return;
  }
  for (const [accountId, rows] of mockSavedReplies.entries()) {
    if (!rows.some((row) => row.id === id)) {
      continue;
    }
    const filtered = rows
      .filter((row) => row.id !== id)
      .map((row, index) => ({ ...row, sort_order: index, updated_at: nowEpoch() }));
    mockSavedReplies.set(accountId, filtered);
    return;
  }
}

export async function reorderSavedReplies(accountId: string, orderedIds: number[]): Promise<void> {
  if (isTauriRuntime()) {
    await unwrap(commands.reorderSavedReplies({ account_id: accountId, ordered_ids: orderedIds }));
    return;
  }
  const rows = ensureMockSavedReplies(accountId);
  const byId = new Map(rows.map((row) => [row.id, row]));
  const reordered = orderedIds
    .map((id) => byId.get(id))
    .filter((row): row is SavedReply => Boolean(row))
    .map((row, index) => ({ ...row, sort_order: index, updated_at: nowEpoch() }));
  const missing = rows
    .filter((row) => !orderedIds.includes(row.id))
    .map((row, index) => ({
      ...row,
      sort_order: reordered.length + index,
      updated_at: nowEpoch()
    }));
  mockSavedReplies.set(accountId, [...reordered, ...missing]);
}

export async function importSavedRepliesFromGithub(accountId: string): Promise<SavedReply[]> {
  if (isTauriRuntime()) {
    return unwrap(commands.importSavedRepliesFromGithub(accountId));
  }
  void accountId;
  throw new Error(
    'SavedRepliesImportUnavailable: GitHub does not expose saved replies via API; create them here.'
  );
}

export async function uploadImageToGithubUserContent(
  accountId: string,
  imageBytes: number[],
  mime: string
): Promise<ImageUploadResult> {
  if (isTauriRuntime()) {
    return unwrap(commands.uploadImageToGithubUserContent(accountId, imageBytes, mime));
  }
  mockImageUploadInvocations.push({
    account_id: accountId,
    mime,
    size_bytes: imageBytes.length
  });
  const next = mockImageUploadQueue.shift();
  if (next?.kind === 'error') {
    await new Promise((resolve) => setTimeout(resolve, 20));
    throw new Error(`ImageUploadFailed: ${next.message}`);
  }
  if (next?.kind === 'success') {
    await new Promise((resolve) => setTimeout(resolve, 20));
    return next.result;
  }
  const hash = mockImageHash(imageBytes);
  await new Promise((resolve) => setTimeout(resolve, 20));
  return {
    url: `https://user-images.githubusercontent.com/mock/${hash}.png`,
    alt: 'pasted-image',
    content_hash: hash,
    size_bytes: imageBytes.length
  };
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    await openExternal(url);
    return;
  }
  mockExternalOpenInvocations.push(url);
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
    __NOTIF_DEBUG__?: {
      invocations: () => Array<{ command: string; payload: unknown }>;
      clearInvocations: () => void;
      simulateEvent: (
        accountId: string,
        payload: {
          kind: string;
          title: string;
          body: string;
          repo_id?: string | null;
          repo_full_name?: string | null;
          pr_id?: string | null;
          actor_id?: string | null;
          server_event_id?: string | null;
        }
      ) => Promise<NotificationEventPayload | null>;
    };
    __AUTH_DEBUG__?: {
      endpointInvocations: () => Array<{ host: string; api_url: string; graphql_url: string }>;
      clearEndpointInvocations: () => void;
    };
    __M4_MULTI_ACCOUNT_DEBUG__?: {
      mutationInvocations: () => Array<{
        account_id: string;
        kind: MutationKind;
        payload: Record<string, unknown>;
      }>;
      clearMutationInvocations: () => void;
      setRateLimit: (
        accountId: string,
        resource: string,
        remaining: number,
        limitTotal: number,
        resetAt?: number
      ) => void;
      emitRateLimitPressure: (accountId: string) => Promise<void>;
      emitRateLimitBypass: (accountId: string) => Promise<void>;
    };
    __M4_DEBUG__?: {
      setPrDetail: (partial: Partial<PrDetailSummary>) => void;
      resetPrDetail: () => void;
      setAutoSettleMutations: (enabled: boolean) => void;
      settleMutation: (mutationId: string) => Promise<void>;
      emitMergeableBackoffTick: (
        accountId: string,
        prId: string,
        attempt: number,
        nextSleepSeconds: number
      ) => Promise<void>;
      emitPrChanged: (prId: string) => Promise<void>;
      mutationCalls: () => Array<{
        mutation_id: string;
        account_id: string;
        kind: MutationKind;
        payload: Record<string, unknown>;
      }>;
      clearMutationCalls: () => void;
    };
    __RANGE_DIFF_DEBUG__?: {
      setMode: (mode: 'local' | 'rest' | null) => void;
      getMode: () => 'local' | 'rest' | null;
    };
    __M5_SUGGESTION_DEBUG__?: {
      emitWorktreeStep: (
        prId: string,
        step: 'opened' | 'assertions_ok' | 'patched' | 'committed' | 'pushed'
      ) => Promise<void>;
      setWorktreeDirty: (dirty: boolean) => Promise<void>;
      resetSuggestions: () => void;
    };
    __M5_SAVED_REPLIES_DEBUG__?: {
      setSavedReplies: (accountId: string, replies: Array<{ name: string; body: string }>) => void;
      clearSavedReplies: (accountId: string) => void;
      queueImageUploadFailure: (message: string) => void;
      queueImageUploadSuccess: (url: string, alt?: string) => void;
      resetImageUploadQueue: () => void;
      imageUploadInvocations: () => Array<{ account_id: string; mime: string; size_bytes: number }>;
    };
    __COMMANDS_DEBUG__?: {
      externalOpenInvocations: () => string[];
      clearExternalOpenInvocations: () => void;
    };
    __M6_STACKS_DEBUG__?: {
      invocations: () => Array<{ command: string; payload: Record<string, unknown> }>;
      clearInvocations: () => void;
      latestOpId: () => string | null;
      setStacks: (accountId: string, repoId: string, stacks: StackGraph[]) => Promise<void>;
      emitStackOperation: (operation: StackOperationView) => Promise<void>;
      setGraphiteStatus: (status: GraphiteIntegrationStatus) => void;
      getGraphiteStatus: () => GraphiteIntegrationStatus;
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

if (typeof window !== 'undefined' && !window.__NOTIF_DEBUG__) {
  window.__NOTIF_DEBUG__ = {
    invocations: () => [...mockNotificationInvocations],
    clearInvocations: () => {
      mockNotificationInvocations.splice(0, mockNotificationInvocations.length);
    },
    simulateEvent: (accountId, payload) => notifDebugSimulateEvent(accountId, payload)
  };
}

if (typeof window !== 'undefined' && !window.__AUTH_DEBUG__) {
  window.__AUTH_DEBUG__ = {
    endpointInvocations: () => [...mockEndpointTestInvocations],
    clearEndpointInvocations: () => {
      mockEndpointTestInvocations.splice(0, mockEndpointTestInvocations.length);
    }
  };
}

if (typeof window !== 'undefined' && !window.__M4_MULTI_ACCOUNT_DEBUG__) {
  window.__M4_MULTI_ACCOUNT_DEBUG__ = {
    mutationInvocations: () => [...mockMutationInvocations],
    clearMutationInvocations: () => {
      mockMutationInvocations.splice(0, mockMutationInvocations.length);
    },
    setRateLimit: (accountId, resource, remaining, limitTotal, resetAt) => {
      setMockRateLimit(accountId, resource, remaining, limitTotal, resetAt);
    },
    emitRateLimitPressure: (accountId) => emitMockRateLimitPressure(accountId),
    emitRateLimitBypass: (accountId) => emitMockRateLimitBypass(accountId)
  };
}

if (typeof window !== 'undefined' && !window.__M4_DEBUG__) {
  window.__M4_DEBUG__ = {
    setPrDetail: (partial) => {
      mockPrDetail = { ...mockPrDetail, ...partial };
    },
    resetPrDetail: () => {
      mockPrDetail = { ...MOCK_PR_DETAIL };
      mockChecksState = {
        ...MOCK_CHECKS,
        runs: MOCK_CHECKS.runs.map((run) => ({ ...run }))
      };
      mockAppliedSuggestionCommentIds.clear();
    },
    setAutoSettleMutations: (enabled) => {
      mockAutoSettleMutations = enabled;
    },
    settleMutation: async (mutationId) => {
      const mutation = mockPendingMutations.get(mutationId);
      if (!mutation) {
        return;
      }
      await settleMockMutation(mutation);
    },
    emitMergeableBackoffTick: async (accountId, prId, attempt, nextSleepSeconds) => {
      await emitMockEvent(`mergeable_backoff:${accountId}:${prId} tick`, {
        account_id: accountId,
        pr_id: prId,
        attempt,
        next_sleep_seconds: nextSleepSeconds
      });
    },
    emitPrChanged: async (prId) => {
      await emitMockEvent(`pr:${prId} changed`, { pr_id: prId });
    },
    mutationCalls: () => [...mockMutationCalls],
    clearMutationCalls: () => {
      mockMutationCalls.splice(0, mockMutationCalls.length);
    }
  };
}

if (typeof window !== 'undefined' && !window.__RANGE_DIFF_DEBUG__) {
  mockRangeDiffModeOverride = readStoredRangeDiffMode();
  window.__RANGE_DIFF_DEBUG__ = {
    setMode: (mode) => {
      mockRangeDiffModeOverride = mode;
      writeStoredRangeDiffMode(mode);
    },
    getMode: () => mockRangeDiffModeOverride
  };
}

if (typeof window !== 'undefined' && !window.__M5_SUGGESTION_DEBUG__) {
  window.__M5_SUGGESTION_DEBUG__ = {
    emitWorktreeStep: (prId, step) =>
      emitMockEvent(`worktree_write:${prId}:${step}`, { pr_id: prId, step }),
    setWorktreeDirty: async (dirty) => {
      mockWorktrees = mockWorktrees.map((worktree) =>
        worktree.mapped_pr_id === 'pr_1'
          ? {
              ...worktree,
              dirty,
              untracked_count: dirty ? 1 : 0,
              modified_count: dirty ? 1 : 0
            }
          : worktree
      );
      await emitMockEvent('worktree:discovery completed', {
        summary: {
          ...MOCK_REDISCOVER_SUMMARY,
          roots: mockWorktreeRoots,
          discovered: mockWorktrees.length,
          watched: mockWorktrees.length
        }
      });
    },
    resetSuggestions: () => {
      mockAppliedSuggestionCommentIds.clear();
    }
  };
}

if (typeof window !== 'undefined' && !window.__M5_SAVED_REPLIES_DEBUG__) {
  window.__M5_SAVED_REPLIES_DEBUG__ = {
    setSavedReplies: (accountId, replies) => {
      const now = nowEpoch();
      const mapped: SavedReply[] = replies.map((reply, index) => ({
        id: ++mockSavedReplySeq,
        account_id: accountId,
        name: reply.name,
        body: reply.body,
        sort_order: index,
        created_at: now,
        updated_at: now
      }));
      mockSavedReplies.set(accountId, mapped);
    },
    clearSavedReplies: (accountId) => {
      mockSavedReplies.set(accountId, []);
    },
    queueImageUploadFailure: (message) => {
      mockImageUploadQueue.push({ kind: 'error', message });
    },
    queueImageUploadSuccess: (url, alt = 'pasted-image') => {
      mockImageUploadQueue.push({
        kind: 'success',
        result: {
          url,
          alt,
          content_hash: `mock-${Date.now()}`,
          size_bytes: 1
        }
      });
    },
    resetImageUploadQueue: () => {
      mockImageUploadQueue.splice(0, mockImageUploadQueue.length);
      mockImageUploadInvocations.splice(0, mockImageUploadInvocations.length);
    },
    imageUploadInvocations: () => [...mockImageUploadInvocations]
  };
}

if (typeof window !== 'undefined' && !window.__COMMANDS_DEBUG__) {
  window.__COMMANDS_DEBUG__ = {
    externalOpenInvocations: () => [...mockExternalOpenInvocations],
    clearExternalOpenInvocations: () => {
      mockExternalOpenInvocations.splice(0, mockExternalOpenInvocations.length);
    }
  };
}

if (typeof window !== 'undefined' && !window.__M6_STACKS_DEBUG__) {
  window.__M6_STACKS_DEBUG__ = {
    invocations: () => [...mockStackInvocations],
    clearInvocations: () => {
      mockStackInvocations.splice(0, mockStackInvocations.length);
    },
    latestOpId: () => latestMockStackOpId(),
    setStacks: (accountId, repoId, stacks) => emitMockStackChanged(accountId, repoId, stacks),
    emitStackOperation: (operation) => emitMockStackOperationEvent(operation),
    setGraphiteStatus: (status) => {
      mockGraphiteStatus = {
        detected_version: status.detected_version ? { ...status.detected_version } : null,
        enabled: status.enabled
      };
      writeStoredGraphiteStatus(mockGraphiteStatus);
    },
    getGraphiteStatus: () => ({
      detected_version: mockGraphiteStatus.detected_version
        ? { ...mockGraphiteStatus.detected_version }
        : null,
      enabled: mockGraphiteStatus.enabled
    })
  };
}

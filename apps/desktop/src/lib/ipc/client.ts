import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  commands,
  type AccountLocator,
  type AuthAccount,
  type InitInboxResponse,
  type IpcError,
  type PrDetailSummary
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

let mockActiveAccountId = MOCK_INIT_INBOX.active_account_id ?? 'github.com:fixture-user';

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

export async function renderCommentHtml(body: string, repo: string | null) {
  if (isTauriRuntime()) {
    return unwrap(commands.ipcRenderedCommentHtml({ body, repo }));
  }
  return {
    html: `<p>${body.replace(/</g, '&lt;').replace(/>/g, '&gt;')}</p>`,
    cache_hit: false,
    content_hash: 'mock',
    cache_key: 'mock',
    renderer_version: 'mock'
  };
}

export async function listenEvent(
  eventName: string,
  callback: () => Promise<void> | void
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  return listen(eventName, async () => callback());
}

import { get, writable } from 'svelte/store';

import type {
  AuthAccount,
  InitInboxResponse,
  InboxItem,
  RepoSubscriptionItem,
  StackGraph,
  SystemStatusResponse,
  WorktreeView
} from '$lib/ipc/bindings';
import {
  listAccounts,
  listInbox,
  listStacks,
  listWorktreeRoots,
  listWorktrees,
  listRepoSubscriptions,
  rediscoverWorktrees,
  setWorktreeRoots,
  switchAccount,
  toAccountId,
  getSystemStatus
} from '$lib/ipc/client';

const ACTIVE_ACCOUNT_STORAGE_KEY = 'cockpit.active-account-id';
const INBOX_FILTER_STORAGE_KEY = 'cockpit.inbox-account-filter';
const ALL_ACCOUNTS_STORAGE_VALUE = '__all_accounts__';

export const accountsStore = writable<AuthAccount[]>([]);
export const activeAccountIdStore = writable<string | null>(null);
export const inboxAccountFilterStore = writable<string | null>(null);
export const inboxStore = writable<InboxItem[]>([]);
export const repoSubscriptionsStore = writable<RepoSubscriptionItem[]>([]);
export const statusStore = writable<SystemStatusResponse | null>(null);
export const worktreesStore = writable<WorktreeView[]>([]);
export const worktreeRootsStore = writable<string[]>([]);
export const stacksStore = writable<StackGraph[]>([]);
export const shellBootedStore = writable(false);
export const focusModeStore = writable<'focused' | 'background'>('focused');

function pickStoredAccountId(accounts: AuthAccount[]): string | null {
  if (typeof localStorage === 'undefined') {
    return null;
  }
  const stored = localStorage.getItem(ACTIVE_ACCOUNT_STORAGE_KEY);
  if (!stored) {
    return null;
  }
  return accounts.some((account) => toAccountId(account) === stored) ? stored : null;
}

function persistActiveAccountId(accountId: string | null): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  if (!accountId) {
    localStorage.removeItem(ACTIVE_ACCOUNT_STORAGE_KEY);
    return;
  }
  localStorage.setItem(ACTIVE_ACCOUNT_STORAGE_KEY, accountId);
}

function pickStoredInboxFilter(accounts: AuthAccount[]): string | null | undefined {
  if (typeof localStorage === 'undefined') {
    return undefined;
  }
  const stored = localStorage.getItem(INBOX_FILTER_STORAGE_KEY);
  if (stored === null) {
    return undefined;
  }
  if (stored === ALL_ACCOUNTS_STORAGE_VALUE) {
    return null;
  }
  return accounts.some((account) => toAccountId(account) === stored) ? stored : undefined;
}

function persistInboxFilter(accountIdFilter: string | null): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  if (accountIdFilter === null) {
    localStorage.setItem(INBOX_FILTER_STORAGE_KEY, ALL_ACCOUNTS_STORAGE_VALUE);
    return;
  }
  localStorage.setItem(INBOX_FILTER_STORAGE_KEY, accountIdFilter);
}

export async function initializeCockpit(seed: InitInboxResponse): Promise<void> {
  accountsStore.set(seed.accounts.accounts);
  const storedAccountId = pickStoredAccountId(seed.accounts.accounts);
  const activeAccountId = storedAccountId ?? seed.active_account_id;
  const storedFilter = pickStoredInboxFilter(seed.accounts.accounts);
  const inboxAccountFilter = storedFilter ?? activeAccountId;
  activeAccountIdStore.set(activeAccountId);
  inboxAccountFilterStore.set(inboxAccountFilter);
  persistActiveAccountId(activeAccountId);
  persistInboxFilter(inboxAccountFilter);
  inboxStore.set(seed.inbox);
  repoSubscriptionsStore.set(seed.subscriptions);
  statusStore.set(seed.status);
  worktreeRootsStore.set([]);
  worktreesStore.set([]);
  stacksStore.set([]);
  shellBootedStore.set(true);
  void (async () => {
    try {
      worktreeRootsStore.set(await listWorktreeRoots());
      if (activeAccountId) {
        worktreesStore.set(await listWorktrees(activeAccountId));
      }
    } catch {
      // Keep boot path resilient if worktree IPC fails.
    }
  })();
  if (!activeAccountId) {
    return;
  }
  if (storedAccountId && storedAccountId !== seed.active_account_id) {
    await refreshAccountData(inboxAccountFilter);
    return;
  }
  if (storedFilter !== undefined && storedFilter !== seed.active_account_id) {
    await refreshAccountData(inboxAccountFilter);
  }
}

export async function refreshAccounts(): Promise<void> {
  const response = await listAccounts();
  accountsStore.set(response.accounts);
  const fallback = response.active
    ? toAccountId(response.active)
    : response.accounts[0]
      ? toAccountId(response.accounts[0])
      : null;
  const current = get(activeAccountIdStore);
  const nextActive =
    current && response.accounts.some((account) => toAccountId(account) === current)
      ? current
      : fallback;
  activeAccountIdStore.set(nextActive);
  persistActiveAccountId(nextActive);
  const currentFilter = get(inboxAccountFilterStore);
  if (
    currentFilter &&
    !response.accounts.some((account) => toAccountId(account) === currentFilter)
  ) {
    inboxAccountFilterStore.set(nextActive);
    persistInboxFilter(nextActive);
  }
}

export async function refreshAccountData(accountIdFilter?: string | null): Promise<void> {
  const selectedActive = get(activeAccountIdStore);
  const selectedFilter =
    accountIdFilter === undefined ? get(inboxAccountFilterStore) : accountIdFilter;
  if (!selectedActive) {
    inboxStore.set([]);
    repoSubscriptionsStore.set([]);
    statusStore.set(null);
    worktreesStore.set([]);
    stacksStore.set([]);
    return;
  }
  const [inboxRows, subscriptions, status, worktrees] = await Promise.all([
    listInbox(selectedFilter),
    listRepoSubscriptions(selectedActive),
    getSystemStatus(selectedFilter),
    listWorktrees(selectedActive)
  ]);
  inboxStore.set(inboxRows);
  repoSubscriptionsStore.set(subscriptions);
  statusStore.set(status);
  worktreesStore.set(worktrees);
  const stackGroups = await Promise.all(
    subscriptions.map((subscription) => listStacks(selectedActive, subscription.repo_id))
  );
  stacksStore.set(stackGroups.flat());
}

export async function selectAccountById(accountId: string | null): Promise<void> {
  if (!accountId) {
    inboxAccountFilterStore.set(null);
    persistInboxFilter(null);
    await refreshAccountData(null);
    return;
  }
  const accounts = get(accountsStore);
  const account = accounts.find((candidate) => toAccountId(candidate) === accountId);
  if (!account) {
    return;
  }
  await switchAccount({ host: account.host, login: account.login });
  activeAccountIdStore.set(accountId);
  persistActiveAccountId(accountId);
  accountsStore.set(
    accounts.map((candidate) => ({
      ...candidate,
      is_active: toAccountId(candidate) === accountId
    }))
  );
  inboxAccountFilterStore.set(accountId);
  persistInboxFilter(accountId);
  await refreshAccountData(accountId);
}

export function getActiveAccountId(): string | null {
  return get(activeAccountIdStore);
}

export async function refreshWorktreeRoots(): Promise<void> {
  worktreeRootsStore.set(await listWorktreeRoots());
}

export async function saveWorktreeRoots(roots: string[]): Promise<void> {
  await setWorktreeRoots(roots);
  await refreshWorktreeRoots();
  const active = get(activeAccountIdStore);
  if (active) {
    await refreshAccountData(active);
  }
}

export async function triggerWorktreeRediscovery(): Promise<void> {
  await rediscoverWorktrees();
  const active = get(activeAccountIdStore);
  if (active) {
    worktreesStore.set(await listWorktrees(active));
  }
}

export async function refreshStacksForActiveAccount(): Promise<void> {
  const selectedActive = get(activeAccountIdStore);
  if (!selectedActive) {
    stacksStore.set([]);
    return;
  }
  const subscriptions = get(repoSubscriptionsStore);
  const stackGroups = await Promise.all(
    subscriptions.map((subscription) => listStacks(selectedActive, subscription.repo_id))
  );
  stacksStore.set(stackGroups.flat());
}

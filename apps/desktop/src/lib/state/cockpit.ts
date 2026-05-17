import { get, writable } from 'svelte/store';

import type {
  AuthAccount,
  InitInboxResponse,
  InboxItem,
  RepoSubscriptionItem,
  SystemStatusResponse
} from '$lib/ipc/bindings';
import {
  listAccounts,
  listInbox,
  listRepoSubscriptions,
  switchAccount,
  toAccountId,
  getSystemStatus
} from '$lib/ipc/client';

const ACTIVE_ACCOUNT_STORAGE_KEY = 'cockpit.active-account-id';

export const accountsStore = writable<AuthAccount[]>([]);
export const activeAccountIdStore = writable<string | null>(null);
export const inboxStore = writable<InboxItem[]>([]);
export const repoSubscriptionsStore = writable<RepoSubscriptionItem[]>([]);
export const statusStore = writable<SystemStatusResponse | null>(null);
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

export async function initializeCockpit(seed: InitInboxResponse): Promise<void> {
  accountsStore.set(seed.accounts.accounts);
  const storedAccountId = pickStoredAccountId(seed.accounts.accounts);
  const activeAccountId = storedAccountId ?? seed.active_account_id;
  activeAccountIdStore.set(activeAccountId);
  persistActiveAccountId(activeAccountId);
  inboxStore.set(seed.inbox);
  repoSubscriptionsStore.set(seed.subscriptions);
  statusStore.set(seed.status);
  shellBootedStore.set(true);
  if (!activeAccountId) {
    return;
  }
  if (storedAccountId && storedAccountId !== seed.active_account_id) {
    await refreshAccountData(storedAccountId);
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
}

export async function refreshAccountData(accountId?: string): Promise<void> {
  const selected = accountId ?? get(activeAccountIdStore);
  if (!selected) {
    inboxStore.set([]);
    repoSubscriptionsStore.set([]);
    statusStore.set(null);
    return;
  }
  const [inboxRows, subscriptions, status] = await Promise.all([
    listInbox(selected),
    listRepoSubscriptions(selected),
    getSystemStatus(selected)
  ]);
  inboxStore.set(inboxRows);
  repoSubscriptionsStore.set(subscriptions);
  statusStore.set(status);
}

export async function selectAccountById(accountId: string): Promise<void> {
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
  await refreshAccountData(accountId);
}

export function getActiveAccountId(): string | null {
  return get(activeAccountIdStore);
}

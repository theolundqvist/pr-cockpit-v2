import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { InboxItem } from '$lib/ipc/bindings';

describe('aggregated inbox badges', () => {
  it('renders per-row account badges in all-accounts mode', async () => {
    const selectAccount = vi.fn();
    const row: InboxItem = {
      account_id: 'github.enterprise.test:octo-enterprise',
      account_login: 'octo-enterprise',
      account_host: 'github.enterprise.test',
      pr_id: 'pr_2',
      repo_id: 'repo_2',
      repo_owner: 'octo',
      repo_name: 'enterprise',
      pr_number: 2,
      title: 'Enterprise account row',
      state: 'open',
      draft: false,
      head_sha: 'head-b',
      base_sha: 'base-b',
      mergeable_state: 'clean',
      merge_state_status: 'behind',
      updated_at: 1_715_000_200,
      author_login: 'octo-enterprise',
      unread_notification_count: 0,
      latest_notification_at: 1_715_000_200,
      pending_overlay: null
    };

    const InboxRow = (await import('$lib/components/InboxRow.svelte')).default;
    render(InboxRow, {
      item: row,
      selected: false,
      showAccountBadge: true,
      onSelectAccount: selectAccount
    });

    const badge = screen.getByTestId('inbox-account-badge');
    expect(badge.textContent ?? '').toContain('@octo-enterprise · github.enterprise.test');
    await fireEvent.click(badge);
    expect(selectAccount).toHaveBeenCalledWith('github.enterprise.test:octo-enterprise');
  });
});

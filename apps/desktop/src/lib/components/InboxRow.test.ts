import { describe, expect, it } from 'vitest';

import { checkLabel, syntheticLabels } from '$lib/components/inbox-row';
import type { InboxItem } from '$lib/ipc/bindings';

const row: InboxItem = {
  account_id: 'github.com:fixture-user',
  pr_id: 'pr_1',
  repo_id: 'repo_1',
  repo_owner: 'fixture-org',
  repo_name: 'repo-1',
  pr_number: 1,
  title: 'Active Fixture PR Falcon Diff Stress',
  state: 'open',
  draft: false,
  head_sha: 'head',
  base_sha: 'base',
  mergeable_state: 'clean',
  merge_state_status: 'behind',
  updated_at: 1_715_000_100,
  author_login: 'fixture-user-01',
  unread_notification_count: 2,
  latest_notification_at: 1_715_000_100
};

describe('InboxRow', () => {
  it('computes chip labels and check rollup text for the component model', () => {
    expect(syntheticLabels(row)).toContain('needs-review');
    expect(checkLabel(row)).toBe('Checks green');
  });
});

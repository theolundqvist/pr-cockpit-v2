import { describe, expect, it } from 'vitest';

import { checkLabel, mappedWorktreeForPr, syntheticLabels } from '$lib/components/inbox-row';
import type { InboxItem, WorktreeView } from '$lib/ipc/bindings';

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
  latest_notification_at: 1_715_000_100,
  pending_overlay: null
};

describe('InboxRow', () => {
  it('computes chip labels and check rollup text for the component model', () => {
    expect(syntheticLabels(row)).toContain('needs-review');
    expect(checkLabel(row)).toBe('Checks green');
  });

  it('picks the highest-confidence mapped worktree for a PR', () => {
    const worktrees: WorktreeView[] = [
      {
        id: 'wt-1',
        account_id: row.account_id,
        repo_id: row.repo_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        path: '/tmp/repo-a',
        head_sha: row.head_sha,
        branch: 'feature/a',
        dirty: false,
        ahead: 0,
        behind: 0,
        untracked_count: 0,
        staged_count: 0,
        modified_count: 0,
        mapped_pr_id: row.pr_id,
        mapped_pr_number: row.pr_number,
        mapping_confidence: 0.33,
        mapping_source: '{}',
        is_app_managed: false,
        manual_override_pr_id: null,
        manual_override_at: null,
        last_cleanup_snapshot_id: null,
        created_at: 1,
        updated_at: 1
      },
      {
        id: 'wt-2',
        account_id: row.account_id,
        repo_id: row.repo_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        path: '/tmp/repo-b',
        head_sha: row.head_sha,
        branch: 'feature/b',
        dirty: true,
        ahead: 1,
        behind: 1,
        untracked_count: 1,
        staged_count: 0,
        modified_count: 1,
        mapped_pr_id: row.pr_id,
        mapped_pr_number: row.pr_number,
        mapping_confidence: 0.85,
        mapping_source: '{}',
        is_app_managed: false,
        manual_override_pr_id: null,
        manual_override_at: null,
        last_cleanup_snapshot_id: null,
        created_at: 1,
        updated_at: 2
      }
    ];
    expect(mappedWorktreeForPr(row.pr_id, worktrees)?.id).toBe('wt-2');
  });
});

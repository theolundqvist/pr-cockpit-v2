import { describe, expect, it } from 'vitest';

import { reduceConversationTimeline } from '$lib/timeline/reducer';

describe('reduceConversationTimeline', () => {
  it('combines rendered timeline entries with thread and metadata events', () => {
    const entries = reduceConversationTimeline(
      {
        account_id: 'acct',
        pr_id: 'pr_1',
        repo_id: 'repo_1',
        pr_number: 1,
        title: 'PR',
        body: 'Body',
        state: 'open',
        draft: false,
        base_ref: 'main',
        base_sha: 'base',
        head_ref: 'feature',
        head_sha: 'head',
        mergeable_state: 'clean',
        merge_state_status: 'behind',
        additions: 1,
        deletions: 1,
        changed_files: 1,
        comment_count: 1,
        review_count: 1,
        thread_count: 1,
        check_run_count: 1,
        file_count: 1,
        updated_at: 100
      },
      [
        {
          item_id: 'comment_1',
          item_kind: 'issue',
          body: 'hello',
          author_login: 'alice',
          created_at: 101,
          updated_at: 101,
          review_state: null,
          rendered_html: '<p>hello</p>',
          renderer_version: 'v1'
        }
      ],
      [
        {
          id: 'thread_1',
          path: 'src/a.ts',
          line: 10,
          side: 'RIGHT',
          start_line: 8,
          start_side: 'RIGHT',
          is_outdated: true,
          is_resolved: false,
          resolved_by_login: null,
          updated_at: 102,
          comment_count: 3
        }
      ],
      {
        labels: [{ label_name: 'needs-review', label_color: 'fbca04', description: null }],
        assignees: [{ user_id: 'u1', login: 'alice' }],
        requested_reviewers: [
          {
            user_id: 'u2',
            login: 'bob',
            reviewer_type: 'user',
            reviewer_state: 'requested',
            requested_at: 99
          }
        ],
        projects: [],
        milestones: []
      }
    );

    expect(entries.some((entry) => entry.kind === 'comment')).toBe(true);
    expect(entries.some((entry) => entry.kind === 'thread')).toBe(true);
    expect(entries.some((entry) => entry.kind === 'event' && entry.title === 'Label')).toBe(true);
    expect(entries[0]?.createdAt).toBeGreaterThanOrEqual(
      entries[entries.length - 1]?.createdAt ?? 0
    );
  });
});

import type {
  AccountsListResponse,
  CheckAnnotationView,
  InitInboxResponse,
  InboxItem,
  PrCheckSummary,
  PrDetailSummary,
  PrFilesResponse,
  PrMetadataResponse,
  PrPushView,
  RangeDiff,
  PrPatchResponse,
  RepoSubscriptionItem,
  ReviewThreadsPage,
  WorktreeView,
  RediscoverSummary,
  CleanupOutcome,
  SystemStatusResponse,
  TimelinePage
} from '$lib/ipc/bindings';

const BASE_TS = 1_715_000_000;

const primaryAccount = {
  host: 'github.com',
  login: 'fixture-user',
  api_base_url: 'https://api.github.com',
  graphql_url: 'https://api.github.com/graphql',
  token_kind: 'pat',
  scopes: ['notifications', 'read:org', 'repo'],
  created_at: BASE_TS,
  updated_at: BASE_TS,
  is_active: true
};

const secondaryAccount = {
  host: 'github.enterprise.test',
  login: 'octo-enterprise',
  api_base_url: 'https://github.enterprise.test/api/v3',
  graphql_url: 'https://github.enterprise.test/api/graphql',
  token_kind: 'oauth-device',
  scopes: ['notifications', 'repo'],
  created_at: BASE_TS,
  updated_at: BASE_TS,
  is_active: false
};

const accountId = (host: string, login: string): string => `${host}:${login}`;

function buildInbox(account_id: string, account_login: string, account_host: string): InboxItem[] {
  const rows: InboxItem[] = [];
  for (let idx = 1; idx <= 200; idx += 1) {
    const repoN = (idx % 3) + 1;
    rows.push({
      account_id,
      account_login,
      account_host,
      pr_id: `pr_${idx}`,
      repo_id: `repo_${repoN}`,
      repo_owner: 'fixture-org',
      repo_name: `repo-${repoN}`,
      pr_number: idx,
      title: idx === 1 ? 'Active Fixture PR Falcon Diff Stress' : `Fixture Inbox PR #${idx}`,
      state: 'open',
      draft: idx % 11 === 0,
      head_sha:
        idx === 1
          ? 'active_head_sha_000000000000000000000000000000000001'
          : `head_sha_${idx.toString(16).padStart(40, '0')}`,
      base_sha: `base_sha_${idx.toString(16).padStart(40, '0')}`,
      mergeable_state: 'clean',
      merge_state_status: 'behind',
      updated_at: BASE_TS + idx,
      author_login: `fixture-user-${((idx % 40) + 1).toString().padStart(2, '0')}`,
      unread_notification_count: idx <= 150 ? (idx % 4) + 1 : 0,
      latest_notification_at: BASE_TS + idx,
      pending_overlay: null
    });
  }
  return rows;
}

export const MOCK_SUBSCRIPTIONS: RepoSubscriptionItem[] = [
  {
    account_id: accountId(primaryAccount.host, primaryAccount.login),
    repo_id: 'repo_1',
    repo_owner: 'fixture-org',
    repo_name: 'repo-1',
    watch_tier: 'hot',
    last_full_sync_at: BASE_TS - 60,
    updated_at: BASE_TS - 60
  },
  {
    account_id: accountId(primaryAccount.host, primaryAccount.login),
    repo_id: 'repo_2',
    repo_owner: 'fixture-org',
    repo_name: 'repo-2',
    watch_tier: 'warm',
    last_full_sync_at: BASE_TS - 60,
    updated_at: BASE_TS - 60
  },
  {
    account_id: accountId(primaryAccount.host, primaryAccount.login),
    repo_id: 'repo_3',
    repo_owner: 'fixture-org',
    repo_name: 'repo-3',
    watch_tier: 'cool',
    last_full_sync_at: BASE_TS - 60,
    updated_at: BASE_TS - 60
  }
];

export const MOCK_WORKTREE_ROOTS: string[] = ['~/dev', '~/code', '~/src', '~/repos'];

export const MOCK_WORKTREES: WorktreeView[] = [
  {
    id: 'wt_1',
    account_id: accountId(primaryAccount.host, primaryAccount.login),
    repo_id: 'repo_1',
    repo_owner: 'fixture-org',
    repo_name: 'repo-1',
    path: '/workspace/demo/repo-1',
    head_sha: 'active_head_sha_000000000000000000000000000000000001',
    branch: 'feature/pr-1',
    dirty: false,
    ahead: 1,
    behind: 0,
    untracked_count: 0,
    staged_count: 0,
    modified_count: 0,
    mapped_pr_id: 'pr_1',
    mapped_pr_number: 1,
    mapping_confidence: 0.87,
    mapping_source:
      '{"manual":false,"confidence":0.87,"signals":[{"signal":"remote_url_match","weight":0.3,"confidence":1,"contribution":0.3},{"signal":"branch_upstream_match","weight":0.2,"confidence":1,"contribution":0.2},{"signal":"exact_head_sha","weight":0.15,"confidence":1,"contribution":0.15},{"signal":"branch_convention","weight":0.1,"confidence":1,"contribution":0.1},{"signal":"head_sha_ancestry","weight":0.05,"confidence":1,"contribution":0.05}]}',
    is_app_managed: false,
    manual_override_pr_id: null,
    manual_override_at: null,
    last_cleanup_snapshot_id: null,
    created_at: BASE_TS,
    updated_at: BASE_TS + 10
  }
];

export const MOCK_REDISCOVER_SUMMARY: RediscoverSummary = {
  roots: MOCK_WORKTREE_ROOTS,
  discovered: 1,
  watched: 1,
  skipped_unmapped: 0,
  updated_at: BASE_TS + 10
};

export const MOCK_CLEANUP_OUTCOME: CleanupOutcome = {
  worktree_id: 'wt_1',
  snapshot_id: 'snapshot_mock',
  blocked_reason: 'm3_write_surface_disabled',
  dirty_detected: false,
  would_remove: true,
  removed: false
};

export const MOCK_ACCOUNTS: AccountsListResponse = {
  active: { host: primaryAccount.host, login: primaryAccount.login },
  accounts: [primaryAccount, secondaryAccount]
};

const INBOX_BY_ACCOUNT: Record<string, InboxItem[]> = {
  [accountId(primaryAccount.host, primaryAccount.login)]: buildInbox(
    accountId(primaryAccount.host, primaryAccount.login),
    primaryAccount.login,
    primaryAccount.host
  ),
  [accountId(secondaryAccount.host, secondaryAccount.login)]: buildInbox(
    accountId(secondaryAccount.host, secondaryAccount.login),
    secondaryAccount.login,
    secondaryAccount.host
  )
};

export const MOCK_INIT_INBOX: InitInboxResponse = {
  active_account_id: accountId(primaryAccount.host, primaryAccount.login),
  accounts: MOCK_ACCOUNTS,
  subscriptions: MOCK_SUBSCRIPTIONS,
  inbox: INBOX_BY_ACCOUNT[accountId(primaryAccount.host, primaryAccount.login)] ?? [],
  status: {
    rate_limits: [
      {
        account_id: accountId(primaryAccount.host, primaryAccount.login),
        resource: 'graphql',
        remaining: 4800,
        used: 200,
        limit_total: 5000,
        reset_at: BASE_TS + 3600,
        updated_at: BASE_TS + 200
      },
      {
        account_id: accountId(primaryAccount.host, primaryAccount.login),
        resource: 'core',
        remaining: 4500,
        used: 500,
        limit_total: 5000,
        reset_at: BASE_TS + 2400,
        updated_at: BASE_TS + 200
      }
    ],
    sync: {
      focus_state: 'focused',
      tiers: [
        {
          tier: 'hot',
          last_started_at: BASE_TS + 150,
          last_finished_at: BASE_TS + 151,
          last_error: null
        },
        {
          tier: 'warm',
          last_started_at: BASE_TS + 120,
          last_finished_at: BASE_TS + 122,
          last_error: null
        }
      ]
    }
  }
};

export function mockInboxForAccount(activeAccountId: string | null): InboxItem[] {
  if (!activeAccountId) {
    return Object.values(INBOX_BY_ACCOUNT)
      .flatMap((rows) => rows)
      .sort(
        (left, right) => right.updated_at - left.updated_at || left.pr_id.localeCompare(right.pr_id)
      );
  }
  return INBOX_BY_ACCOUNT[activeAccountId] ?? [];
}

export const MOCK_PR_DETAIL: PrDetailSummary = {
  account_id: accountId(primaryAccount.host, primaryAccount.login),
  pr_id: 'pr_1',
  repo_id: 'repo_1',
  pr_number: 1,
  title: 'Active Fixture PR Falcon Diff Stress',
  body: 'This active fixture PR drives read-only cockpit validation with synthetic diff and timeline.',
  state: 'open',
  draft: false,
  base_ref: 'main',
  base_sha: 'base_sha_0000000000000000000000000000000000000001',
  head_ref: 'feature/pr-1',
  head_sha: 'active_head_sha_000000000000000000000000000000000001',
  mergeable_state: 'clean',
  merge_state_status: 'behind',
  merge_commit_allowed: true,
  squash_merge_allowed: true,
  rebase_merge_allowed: true,
  delete_branch_on_merge_default: true,
  viewer_can_merge: true,
  viewer_can_enable_auto_merge: true,
  viewer_can_disable_auto_merge: true,
  viewer_can_update_branch: true,
  viewer_can_delete_head_ref: true,
  auto_merge_enabled: false,
  auto_merge_method: null,
  auto_merge_commit_headline: null,
  auto_merge_commit_body: null,
  auto_merge_enabled_by_login: null,
  auto_merge_enabled_at: null,
  merge_queue_entry_id: null,
  merge_queue_entry_position: null,
  merge_queue_entry_state: null,
  merge_queue_entry_estimated_ms: null,
  branch_protection_summary_json: JSON.stringify({
    requires_approving_reviews: true,
    required_approving_review_count: 1,
    requires_status_checks: true,
    required_status_check_contexts: ['ci-linux'],
    requires_strict_status_checks: true,
    restricts_pushes: false,
    restricts_review_dismissals: false
  }),
  repo_has_merge_queue: true,
  head_ref_state: 'ACTIVE',
  additions: 5200,
  deletions: 250,
  changed_files: 30,
  comment_count: 55,
  review_count: 10,
  thread_count: 5,
  check_run_count: 10,
  file_count: 30,
  updated_at: BASE_TS + 1,
  body_server_adjusted: false,
  pending_overlay: null
};

const RANGE_DIFF_BASE_SHA = 'base_sha_0000000000000000000000000000000000000001';
const RANGE_DIFF_OLD_SHA = '1111111111111111111111111111111111111111';
const RANGE_DIFF_NEW_SHA = '2222222222222222222222222222222222222222';

export const MOCK_PR_PUSHES: Record<string, PrPushView[]> = {
  pr_1: [
    {
      id: 1,
      pr_id: 'pr_1',
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      head_sha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      base_sha: RANGE_DIFF_BASE_SHA,
      observed_at: BASE_TS + 1000,
      push_kind: 'initial',
      supersedes_head_sha: null
    },
    {
      id: 2,
      pr_id: 'pr_1',
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      head_sha: RANGE_DIFF_OLD_SHA,
      base_sha: RANGE_DIFF_BASE_SHA,
      observed_at: BASE_TS + 1100,
      push_kind: 'force-push',
      supersedes_head_sha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    },
    {
      id: 3,
      pr_id: 'pr_1',
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      head_sha: RANGE_DIFF_NEW_SHA,
      base_sha: RANGE_DIFF_BASE_SHA,
      observed_at: BASE_TS + 1200,
      push_kind: 'force-push',
      supersedes_head_sha: RANGE_DIFF_OLD_SHA
    }
  ]
};

export function mockRangeDiff(
  mode: 'LocalGit' | 'RestCompare',
  baseSha = RANGE_DIFF_BASE_SHA,
  oldHeadSha = RANGE_DIFF_OLD_SHA,
  newHeadSha = RANGE_DIFF_NEW_SHA
): RangeDiff {
  const oldCommit = {
    sha: oldHeadSha,
    title: 'Adjust greeting wording',
    body: '',
    author_name: 'fixture-user',
    committed_at: BASE_TS + 1110,
    patch_hash: 'old-patch-hash',
    file_paths: ['src/example.txt'],
    additions: 1,
    deletions: 1
  };
  const newCommit = {
    sha: newHeadSha,
    title: 'Adjust greeting wording',
    body: '',
    author_name: 'fixture-user',
    committed_at: BASE_TS + 1210,
    patch_hash: 'new-patch-hash',
    file_paths: ['src/example.txt'],
    additions: 1,
    deletions: 1
  };
  return {
    old_range: {
      base_sha: baseSha,
      head_sha: oldHeadSha,
      commits: [oldCommit]
    },
    new_range: {
      base_sha: baseSha,
      head_sha: newHeadSha,
      commits: [newCommit]
    },
    commit_pairs: [
      {
        status: 'Modified',
        old: oldCommit,
        new: newCommit,
        intra_diff: {
          hunks: [
            {
              old_lines: [
                {
                  text: 'hello old world',
                  side: 'Old',
                  segments: [
                    { start: 0, end: 6, kind: 'Unchanged' },
                    { start: 6, end: 9, kind: 'Removed' },
                    { start: 9, end: 15, kind: 'Unchanged' }
                  ]
                }
              ],
              new_lines: [
                {
                  text: 'hello new world',
                  side: 'New',
                  segments: [
                    { start: 0, end: 6, kind: 'Unchanged' },
                    { start: 6, end: 9, kind: 'Added' },
                    { start: 9, end: 15, kind: 'Unchanged' }
                  ]
                }
              ]
            }
          ]
        }
      }
    ],
    mode
  };
}

export const MOCK_PR_METADATA: PrMetadataResponse = {
  labels: [
    {
      label_name: 'needs-review',
      label_color: 'fbca04',
      description: 'Needs review before merge',
      pending_overlay: null
    }
  ],
  assignees: [{ user_id: 'user_3', login: 'fixture-user-03', pending_overlay: null }],
  requested_reviewers: [
    {
      user_id: 'user_4',
      login: 'fixture-user-04',
      reviewer_type: 'user',
      reviewer_state: 'requested',
      requested_at: BASE_TS + 811,
      pending_overlay: null
    }
  ],
  projects: [
    {
      project_id: 'proj_1',
      project_title: 'Roadmap',
      item_id: 'item_1',
      status: 'In Review',
      updated_at: BASE_TS + 812,
      pending_overlay: null
    }
  ],
  milestones: [
    {
      milestone_id: 'mile_1',
      title: 'M1 cockpit',
      state: 'open',
      due_on: BASE_TS + 86_400,
      description: 'Milestone for read-only cockpit',
      pending_overlay: null
    }
  ]
};

export const MOCK_TIMELINE: TimelinePage = {
  items: Array.from({ length: 35 }, (_value, idx) => {
    const id = idx + 1;
    const isReview = id % 5 === 0;
    return {
      item_id: isReview ? `review_${id}` : `comment_${id}`,
      item_kind: isReview ? 'review' : id % 3 === 0 ? 'review_thread_reply' : 'issue',
      body: isReview
        ? `Review ${id} body mentions saturn gate and fixture quality.`
        : `Comment ${id} references nebula token and deterministic fixture flows.`,
      author_login: `fixture-user-${((id % 40) + 1).toString().padStart(2, '0')}`,
      created_at: BASE_TS + 500 + id,
      updated_at: BASE_TS + 500 + id,
      review_state: isReview ? (id % 2 === 0 ? 'COMMENTED' : 'CHANGES_REQUESTED') : null,
      body_server_adjusted: id % 7 === 0,
      pending_overlay: null
    };
  }),
  next_offset: null
};

export const MOCK_THREADS: ReviewThreadsPage = {
  threads: Array.from({ length: 5 }, (_value, idx) => {
    const id = idx + 1;
    return {
      id: `thread_${id}`,
      path: 'src/generated/huge_fixture.rs',
      line: 100 * id,
      side: 'RIGHT',
      start_line: 100 * id - 2,
      start_side: 'RIGHT',
      is_outdated: id % 2 === 0,
      is_resolved: id === 5,
      resolved_by_login: id === 5 ? 'fixture-user-02' : null,
      updated_at: BASE_TS + 350 + id,
      comment_count: id + 1,
      pending_overlay: null
    };
  }),
  next_offset: null
};

export const MOCK_CHECKS: PrCheckSummary = {
  total_runs: 4,
  successful_runs: 2,
  failed_runs: 1,
  pending_runs: 1,
  runs: [
    {
      id: 'run_suite_a_1',
      check_suite_id: 'suite_a',
      rest_id: 7001,
      name: 'lint',
      status: 'completed',
      conclusion: 'failure',
      details_url: 'https://github.com/fixture-org/repo-1/actions/runs/1/jobs/7001',
      started_at: BASE_TS + 720,
      completed_at: BASE_TS + 730,
      app_name: 'ci-linux',
      check_suite_status: 'completed',
      check_suite_conclusion: 'failure',
      check_run_head_sha: MOCK_PR_DETAIL.head_sha
    },
    {
      id: 'run_suite_a_2',
      check_suite_id: 'suite_a',
      rest_id: 7002,
      name: 'typecheck',
      status: 'completed',
      conclusion: 'neutral',
      details_url: 'https://github.com/fixture-org/repo-1/actions/runs/1/jobs/7002',
      started_at: BASE_TS + 721,
      completed_at: BASE_TS + 731,
      app_name: 'ci-linux',
      check_suite_status: 'completed',
      check_suite_conclusion: 'failure',
      check_run_head_sha: MOCK_PR_DETAIL.head_sha
    },
    {
      id: 'run_suite_b_1',
      check_suite_id: 'suite_b',
      rest_id: 7101,
      name: 'unit-tests',
      status: 'completed',
      conclusion: 'success',
      details_url: 'https://github.com/fixture-org/repo-1/actions/runs/2/jobs/7101',
      started_at: BASE_TS + 722,
      completed_at: BASE_TS + 732,
      app_name: 'ci-macos',
      check_suite_status: 'completed',
      check_suite_conclusion: 'success',
      check_run_head_sha: MOCK_PR_DETAIL.head_sha
    },
    {
      id: 'run_suite_c_1',
      check_suite_id: 'suite_c',
      rest_id: 7201,
      name: 'integration',
      status: 'queued',
      conclusion: null,
      details_url: 'https://github.com/fixture-org/repo-1/actions/runs/3/jobs/7201',
      started_at: BASE_TS + 723,
      completed_at: null,
      app_name: 'ci-windows',
      check_suite_status: 'in_progress',
      check_suite_conclusion: null,
      check_run_head_sha: MOCK_PR_DETAIL.head_sha
    }
  ]
};

export const MOCK_CHECK_ANNOTATIONS: CheckAnnotationView[] = [
  {
    annotation_id: 'run_suite_a_1:ann-12',
    check_run_id: 'run_suite_a_1',
    check_suite_id: 'suite_a',
    check_run_rest_id: 7001,
    check_run_name: 'lint',
    check_run_status: 'completed',
    check_run_conclusion: 'failure',
    check_run_details_url: 'https://github.com/fixture-org/repo-1/actions/runs/1/jobs/7001',
    check_run_head_sha: MOCK_PR_DETAIL.head_sha,
    is_outdated: false,
    path: 'src/foo.ts',
    start_line: 12,
    end_line: 12,
    start_column: 1,
    end_column: 8,
    annotation_level: 'failure',
    title: 'Lint failure',
    message: 'Unexpected any. Please provide a concrete type.',
    raw_details: 'eslint(no-explicit-any)',
    anchor_line: 12,
    anchor_side: 'RIGHT',
    anchor_path: 'src/foo.ts',
    anchor_signature_hash: 'mock-hash-12'
  },
  {
    annotation_id: 'run_suite_a_2:ann-28',
    check_run_id: 'run_suite_a_2',
    check_suite_id: 'suite_a',
    check_run_rest_id: 7002,
    check_run_name: 'typecheck',
    check_run_status: 'completed',
    check_run_conclusion: 'neutral',
    check_run_details_url: 'https://github.com/fixture-org/repo-1/actions/runs/1/jobs/7002',
    check_run_head_sha: MOCK_PR_DETAIL.head_sha,
    is_outdated: false,
    path: 'src/foo.ts',
    start_line: 28,
    end_line: 28,
    start_column: 1,
    end_column: 20,
    annotation_level: 'warning',
    title: 'Deprecated call',
    message: 'This helper is deprecated and will be removed.',
    raw_details: 'ts(6385)',
    anchor_line: 28,
    anchor_side: 'RIGHT',
    anchor_path: 'src/foo.ts',
    anchor_signature_hash: 'mock-hash-28'
  },
  {
    annotation_id: 'run_suite_b_1:ann-47',
    check_run_id: 'run_suite_b_1',
    check_suite_id: 'suite_b',
    check_run_rest_id: 7101,
    check_run_name: 'unit-tests',
    check_run_status: 'completed',
    check_run_conclusion: 'success',
    check_run_details_url: 'https://github.com/fixture-org/repo-1/actions/runs/2/jobs/7101',
    check_run_head_sha: 'outdated_head_sha_000000000000000000000000000000000001',
    is_outdated: true,
    path: 'src/foo.ts',
    start_line: 47,
    end_line: 47,
    start_column: 1,
    end_column: 10,
    annotation_level: 'notice',
    title: 'Coverage note',
    message: 'No assertion for the error branch.',
    raw_details: 'jest/expect-expect',
    anchor_line: 47,
    anchor_side: 'RIGHT',
    anchor_path: 'src/foo.ts',
    anchor_signature_hash: 'mock-hash-47'
  }
];

export const MOCK_FILES: PrFilesResponse = {
  files: [
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: 'src/generated/huge_fixture.rs',
      old_path: 'src/generated/huge_fixture.rs',
      previous_path: 'src/generated/huge_fixture.rs',
      status: 'modified',
      additions: 5000,
      deletions: 0,
      is_binary: false,
      kind: 'text',
      rename_similarity: null,
      is_viewed: true,
      patch_blob_sha: '356fb2a44e36283baed13a4f96756b377d5eec0cd8340a2f5a35eb0af7b118f6',
      viewed_by_account_id: accountId(primaryAccount.host, primaryAccount.login),
      viewed_at_head_sha: MOCK_PR_DETAIL.head_sha,
      pending_overlay: null
    },
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: 'src/foo.ts',
      old_path: 'src/foo.ts',
      previous_path: 'src/foo.ts',
      status: 'modified',
      additions: 60,
      deletions: 0,
      is_binary: false,
      kind: 'text',
      rename_similarity: null,
      is_viewed: false,
      patch_blob_sha: 'mock-foo-sha',
      viewed_by_account_id: null,
      viewed_at_head_sha: null,
      pending_overlay: null
    },
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: 'assets/test-pattern.png',
      old_path: 'assets/test-pattern.png',
      previous_path: 'assets/test-pattern.png',
      status: 'modified',
      additions: 1,
      deletions: 1,
      is_binary: true,
      kind: 'image',
      rename_similarity: null,
      is_viewed: false,
      patch_blob_sha: 'mock-image-sha',
      viewed_by_account_id: null,
      viewed_at_head_sha: null,
      pending_overlay: null
    },
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: 'assets/header.bin',
      old_path: null,
      previous_path: null,
      status: 'added',
      additions: 0,
      deletions: 0,
      is_binary: true,
      kind: 'binary',
      rename_similarity: null,
      is_viewed: false,
      patch_blob_sha: 'mock-binary-sha',
      viewed_by_account_id: null,
      viewed_at_head_sha: null,
      pending_overlay: null
    },
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: 'src/renamed/new_name.txt',
      old_path: 'src/renamed/old_name.txt',
      previous_path: 'src/renamed/old_name.txt',
      status: 'renamed',
      additions: 2,
      deletions: 2,
      is_binary: false,
      kind: 'text',
      rename_similarity: 95,
      is_viewed: false,
      patch_blob_sha: null,
      viewed_by_account_id: null,
      viewed_at_head_sha: null,
      pending_overlay: null
    },
    ...Array.from({ length: 29 }, (_value, idx) => ({
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      path: `src/module_${(idx + 1).toString().padStart(2, '0')}/file_${(idx + 1).toString().padStart(2, '0')}.ts`,
      old_path: null,
      previous_path: null,
      status: 'added',
      additions: 12 + idx,
      deletions: idx % 3,
      is_binary: false,
      kind: 'text',
      rename_similarity: null,
      is_viewed: false,
      patch_blob_sha: null,
      viewed_by_account_id: null,
      viewed_at_head_sha: null,
      pending_overlay: null
    }))
  ],
  tree: [
    {
      account_id: accountId(primaryAccount.host, primaryAccount.login),
      pr_id: 'pr_1',
      head_sha: MOCK_PR_DETAIL.head_sha,
      directory: 'src',
      file_count: 33,
      viewed_file_count: 1,
      additions: 5203,
      deletions: 253
    }
  ]
};

let patchCache: string | null = null;

function buildLargePatch(): string {
  if (patchCache) {
    return patchCache;
  }
  const lines = [
    'diff --git a/src/generated/huge_fixture.rs b/src/generated/huge_fixture.rs',
    'index 0000000..1111111 100644',
    '--- a/src/generated/huge_fixture.rs',
    '+++ b/src/generated/huge_fixture.rs',
    '@@ -0,0 +1,5000 @@'
  ];
  for (let idx = 1; idx <= 5000; idx += 1) {
    lines.push(
      `+pub const LINE_${idx.toString().padStart(4, '0')}: &str = "synthetic fixture line ${idx.toString().padStart(4, '0')}";`
    );
  }
  lines.push(
    'diff --git a/src/foo.ts b/src/foo.ts',
    'index 2222222..3333333 100644',
    '--- a/src/foo.ts',
    '+++ b/src/foo.ts',
    '@@ -0,0 +1,60 @@'
  );
  for (let idx = 1; idx <= 60; idx += 1) {
    lines.push(`+export const fooLine${idx} = ${idx};`);
  }
  patchCache = `${lines.join('\n')}\n`;
  return patchCache;
}

export function mockPatch(): PrPatchResponse {
  return {
    patch_blob_sha: '356fb2a44e36283baed13a4f96756b377d5eec0cd8340a2f5a35eb0af7b118f6',
    patch: buildLargePatch()
  };
}

export function mockStatus(activeAccountIdFilter: string | null): SystemStatusResponse {
  const accountIds =
    activeAccountIdFilter === null
      ? [
          accountId(primaryAccount.host, primaryAccount.login),
          accountId(secondaryAccount.host, secondaryAccount.login)
        ]
      : [activeAccountIdFilter];
  const rateLimits = accountIds.flatMap((account_id, index) => [
    {
      account_id,
      resource: 'graphql',
      remaining: index === 0 ? 4800 : 1400,
      used: index === 0 ? 200 : 3600,
      limit_total: 5000,
      reset_at: BASE_TS + 3600,
      updated_at: BASE_TS + 200
    },
    {
      account_id,
      resource: 'core',
      remaining: index === 0 ? 4200 : 900,
      used: index === 0 ? 800 : 4100,
      limit_total: 5000,
      reset_at: BASE_TS + 2400,
      updated_at: BASE_TS + 200
    }
  ]);
  return {
    rate_limits: rateLimits,
    sync: {
      focus_state: 'focused',
      tiers: [
        {
          tier: 'hot',
          last_started_at: BASE_TS + 150,
          last_finished_at: BASE_TS + 151,
          last_error: null
        }
      ]
    }
  };
}

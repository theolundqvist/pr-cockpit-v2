import type {
  AccountsListResponse,
  InitInboxResponse,
  InboxItem,
  PrCheckSummary,
  PrDetailSummary,
  PrFilesResponse,
  PrMetadataResponse,
  PrPatchResponse,
  RepoSubscriptionItem,
  ReviewThreadsPage,
  SystemStatusResponse,
  TimelinePage
} from '$lib/ipc/bindings';

const BASE_TS = 1_715_000_000;

const primaryAccount = {
  host: 'github.com',
  login: 'fixture-user',
  token_kind: 'pat',
  scopes: ['notifications', 'read:org', 'repo'],
  created_at: BASE_TS,
  updated_at: BASE_TS,
  is_active: true
};

const secondaryAccount = {
  host: 'github.enterprise.test',
  login: 'octo-enterprise',
  token_kind: 'oauth-device',
  scopes: ['notifications', 'repo'],
  created_at: BASE_TS,
  updated_at: BASE_TS,
  is_active: false
};

const accountId = (host: string, login: string): string => `${host}:${login}`;

function buildInbox(account_id: string): InboxItem[] {
  const rows: InboxItem[] = [];
  for (let idx = 1; idx <= 200; idx += 1) {
    const repoN = (idx % 3) + 1;
    rows.push({
      account_id,
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

export const MOCK_ACCOUNTS: AccountsListResponse = {
  active: { host: primaryAccount.host, login: primaryAccount.login },
  accounts: [primaryAccount, secondaryAccount]
};

const INBOX_BY_ACCOUNT: Record<string, InboxItem[]> = {
  [accountId(primaryAccount.host, primaryAccount.login)]: buildInbox(
    accountId(primaryAccount.host, primaryAccount.login)
  ),
  [accountId(secondaryAccount.host, secondaryAccount.login)]: buildInbox(
    accountId(secondaryAccount.host, secondaryAccount.login)
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
        limit_total: 5000,
        reset_at: BASE_TS + 3600,
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

export function mockInboxForAccount(activeAccountId: string): InboxItem[] {
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
  total_runs: 10,
  successful_runs: 10,
  failed_runs: 0,
  pending_runs: 0,
  runs: Array.from({ length: 10 }, (_value, idx) => ({
    id: `run_${Math.floor(idx / 5) + 1}_${(idx % 5) + 1}`,
    name: `suite-${Math.floor(idx / 5) + 1}-run-${(idx % 5) + 1}`,
    status: 'completed',
    conclusion: 'success',
    details_url: `https://example.test/check-runs/${Math.floor(idx / 5) + 1}/${(idx % 5) + 1}`,
    started_at: BASE_TS + 720 + idx,
    completed_at: BASE_TS + 730 + idx,
    app_name: idx < 5 ? 'ci-linux' : 'ci-macos'
  }))
};

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
  patchCache = `${lines.join('\n')}\n`;
  return patchCache;
}

export function mockPatch(): PrPatchResponse {
  return {
    patch_blob_sha: '356fb2a44e36283baed13a4f96756b377d5eec0cd8340a2f5a35eb0af7b118f6',
    patch: buildLargePatch()
  };
}

export function mockStatus(activeAccountId: string): SystemStatusResponse {
  return {
    rate_limits: [
      {
        account_id: activeAccountId,
        resource: 'graphql',
        remaining: 4800,
        limit_total: 5000,
        reset_at: BASE_TS + 3600,
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
        }
      ]
    }
  };
}

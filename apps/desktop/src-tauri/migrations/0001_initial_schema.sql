PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  host TEXT NOT NULL,
  login TEXT NOT NULL,
  token_kind TEXT NOT NULL CHECK (token_kind IN ('gh_cli', 'oauth_device', 'pat')),
  scopes TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(host, login)
);

CREATE TABLE IF NOT EXISTS repos (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  default_branch TEXT,
  description TEXT,
  html_url TEXT,
  is_private INTEGER NOT NULL DEFAULT 0 CHECK (is_private IN (0, 1)),
  is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
  pushed_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, owner, name)
);

CREATE TABLE IF NOT EXISTS repo_subscriptions (
  repo_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  watch_tier TEXT NOT NULL CHECK (watch_tier IN ('hot', 'warm', 'cool', 'cold')),
  last_full_sync_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(repo_id, account_id),
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  login TEXT NOT NULL,
  display_name TEXT,
  avatar_url TEXT,
  html_url TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, login)
);

CREATE TABLE IF NOT EXISTS orgs (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  login TEXT NOT NULL,
  display_name TEXT,
  avatar_url TEXT,
  html_url TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, login)
);

CREATE TABLE IF NOT EXISTS pull_requests (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  repo_id TEXT NOT NULL,
  number INTEGER NOT NULL,
  state TEXT NOT NULL,
  draft INTEGER NOT NULL DEFAULT 0 CHECK (draft IN (0, 1)),
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  author_id TEXT,
  base_ref TEXT NOT NULL,
  base_sha TEXT NOT NULL,
  head_ref TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  head_repo_id TEXT,
  mergeable_state TEXT,
  merge_state_status TEXT,
  additions INTEGER NOT NULL DEFAULT 0,
  deletions INTEGER NOT NULL DEFAULT 0,
  changed_files INTEGER NOT NULL DEFAULT 0,
  comments_count INTEGER NOT NULL DEFAULT 0,
  reviews_count INTEGER NOT NULL DEFAULT 0,
  commits_count INTEGER NOT NULL DEFAULT 0,
  is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
  html_url TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER,
  merged_at INTEGER,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(author_id) REFERENCES users(id) ON DELETE SET NULL,
  UNIQUE(account_id, repo_id, number)
);

CREATE TABLE IF NOT EXISTS pr_labels (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  label_name TEXT NOT NULL,
  label_color TEXT NOT NULL,
  description TEXT,
  PRIMARY KEY(account_id, pr_id, label_name),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_assignees (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  assigned_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, pr_id, user_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_reviewers (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  reviewer_type TEXT NOT NULL CHECK (reviewer_type IN ('user', 'team')),
  reviewer_state TEXT NOT NULL DEFAULT 'requested',
  requested_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, pr_id, user_id, reviewer_type),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_projects (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  project_title TEXT NOT NULL,
  item_id TEXT,
  status TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, pr_id, project_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_milestones (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  milestone_id TEXT NOT NULL,
  title TEXT NOT NULL,
  state TEXT NOT NULL,
  due_on INTEGER,
  description TEXT,
  PRIMARY KEY(account_id, pr_id, milestone_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS commits (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  repo_id TEXT NOT NULL,
  author_id TEXT,
  message_headline TEXT NOT NULL,
  message_body TEXT NOT NULL DEFAULT '',
  committed_at INTEGER NOT NULL,
  parents_json TEXT NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(author_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS pr_commits (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  commit_id TEXT NOT NULL,
  commit_order INTEGER NOT NULL,
  PRIMARY KEY(account_id, pr_id, commit_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(commit_id) REFERENCES commits(id) ON DELETE CASCADE,
  UNIQUE(account_id, pr_id, commit_order)
);

CREATE TABLE IF NOT EXISTS review_threads (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  path TEXT NOT NULL,
  line INTEGER,
  side TEXT CHECK (side IN ('LEFT', 'RIGHT')),
  start_line INTEGER,
  start_side TEXT CHECK (start_side IN ('LEFT', 'RIGHT')),
  original_commit_sha TEXT,
  original_path TEXT,
  original_position INTEGER,
  original_line INTEGER,
  is_outdated INTEGER NOT NULL DEFAULT 0 CHECK (is_outdated IN (0, 1)),
  is_resolved INTEGER NOT NULL DEFAULT 0 CHECK (is_resolved IN (0, 1)),
  resolved_by_id TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(resolved_by_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS reviews (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  author_id TEXT NOT NULL,
  state TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  commit_sha TEXT,
  submitted_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(author_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS comments (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('issue', 'review', 'review_thread_reply')),
  author_id TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER,
  in_reply_to_id TEXT,
  review_id TEXT,
  thread_id TEXT,
  path TEXT,
  line INTEGER,
  side TEXT CHECK (side IN ('LEFT', 'RIGHT')),
  start_line INTEGER,
  start_side TEXT CHECK (start_side IN ('LEFT', 'RIGHT')),
  original_commit_sha TEXT,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(author_id) REFERENCES users(id) ON DELETE CASCADE,
  FOREIGN KEY(in_reply_to_id) REFERENCES comments(id) ON DELETE SET NULL,
  FOREIGN KEY(review_id) REFERENCES reviews(id) ON DELETE SET NULL,
  FOREIGN KEY(thread_id) REFERENCES review_threads(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS check_suites (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  app_name TEXT NOT NULL,
  status TEXT NOT NULL,
  conclusion TEXT,
  details_url TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS check_runs (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  check_suite_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  name TEXT NOT NULL,
  status TEXT NOT NULL,
  conclusion TEXT,
  details_url TEXT,
  output_title TEXT,
  output_summary TEXT,
  started_at INTEGER,
  completed_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(check_suite_id) REFERENCES check_suites(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS check_annotations (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  check_run_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  path TEXT NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  start_column INTEGER,
  end_column INTEGER,
  annotation_level TEXT NOT NULL,
  title TEXT,
  message TEXT NOT NULL,
  raw_details TEXT,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(check_run_id) REFERENCES check_runs(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_files (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  old_path TEXT,
  status TEXT NOT NULL,
  additions INTEGER NOT NULL DEFAULT 0,
  deletions INTEGER NOT NULL DEFAULT 0,
  is_binary INTEGER NOT NULL DEFAULT 0 CHECK (is_binary IN (0, 1)),
  patch_blob_sha TEXT,
  viewed_by_account_id TEXT,
  viewed_at_head_sha TEXT,
  PRIMARY KEY(account_id, pr_id, head_sha, path),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pr_patches (
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  patch_blob_sha TEXT NOT NULL,
  fetched_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, pr_id, head_sha),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS notifications (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  repo_id TEXT NOT NULL,
  pr_id TEXT,
  reason TEXT NOT NULL,
  subject_type TEXT NOT NULL,
  subject_id TEXT NOT NULL,
  title TEXT NOT NULL,
  unread INTEGER NOT NULL CHECK (unread IN (0, 1)),
  updated_at INTEGER NOT NULL,
  last_read_at INTEGER,
  url TEXT,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS worktrees (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  repo_id TEXT NOT NULL,
  path TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  branch TEXT NOT NULL,
  dirty INTEGER NOT NULL CHECK (dirty IN (0, 1)),
  ahead INTEGER NOT NULL DEFAULT 0,
  behind INTEGER NOT NULL DEFAULT 0,
  mapped_pr_id TEXT,
  mapping_confidence REAL,
  mapping_source TEXT,
  is_app_managed INTEGER NOT NULL DEFAULT 0 CHECK (is_app_managed IN (0, 1)),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(mapped_pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS pending_mutations (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  input_json TEXT NOT NULL,
  optimistic_patch_json TEXT NOT NULL,
  inverse_patch_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'applied', 'failed', 'discarded')),
  retries INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_error TEXT,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(idempotency_key)
);

CREATE TABLE IF NOT EXISTS id_mappings (
  account_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  local_id TEXT NOT NULL,
  server_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, kind, local_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, kind, server_id)
);

CREATE TABLE IF NOT EXISTS sync_cursors (
  account_id TEXT NOT NULL,
  resource TEXT NOT NULL,
  cursor TEXT,
  etag TEXT,
  last_fetched_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, resource),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS rate_limit_buckets (
  account_id TEXT NOT NULL,
  resource TEXT NOT NULL,
  remaining INTEGER NOT NULL,
  limit_total INTEGER NOT NULL,
  reset_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(account_id, resource),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS blob_refs (
  sha256 TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('patch', 'markdown_html', 'check_log', 'asset')),
  size INTEGER NOT NULL,
  ref_count INTEGER NOT NULL,
  last_accessed_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repos_account_owner_name ON repos(account_id, owner, name);
CREATE INDEX IF NOT EXISTS idx_pull_requests_repo_state ON pull_requests(account_id, repo_id, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_pull_requests_account_updated ON pull_requests(account_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_comments_pr_created ON comments(account_id, pr_id, created_at);
CREATE INDEX IF NOT EXISTS idx_review_threads_pr ON review_threads(account_id, pr_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_reviews_pr ON reviews(account_id, pr_id, submitted_at DESC);
CREATE INDEX IF NOT EXISTS idx_check_suites_pr ON check_suites(account_id, pr_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_check_runs_pr ON check_runs(account_id, pr_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_pr_files_pr ON pr_files(account_id, pr_id, head_sha, path);
CREATE INDEX IF NOT EXISTS idx_notifications_account_unread ON notifications(account_id, unread, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_worktrees_repo ON worktrees(account_id, repo_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_pending_mutations_account_status ON pending_mutations(account_id, status, created_at);
CREATE INDEX IF NOT EXISTS idx_blob_refs_lru ON blob_refs(last_accessed_at ASC, ref_count ASC);

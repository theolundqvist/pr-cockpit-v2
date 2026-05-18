PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS stacks (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('linear', 'dag')),
  warning_json TEXT,
  head_pr_id TEXT,
  base_branch TEXT,
  detected_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(head_pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS pr_stack_position (
  pr_id TEXT PRIMARY KEY,
  stack_id TEXT NOT NULL,
  position INTEGER NOT NULL,
  parent_pr_id TEXT,
  blocked_by_json TEXT,
  computed_at INTEGER NOT NULL,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(stack_id) REFERENCES stacks(id) ON DELETE CASCADE,
  FOREIGN KEY(parent_pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS stack_operations (
  id TEXT PRIMARY KEY,
  stack_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  op_kind TEXT NOT NULL CHECK (op_kind IN ('rebase', 'merge')),
  status TEXT NOT NULL CHECK (
    status IN ('pending', 'running', 'paused_conflict', 'paused_failure', 'succeeded', 'aborted')
  ),
  current_pr_id TEXT,
  current_step INTEGER,
  total_steps INTEGER,
  worktree_path TEXT,
  conflict_files_json TEXT,
  last_error TEXT,
  started_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  finished_at INTEGER,
  FOREIGN KEY(stack_id) REFERENCES stacks(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(current_pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_stacks_repo_account ON stacks(repo_id, account_id);
CREATE INDEX IF NOT EXISTS idx_pr_stack_position_stack_position ON pr_stack_position(stack_id, position);
CREATE INDEX IF NOT EXISTS idx_stack_operations_stack_status ON stack_operations(stack_id, status);

CREATE VIEW IF NOT EXISTS pr_stack_summary AS
SELECT
  inbox.account_id,
  inbox.account_login,
  inbox.account_host,
  inbox.pr_id,
  inbox.repo_id,
  inbox.repo_owner,
  inbox.repo_name,
  inbox.pr_number,
  inbox.title,
  inbox.state,
  inbox.draft,
  inbox.head_sha,
  inbox.base_sha,
  inbox.mergeable_state,
  inbox.merge_state_status,
  inbox.updated_at,
  inbox.author_login,
  inbox.unread_notification_count,
  inbox.latest_notification_at,
  stack_pos.stack_id,
  stack_pos.position AS stack_position,
  stack_pos.parent_pr_id,
  stack_pos.blocked_by_json,
  stack_pos.computed_at AS stack_computed_at,
  stacks.kind AS stack_kind,
  stacks.warning_json AS stack_warning_json,
  stacks.head_pr_id AS stack_head_pr_id,
  stacks.base_branch AS stack_base_branch,
  stacks.detected_at AS stack_detected_at,
  stacks.updated_at AS stack_updated_at
FROM pr_inbox_rows inbox
JOIN pr_stack_position stack_pos ON stack_pos.pr_id = inbox.pr_id
JOIN stacks ON stacks.id = stack_pos.stack_id;

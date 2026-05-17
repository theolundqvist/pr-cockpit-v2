CREATE TABLE IF NOT EXISTS worktree_settings (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  value_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

ALTER TABLE worktrees ADD COLUMN manual_override_pr_id TEXT;
ALTER TABLE worktrees ADD COLUMN manual_override_at INTEGER;
ALTER TABLE worktrees ADD COLUMN last_cleanup_snapshot_id TEXT;
ALTER TABLE worktrees ADD COLUMN untracked_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE worktrees ADD COLUMN staged_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE worktrees ADD COLUMN modified_count INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX IF NOT EXISTS idx_worktrees_account_path
ON worktrees(account_id, path);

CREATE INDEX IF NOT EXISTS idx_worktrees_account_mapped_pr
ON worktrees(account_id, mapped_pr_id);

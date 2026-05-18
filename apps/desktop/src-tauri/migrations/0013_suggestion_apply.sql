CREATE TABLE IF NOT EXISTS suggestion_applies (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  mutation_id TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('single', 'batch')),
  commit_sha TEXT,
  head_sha_before TEXT,
  head_sha_after TEXT,
  suggestion_comment_ids TEXT NOT NULL,
  applied_at INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('applied', 'conflict', 'aborted', 'rejected')),
  error_kind TEXT
);

CREATE INDEX IF NOT EXISTS idx_suggestion_applies_account_pr
  ON suggestion_applies(account_id, pr_id, applied_at DESC);

DROP VIEW IF EXISTS suggestion_blocks;
CREATE VIEW suggestion_blocks AS
SELECT
  c.account_id AS account_id,
  c.pr_id AS pr_id,
  c.id AS comment_id,
  c.path AS path,
  c.body AS body,
  c.line AS line,
  c.start_line AS start_line,
  c.side AS side,
  c.original_commit_sha AS original_commit_sha,
  u.login AS suggestion_author_login,
  COALESCE(rt.is_outdated, 0) AS is_outdated
FROM comments c
LEFT JOIN users u ON u.id = c.author_id
LEFT JOIN review_threads rt ON rt.id = c.thread_id AND rt.account_id = c.account_id
WHERE c.deleted_at IS NULL
  AND c.path IS NOT NULL
  AND c.kind IN ('review', 'review_thread_reply')
  AND c.body LIKE '%```suggestion%';

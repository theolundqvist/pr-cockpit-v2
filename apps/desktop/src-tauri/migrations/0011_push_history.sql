CREATE TABLE IF NOT EXISTS pr_pushes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pr_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT NOT NULL,
  observed_at INTEGER NOT NULL,
  push_kind TEXT NOT NULL CHECK (push_kind IN ('initial', 'force-push', 'fast-forward', 'merge-back')),
  supersedes_head_sha TEXT,
  UNIQUE(pr_id, head_sha),
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pr_pushes_pr_observed_at
  ON pr_pushes(pr_id, observed_at DESC, id DESC);

DROP VIEW IF EXISTS pr_force_push_pairs;
CREATE VIEW pr_force_push_pairs AS
WITH ordered_pushes AS (
  SELECT
    id,
    pr_id,
    base_sha,
    head_sha,
    observed_at,
    push_kind,
    LAG(head_sha) OVER (PARTITION BY pr_id ORDER BY observed_at ASC, id ASC) AS old_head_sha
  FROM pr_pushes
)
SELECT
  pr_id,
  base_sha,
  old_head_sha,
  head_sha AS new_head_sha,
  observed_at AS occurred_at
FROM ordered_pushes
WHERE old_head_sha IS NOT NULL
  AND push_kind = 'force-push';

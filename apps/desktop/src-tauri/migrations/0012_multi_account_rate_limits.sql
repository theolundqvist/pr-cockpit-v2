-- `limit_total` already exists in 0001; this migration only backfills the new `used` column.
ALTER TABLE rate_limit_buckets ADD COLUMN used INTEGER;

UPDATE rate_limit_buckets
SET limit_total = remaining
WHERE limit_total <= 0;

CREATE UNIQUE INDEX IF NOT EXISTS idx_rate_limit_buckets_account_resource
  ON rate_limit_buckets(account_id, resource);

DROP VIEW IF EXISTS pr_inbox_rows;
CREATE VIEW pr_inbox_rows AS
WITH unread AS (
  SELECT
    account_id,
    pr_id,
    COUNT(*) AS unread_count,
    MAX(updated_at) AS latest_notification_at
  FROM notifications
  WHERE unread = 1
  GROUP BY account_id, pr_id
)
SELECT
  pr.account_id,
  a.login AS account_login,
  a.host AS account_host,
  pr.id AS pr_id,
  pr.repo_id,
  r.owner AS repo_owner,
  r.name AS repo_name,
  pr.number AS pr_number,
  pr.title,
  pr.state,
  pr.draft,
  pr.head_sha,
  pr.base_sha,
  pr.mergeable_state,
  pr.merge_state_status,
  pr.updated_at,
  COALESCE(u.login, 'unknown') AS author_login,
  COALESCE(unread.unread_count, 0) AS unread_notification_count,
  unread.latest_notification_at,
  pr.pending_state AS pending_overlay
FROM pull_requests pr
JOIN repos r ON r.id = pr.repo_id
JOIN accounts a ON a.id = pr.account_id
LEFT JOIN users u ON u.id = pr.author_id
LEFT JOIN unread ON unread.account_id = pr.account_id AND unread.pr_id = pr.id
WHERE pr.state = 'open';

DROP VIEW IF EXISTS account_rate_limits;
CREATE VIEW account_rate_limits AS
SELECT
  account_id,
  resource,
  remaining,
  used,
  limit_total,
  reset_at,
  updated_at
FROM rate_limit_buckets;

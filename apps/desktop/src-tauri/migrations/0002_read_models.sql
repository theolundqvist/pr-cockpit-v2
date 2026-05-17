CREATE VIEW IF NOT EXISTS pr_inbox_rows AS
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
  unread.latest_notification_at
FROM pull_requests pr
JOIN repos r ON r.id = pr.repo_id
LEFT JOIN users u ON u.id = pr.author_id
LEFT JOIN unread ON unread.account_id = pr.account_id AND unread.pr_id = pr.id
WHERE pr.state = 'open';

CREATE VIEW IF NOT EXISTS pr_detail_summary AS
SELECT
  pr.account_id,
  pr.id AS pr_id,
  pr.repo_id,
  pr.number AS pr_number,
  pr.title,
  pr.body,
  pr.state,
  pr.draft,
  pr.base_ref,
  pr.base_sha,
  pr.head_ref,
  pr.head_sha,
  pr.mergeable_state,
  pr.merge_state_status,
  pr.additions,
  pr.deletions,
  pr.changed_files,
  (
    SELECT COUNT(*)
    FROM comments c
    WHERE c.account_id = pr.account_id AND c.pr_id = pr.id
  ) AS comment_count,
  (
    SELECT COUNT(*)
    FROM reviews rv
    WHERE rv.account_id = pr.account_id AND rv.pr_id = pr.id
  ) AS review_count,
  (
    SELECT COUNT(*)
    FROM review_threads rt
    WHERE rt.account_id = pr.account_id AND rt.pr_id = pr.id
  ) AS thread_count,
  (
    SELECT COUNT(*)
    FROM check_runs cr
    WHERE cr.account_id = pr.account_id AND cr.pr_id = pr.id
  ) AS check_run_count,
  (
    SELECT COUNT(*)
    FROM pr_files pf
    WHERE pf.account_id = pr.account_id AND pf.pr_id = pr.id AND pf.head_sha = pr.head_sha
  ) AS file_count,
  pr.updated_at
FROM pull_requests pr;

CREATE VIEW IF NOT EXISTS unread_counts AS
SELECT
  account_id,
  COUNT(*) AS total_notifications,
  SUM(CASE WHEN unread = 1 THEN 1 ELSE 0 END) AS unread_notifications,
  COUNT(DISTINCT CASE WHEN unread = 1 THEN pr_id END) AS prs_with_unread
FROM notifications
GROUP BY account_id;

CREATE VIEW IF NOT EXISTS file_tree_summary AS
SELECT
  account_id,
  pr_id,
  head_sha,
  CASE
    WHEN instr(path, '/') > 0 THEN substr(path, 1, instr(path, '/') - 1)
    ELSE '.'
  END AS directory,
  COUNT(*) AS file_count,
  SUM(additions) AS additions,
  SUM(deletions) AS deletions
FROM pr_files
GROUP BY account_id, pr_id, head_sha, directory;

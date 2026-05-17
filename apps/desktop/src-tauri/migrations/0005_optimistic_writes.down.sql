DROP VIEW IF EXISTS pr_inbox_rows;
DROP VIEW IF EXISTS pr_detail_summary;
DROP VIEW IF EXISTS unread_counts;
DROP VIEW IF EXISTS file_tree_summary;

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

CREATE VIEW pr_detail_summary AS
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

CREATE VIEW unread_counts AS
SELECT
  account_id,
  COUNT(*) AS total_notifications,
  SUM(CASE WHEN unread = 1 THEN 1 ELSE 0 END) AS unread_notifications,
  COUNT(DISTINCT CASE WHEN unread = 1 THEN pr_id END) AS prs_with_unread
FROM notifications
GROUP BY account_id;

CREATE VIEW file_tree_summary AS
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

DROP INDEX IF EXISTS idx_mutation_attempts_outcome;
DROP INDEX IF EXISTS idx_pull_requests_pending_state;
DROP INDEX IF EXISTS idx_comments_pending_state;
DROP INDEX IF EXISTS idx_reviews_pending_state;
DROP INDEX IF EXISTS idx_review_threads_pending_state;
DROP INDEX IF EXISTS idx_pr_labels_pending_state;
DROP INDEX IF EXISTS idx_pr_assignees_pending_state;
DROP INDEX IF EXISTS idx_pr_reviewers_pending_state;
DROP INDEX IF EXISTS idx_pr_projects_pending_state;
DROP INDEX IF EXISTS idx_pr_milestones_pending_state;
DROP INDEX IF EXISTS idx_pr_files_pending_state;

DROP TABLE IF EXISTS mutation_attempts;

ALTER TABLE pending_mutations DROP COLUMN server_call_json;
ALTER TABLE pending_mutations DROP COLUMN optimism_level;

ALTER TABLE pr_files DROP COLUMN pending_state;
ALTER TABLE pr_milestones DROP COLUMN pending_state;
ALTER TABLE pr_projects DROP COLUMN pending_state;
ALTER TABLE pr_reviewers DROP COLUMN pending_state;
ALTER TABLE pr_assignees DROP COLUMN pending_state;
ALTER TABLE pr_labels DROP COLUMN pending_state;
ALTER TABLE review_threads DROP COLUMN pending_state;
ALTER TABLE reviews DROP COLUMN pending_state;
ALTER TABLE comments DROP COLUMN pending_state;
ALTER TABLE pull_requests DROP COLUMN pending_state;

ALTER TABLE pull_requests DROP COLUMN server_adjusted_at;
ALTER TABLE pull_requests DROP COLUMN body_server_adjusted;
ALTER TABLE reviews DROP COLUMN server_adjusted_at;
ALTER TABLE reviews DROP COLUMN body_server_adjusted;
ALTER TABLE comments DROP COLUMN server_adjusted_at;
ALTER TABLE comments DROP COLUMN body_server_adjusted;

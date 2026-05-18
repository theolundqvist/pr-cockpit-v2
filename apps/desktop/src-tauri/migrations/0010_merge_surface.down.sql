DROP VIEW IF EXISTS pr_detail_summary;

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
  pr.updated_at,
  COALESCE(
    pr.pending_state,
    (
      SELECT c.pending_state
      FROM comments c
      WHERE c.account_id = pr.account_id
        AND c.pr_id = pr.id
        AND c.pending_state IS NOT NULL
      ORDER BY c.updated_at DESC
      LIMIT 1
    )
  ) AS pending_overlay
FROM pull_requests pr;

ALTER TABLE pull_requests DROP COLUMN head_ref_state;
ALTER TABLE pull_requests DROP COLUMN repo_has_merge_queue;
ALTER TABLE pull_requests DROP COLUMN branch_protection_summary_json;
ALTER TABLE pull_requests DROP COLUMN merge_queue_entry_estimated_ms;
ALTER TABLE pull_requests DROP COLUMN merge_queue_entry_state;
ALTER TABLE pull_requests DROP COLUMN merge_queue_entry_position;
ALTER TABLE pull_requests DROP COLUMN merge_queue_entry_id;
ALTER TABLE pull_requests DROP COLUMN auto_merge_enabled_at;
ALTER TABLE pull_requests DROP COLUMN auto_merge_enabled_by_login;
ALTER TABLE pull_requests DROP COLUMN auto_merge_commit_body;
ALTER TABLE pull_requests DROP COLUMN auto_merge_commit_headline;
ALTER TABLE pull_requests DROP COLUMN auto_merge_method;
ALTER TABLE pull_requests DROP COLUMN auto_merge_enabled;
ALTER TABLE pull_requests DROP COLUMN viewer_can_delete_head_ref;
ALTER TABLE pull_requests DROP COLUMN viewer_can_update_branch;
ALTER TABLE pull_requests DROP COLUMN viewer_can_disable_auto_merge;
ALTER TABLE pull_requests DROP COLUMN viewer_can_enable_auto_merge;
ALTER TABLE pull_requests DROP COLUMN viewer_can_merge;
ALTER TABLE pull_requests DROP COLUMN delete_branch_on_merge_default;
ALTER TABLE pull_requests DROP COLUMN rebase_merge_allowed;
ALTER TABLE pull_requests DROP COLUMN squash_merge_allowed;
ALTER TABLE pull_requests DROP COLUMN merge_commit_allowed;

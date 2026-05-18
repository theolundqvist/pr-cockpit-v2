ALTER TABLE pull_requests ADD COLUMN merge_commit_allowed INTEGER;
ALTER TABLE pull_requests ADD COLUMN squash_merge_allowed INTEGER;
ALTER TABLE pull_requests ADD COLUMN rebase_merge_allowed INTEGER;
ALTER TABLE pull_requests ADD COLUMN delete_branch_on_merge_default INTEGER;
ALTER TABLE pull_requests ADD COLUMN viewer_can_merge INTEGER;
ALTER TABLE pull_requests ADD COLUMN viewer_can_enable_auto_merge INTEGER;
ALTER TABLE pull_requests ADD COLUMN viewer_can_disable_auto_merge INTEGER;
ALTER TABLE pull_requests ADD COLUMN viewer_can_update_branch INTEGER;
ALTER TABLE pull_requests ADD COLUMN viewer_can_delete_head_ref INTEGER;
ALTER TABLE pull_requests ADD COLUMN auto_merge_enabled INTEGER;
ALTER TABLE pull_requests ADD COLUMN auto_merge_method TEXT;
ALTER TABLE pull_requests ADD COLUMN auto_merge_commit_headline TEXT;
ALTER TABLE pull_requests ADD COLUMN auto_merge_commit_body TEXT;
ALTER TABLE pull_requests ADD COLUMN auto_merge_enabled_by_login TEXT;
ALTER TABLE pull_requests ADD COLUMN auto_merge_enabled_at INTEGER;
ALTER TABLE pull_requests ADD COLUMN merge_queue_entry_id TEXT;
ALTER TABLE pull_requests ADD COLUMN merge_queue_entry_position INTEGER;
ALTER TABLE pull_requests ADD COLUMN merge_queue_entry_state TEXT;
ALTER TABLE pull_requests ADD COLUMN merge_queue_entry_estimated_ms INTEGER;
ALTER TABLE pull_requests ADD COLUMN branch_protection_summary_json TEXT;
ALTER TABLE pull_requests ADD COLUMN repo_has_merge_queue INTEGER;
ALTER TABLE pull_requests ADD COLUMN head_ref_state TEXT;

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
  pr.merge_commit_allowed,
  pr.squash_merge_allowed,
  pr.rebase_merge_allowed,
  pr.delete_branch_on_merge_default,
  pr.viewer_can_merge,
  pr.viewer_can_enable_auto_merge,
  pr.viewer_can_disable_auto_merge,
  pr.viewer_can_update_branch,
  pr.viewer_can_delete_head_ref,
  pr.auto_merge_enabled,
  pr.auto_merge_method,
  pr.auto_merge_commit_headline,
  pr.auto_merge_commit_body,
  pr.auto_merge_enabled_by_login,
  pr.auto_merge_enabled_at,
  pr.merge_queue_entry_id,
  pr.merge_queue_entry_position,
  pr.merge_queue_entry_state,
  pr.merge_queue_entry_estimated_ms,
  pr.branch_protection_summary_json,
  pr.repo_has_merge_queue,
  pr.head_ref_state,
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

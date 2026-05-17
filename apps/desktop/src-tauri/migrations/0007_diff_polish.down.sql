DROP VIEW IF EXISTS file_tree_summary;
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
  SUM(deletions) AS deletions,
  MAX(pending_state) AS pending_overlay
FROM pr_files
GROUP BY account_id, pr_id, head_sha, directory;

ALTER TABLE pr_files DROP COLUMN rename_similarity;
ALTER TABLE pr_files DROP COLUMN kind;
ALTER TABLE pr_files DROP COLUMN previous_path;

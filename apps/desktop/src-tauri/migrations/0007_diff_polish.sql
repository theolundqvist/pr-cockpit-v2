ALTER TABLE pr_files ADD COLUMN previous_path TEXT;
UPDATE pr_files
SET previous_path = old_path
WHERE previous_path IS NULL;

ALTER TABLE pr_files ADD COLUMN rename_similarity REAL;

ALTER TABLE pr_files ADD COLUMN kind TEXT NOT NULL DEFAULT 'text' CHECK (kind IN ('text', 'image', 'binary'));
UPDATE pr_files
SET kind = CASE
  WHEN is_binary = 1 THEN 'binary'
  ELSE 'text'
END
WHERE kind IS NULL OR kind = '';

DROP VIEW IF EXISTS file_tree_summary;
CREATE VIEW file_tree_summary AS
SELECT
  pf.account_id,
  pf.pr_id,
  pf.head_sha,
  CASE
    WHEN instr(pf.path, '/') > 0 THEN substr(pf.path, 1, instr(pf.path, '/') - 1)
    ELSE '.'
  END AS directory,
  COUNT(*) AS file_count,
  SUM(
    CASE
      WHEN pf.viewed_by_account_id IS NOT NULL AND pf.viewed_at_head_sha = pr.head_sha THEN 1
      ELSE 0
    END
  ) AS viewed_file_count,
  SUM(pf.additions) AS additions,
  SUM(pf.deletions) AS deletions,
  MAX(pf.pending_state) AS pending_overlay
FROM pr_files pf
JOIN pull_requests pr
  ON pr.account_id = pf.account_id AND pr.id = pf.pr_id
GROUP BY pf.account_id, pf.pr_id, pf.head_sha, directory;

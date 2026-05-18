ALTER TABLE check_runs
ADD COLUMN rest_id INTEGER;

CREATE TABLE IF NOT EXISTS check_annotation_aux (
  annotation_id TEXT PRIMARY KEY,
  check_run_id TEXT NOT NULL,
  pr_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  anchor_line INTEGER NOT NULL,
  anchor_side TEXT NOT NULL CHECK (anchor_side IN ('LEFT', 'RIGHT')),
  anchor_path TEXT NOT NULL,
  anchor_signature_hash TEXT NOT NULL,
  FOREIGN KEY(annotation_id) REFERENCES check_annotations(id) ON DELETE CASCADE,
  FOREIGN KEY(check_run_id) REFERENCES check_runs(id) ON DELETE CASCADE,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE CASCADE,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_check_annotation_aux_pr_path_line
  ON check_annotation_aux(pr_id, anchor_path, anchor_line);

CREATE INDEX IF NOT EXISTS idx_check_annotation_aux_check_run
  ON check_annotation_aux(check_run_id);

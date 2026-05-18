PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS image_uploads (
  sha256 TEXT NOT NULL,
  account_id TEXT NOT NULL,
  url TEXT NOT NULL,
  mime TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  uploaded_at INTEGER NOT NULL,
  PRIMARY KEY (sha256, account_id),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_image_uploads_account_uploaded
  ON image_uploads(account_id, uploaded_at DESC);

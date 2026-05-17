ALTER TABLE pending_mutations
ADD COLUMN requires_connection_confirmation INTEGER NOT NULL DEFAULT 0
CHECK (requires_connection_confirmation IN (0, 1));

CREATE INDEX IF NOT EXISTS idx_pending_mutations_requires_confirmation
ON pending_mutations(account_id, status, requires_connection_confirmation, created_at);

CREATE TABLE IF NOT EXISTS drafts (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_drafts_account_target
ON drafts(account_id, target_type, target_id, updated_at DESC);

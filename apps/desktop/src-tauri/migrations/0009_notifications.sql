CREATE TABLE IF NOT EXISTS notification_events (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  repo_id TEXT,
  pr_id TEXT,
  event_type TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  server_event_id TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  seen_at INTEGER,
  fired_at INTEGER NOT NULL,
  deduped INTEGER NOT NULL DEFAULT 0 CHECK (deduped IN (0, 1)),
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE SET NULL,
  FOREIGN KEY(pr_id) REFERENCES pull_requests(id) ON DELETE SET NULL,
  UNIQUE(account_id, repo_id, pr_id, event_type, actor_id, server_event_id)
);

CREATE TABLE IF NOT EXISTS notification_rules (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  config_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, kind)
);

CREATE TABLE IF NOT EXISTS notification_settings (
  account_id TEXT PRIMARY KEY,
  quiet_hours_json TEXT,
  focus_mode INTEGER NOT NULL DEFAULT 0 CHECK (focus_mode IN (0, 1)),
  per_repo_filters_json TEXT NOT NULL DEFAULT '{"allow":[],"deny":[]}',
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notification_events_account_fired
  ON notification_events(account_id, fired_at DESC);

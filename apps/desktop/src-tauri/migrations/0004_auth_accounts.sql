-- no-transaction

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS accounts_next (
  id TEXT PRIMARY KEY,
  host TEXT NOT NULL,
  login TEXT NOT NULL,
  token_kind TEXT NOT NULL CHECK (token_kind IN ('gh-cli', 'oauth-device', 'pat')),
  scopes TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(host, login)
);

INSERT INTO accounts_next(id, host, login, token_kind, scopes, created_at, updated_at)
SELECT
  id,
  host,
  login,
  CASE token_kind
    WHEN 'gh_cli' THEN 'gh-cli'
    WHEN 'oauth_device' THEN 'oauth-device'
    ELSE token_kind
  END,
  scopes,
  created_at,
  updated_at
FROM accounts;

DROP TABLE accounts;
ALTER TABLE accounts_next RENAME TO accounts;

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

PRAGMA foreign_keys = ON;

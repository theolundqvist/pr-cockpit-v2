CREATE TABLE IF NOT EXISTS search_documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id TEXT NOT NULL,
  pr_id TEXT,
  doc_type TEXT NOT NULL CHECK (doc_type IN ('pr', 'comment', 'review', 'file')),
  doc_ref TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL DEFAULT '',
  filename TEXT NOT NULL DEFAULT '',
  author TEXT NOT NULL DEFAULT '',
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE,
  UNIQUE(account_id, doc_type, doc_ref)
);

CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
  title,
  body,
  filename,
  author,
  content = 'search_documents',
  content_rowid = 'id',
  tokenize = 'unicode61'
);

CREATE TRIGGER IF NOT EXISTS search_documents_ai AFTER INSERT ON search_documents BEGIN
  INSERT INTO search_fts(rowid, title, body, filename, author)
  VALUES (new.id, new.title, new.body, new.filename, new.author);
END;

CREATE TRIGGER IF NOT EXISTS search_documents_ad AFTER DELETE ON search_documents BEGIN
  INSERT INTO search_fts(search_fts, rowid, title, body, filename, author)
  VALUES ('delete', old.id, old.title, old.body, old.filename, old.author);
END;

CREATE TRIGGER IF NOT EXISTS search_documents_au AFTER UPDATE ON search_documents BEGIN
  INSERT INTO search_fts(search_fts, rowid, title, body, filename, author)
  VALUES ('delete', old.id, old.title, old.body, old.filename, old.author);
  INSERT INTO search_fts(rowid, title, body, filename, author)
  VALUES (new.id, new.title, new.body, new.filename, new.author);
END;

CREATE TRIGGER IF NOT EXISTS pull_requests_ai AFTER INSERT ON pull_requests BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.id,
    'pr',
    new.id,
    new.title,
    new.body,
    '',
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    title = excluded.title,
    body = excluded.body,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS pull_requests_au AFTER UPDATE ON pull_requests BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.id,
    'pr',
    new.id,
    new.title,
    new.body,
    '',
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    title = excluded.title,
    body = excluded.body,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS pull_requests_ad AFTER DELETE ON pull_requests BEGIN
  DELETE FROM search_documents
  WHERE account_id = old.account_id AND doc_type = 'pr' AND doc_ref = old.id;
END;

CREATE TRIGGER IF NOT EXISTS comments_ai AFTER INSERT ON comments BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'comment',
    new.id,
    '',
    new.body,
    COALESCE(new.path, ''),
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    pr_id = excluded.pr_id,
    body = excluded.body,
    filename = excluded.filename,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS comments_au AFTER UPDATE ON comments BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'comment',
    new.id,
    '',
    new.body,
    COALESCE(new.path, ''),
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    pr_id = excluded.pr_id,
    body = excluded.body,
    filename = excluded.filename,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS comments_ad AFTER DELETE ON comments BEGIN
  DELETE FROM search_documents
  WHERE account_id = old.account_id AND doc_type = 'comment' AND doc_ref = old.id;
END;

CREATE TRIGGER IF NOT EXISTS reviews_ai AFTER INSERT ON reviews BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'review',
    new.id,
    '',
    new.body,
    '',
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    pr_id = excluded.pr_id,
    body = excluded.body,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS reviews_au AFTER UPDATE ON reviews BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'review',
    new.id,
    '',
    new.body,
    '',
    COALESCE((SELECT login FROM users u WHERE u.id = new.author_id AND u.account_id = new.account_id), ''),
    new.updated_at
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    pr_id = excluded.pr_id,
    body = excluded.body,
    author = excluded.author,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS reviews_ad AFTER DELETE ON reviews BEGIN
  DELETE FROM search_documents
  WHERE account_id = old.account_id AND doc_type = 'review' AND doc_ref = old.id;
END;

CREATE TRIGGER IF NOT EXISTS pr_files_ai AFTER INSERT ON pr_files BEGIN
  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'file',
    new.pr_id || ':' || new.head_sha || ':' || new.path,
    '',
    new.status,
    new.path,
    '',
    CAST(strftime('%s','now') AS INTEGER)
  )
  ON CONFLICT(account_id, doc_type, doc_ref) DO UPDATE SET
    pr_id = excluded.pr_id,
    body = excluded.body,
    filename = excluded.filename,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS pr_files_au AFTER UPDATE ON pr_files BEGIN
  DELETE FROM search_documents
  WHERE account_id = old.account_id
    AND doc_type = 'file'
    AND doc_ref = old.pr_id || ':' || old.head_sha || ':' || old.path;

  INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
  VALUES (
    new.account_id,
    new.pr_id,
    'file',
    new.pr_id || ':' || new.head_sha || ':' || new.path,
    '',
    new.status,
    new.path,
    '',
    CAST(strftime('%s','now') AS INTEGER)
  );
END;

CREATE TRIGGER IF NOT EXISTS pr_files_ad AFTER DELETE ON pr_files BEGIN
  DELETE FROM search_documents
  WHERE account_id = old.account_id
    AND doc_type = 'file'
    AND doc_ref = old.pr_id || ':' || old.head_sha || ':' || old.path;
END;

CREATE INDEX IF NOT EXISTS idx_search_documents_scope ON search_documents(account_id, doc_type, pr_id);

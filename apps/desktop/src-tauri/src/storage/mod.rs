use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct RenderCacheStore {
    conn: Arc<Mutex<Connection>>,
    blob_root: Arc<PathBuf>,
}

impl RenderCacheStore {
    pub fn new(conn: Connection, blob_root: impl AsRef<Path>) -> Result<Self> {
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
            blob_root: Arc::new(blob_root.as_ref().to_path_buf()),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn memory(blob_root: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open_in_memory().context("open in-memory sqlite")?;
        Self::new(conn, blob_root)
    }

    fn init_schema(&self) -> Result<()> {
        fs::create_dir_all(self.blob_root.as_path()).context("create blob root")?;
        let conn = self.conn.lock().expect("render cache sqlite lock poisoned");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS blob_refs (
                sha256 TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                size INTEGER NOT NULL,
                ref_count INTEGER NOT NULL DEFAULT 1
            );
            "#,
        )
        .context("create blob_refs table")?;
        Ok(())
    }

    pub fn cache_key(&self, content_hash: &str, renderer_version: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content_hash.as_bytes());
        hasher.update(b":");
        hasher.update(renderer_version.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn blob_path(&self, sha256: &str) -> PathBuf {
        let prefix = &sha256[..2];
        self.blob_root.join(prefix).join(sha256)
    }

    pub fn load_rendered_html(&self, cache_key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("render cache sqlite lock poisoned");
        let found: Option<String> = conn
            .query_row(
                "SELECT sha256 FROM blob_refs WHERE sha256 = ?1 AND kind = 'rendered-html' LIMIT 1",
                params![cache_key],
                |row| row.get(0),
            )
            .optional()
            .context("lookup rendered-html ref")?;
        drop(conn);

        if found.is_none() {
            return Ok(None);
        }

        let path = self.blob_path(cache_key);
        if !path.exists() {
            return Ok(None);
        }

        let bytes = fs::read(path).context("read rendered html blob")?;
        let html = String::from_utf8(bytes).context("decode rendered html blob")?;
        Ok(Some(html))
    }

    pub fn store_rendered_html(&self, cache_key: &str, html: &str) -> Result<()> {
        let path = self.blob_path(cache_key);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).context("create blob path prefix")?;
            }
            fs::write(&path, html.as_bytes()).context("write rendered html blob")?;
        }

        let size = html.len() as i64;
        let conn = self.conn.lock().expect("render cache sqlite lock poisoned");
        conn.execute(
            r#"
            INSERT INTO blob_refs (sha256, kind, size, ref_count)
            VALUES (?1, 'rendered-html', ?2, 1)
            ON CONFLICT(sha256) DO UPDATE SET
                kind = excluded.kind,
                size = excluded.size,
                ref_count = blob_refs.ref_count + 1
            "#,
            params![cache_key, size],
        )
        .context("upsert rendered-html ref")?;
        Ok(())
    }

    pub fn rendered_ref_count(&self, cache_key: &str) -> Result<i64> {
        let conn = self.conn.lock().expect("render cache sqlite lock poisoned");
        let count = conn
            .query_row(
                "SELECT ref_count FROM blob_refs WHERE sha256 = ?1 AND kind = 'rendered-html'",
                params![cache_key],
                |row| row.get(0),
            )
            .optional()
            .context("query rendered-html ref_count")?
            .unwrap_or(0);
        Ok(count)
    }
}

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use thiserror::Error;
use tokio::fs;

use super::types::{BlobEvictionResult, BlobKind, BlobRefRow};

#[derive(Debug, Error)]
pub enum BlobStoreError {
    #[error(
        "blob kind mismatch for sha {sha256}: existing={existing_kind}, requested={requested_kind}"
    )]
    KindMismatch {
        sha256: String,
        existing_kind: String,
        requested_kind: String,
    },
    #[error("unknown blob kind in sqlite for sha {sha256}: {kind}")]
    UnknownKind { sha256: String, kind: String },
}

#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
    pool: SqlitePool,
    patch_compression_level: i32,
}

impl BlobStore {
    pub async fn new(
        root: impl AsRef<Path>,
        pool: SqlitePool,
        patch_compression_level: i32,
    ) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .await
            .with_context(|| format!("creating blob root at {}", root.display()))?;
        Ok(Self {
            root,
            pool,
            patch_compression_level,
        })
    }

    pub async fn put(&self, bytes: &[u8], kind: BlobKind) -> Result<String> {
        let sha256 = sha256_hex(bytes);
        let blob_path = self.blob_path(&sha256);
        let blob_dir = blob_path
            .parent()
            .context("blob path unexpectedly missing parent directory")?;

        fs::create_dir_all(blob_dir)
            .await
            .with_context(|| format!("creating blob prefix dir {}", blob_dir.display()))?;
        if fs::metadata(&blob_path).await.is_err() {
            fs::write(&blob_path, bytes)
                .await
                .with_context(|| format!("writing blob file {}", blob_path.display()))?;
        }

        let now = now_epoch_seconds()?;
        let size = i64::try_from(bytes.len()).context("blob larger than i64::MAX bytes")?;
        let mut tx = self.pool.begin().await?;

        if let Some(existing) = sqlx::query_as::<_, BlobRefRow>(
            "SELECT sha256, kind, size, ref_count, last_accessed_at, created_at, updated_at
             FROM blob_refs WHERE sha256 = ?1",
        )
        .bind(&sha256)
        .fetch_optional(tx.as_mut())
        .await?
        {
            if existing.kind != kind.as_str() {
                return Err(BlobStoreError::KindMismatch {
                    sha256,
                    existing_kind: existing.kind,
                    requested_kind: kind.as_str().to_string(),
                }
                .into());
            }
        }

        sqlx::query(
            "INSERT INTO blob_refs(sha256, kind, size, ref_count, last_accessed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4, ?4)
             ON CONFLICT(sha256) DO UPDATE SET
               ref_count = blob_refs.ref_count + 1,
               last_accessed_at = excluded.last_accessed_at,
               updated_at = excluded.updated_at",
        )
        .bind(&sha256)
        .bind(kind.as_str())
        .bind(size)
        .bind(now)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        Ok(sha256)
    }

    pub async fn put_patch(&self, bytes: &[u8]) -> Result<String> {
        let compressed = zstd::stream::encode_all(Cursor::new(bytes), self.patch_compression_level)
            .context("compressing patch with zstd")?;
        self.put(&compressed, BlobKind::Patch).await
    }

    pub async fn get(&self, sha256: &str) -> Result<Option<Vec<u8>>> {
        let maybe_row = sqlx::query_as::<_, BlobRefRow>(
            "SELECT sha256, kind, size, ref_count, last_accessed_at, created_at, updated_at
             FROM blob_refs WHERE sha256 = ?1",
        )
        .bind(sha256)
        .fetch_optional(&self.pool)
        .await?;

        let row = match maybe_row {
            Some(row) => row,
            None => return Ok(None),
        };

        let blob_path = self.blob_path(sha256);
        let stored_bytes = fs::read(&blob_path)
            .await
            .with_context(|| format!("reading blob file {}", blob_path.display()))?;
        let now = now_epoch_seconds()?;
        sqlx::query(
            "UPDATE blob_refs SET last_accessed_at = ?2, updated_at = ?2 WHERE sha256 = ?1",
        )
        .bind(sha256)
        .bind(now)
        .execute(&self.pool)
        .await?;

        if row.kind == BlobKind::Patch.as_str() {
            let decompressed = zstd::stream::decode_all(Cursor::new(stored_bytes))
                .context("decompressing zstd patch blob")?;
            return Ok(Some(decompressed));
        }

        if row.kind == BlobKind::MarkdownHtml.as_str()
            || row.kind == BlobKind::CheckLog.as_str()
            || row.kind == BlobKind::Asset.as_str()
        {
            return Ok(Some(stored_bytes));
        }

        Err(BlobStoreError::UnknownKind {
            sha256: row.sha256,
            kind: row.kind,
        }
        .into())
    }

    pub async fn evict_lru(&self, target_bytes: i64) -> Result<BlobEvictionResult> {
        let bytes_before: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM blob_refs")
            .fetch_one(&self.pool)
            .await?;
        if bytes_before <= target_bytes {
            return Ok(BlobEvictionResult {
                bytes_before,
                bytes_after: bytes_before,
                bytes_evicted: 0,
                blobs_evicted: 0,
            });
        }

        let mut bytes_after = bytes_before;
        let mut blobs_evicted = 0usize;
        let rows = sqlx::query_as::<_, BlobRefRow>(
            "SELECT sha256, kind, size, ref_count, last_accessed_at, created_at, updated_at
             FROM blob_refs
             ORDER BY last_accessed_at ASC, ref_count ASC, sha256 ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tx = self.pool.begin().await?;
        for row in rows {
            if bytes_after <= target_bytes {
                break;
            }
            if row.ref_count > 1 {
                continue;
            }

            let blob_path = self.blob_path(&row.sha256);
            if fs::metadata(&blob_path).await.is_ok() {
                fs::remove_file(&blob_path)
                    .await
                    .with_context(|| format!("removing blob file {}", blob_path.display()))?;

                if let Some(parent) = blob_path.parent() {
                    let _ = fs::remove_dir(parent).await;
                }
            }

            sqlx::query("DELETE FROM blob_refs WHERE sha256 = ?1")
                .bind(&row.sha256)
                .execute(tx.as_mut())
                .await?;
            bytes_after -= row.size;
            blobs_evicted += 1;
        }

        tx.commit().await?;
        Ok(BlobEvictionResult {
            bytes_before,
            bytes_after,
            bytes_evicted: bytes_before - bytes_after,
            blobs_evicted,
        })
    }

    fn blob_path(&self, sha256: &str) -> PathBuf {
        let prefix = sha256.get(0..2).unwrap_or("00");
        self.root.join(prefix).join(sha256)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn now_epoch_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(duration.as_secs()).context("unix timestamp exceeds i64")
}

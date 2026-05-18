pub mod blob_store;
pub mod types;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use tokio::fs;

use self::blob_store::BlobStore;
pub use self::types::*;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub struct Db {
    pool: SqlitePool,
    blob_store: BlobStore,
    _fixture_guard: Option<TempDir>,
}

impl Db {
    pub async fn open(app_data_dir: impl AsRef<Path>) -> Result<Self> {
        let app_data_dir = app_data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&app_data_dir)
            .await
            .with_context(|| format!("creating app data dir {}", app_data_dir.display()))?;
        let db_path = app_data_dir.join("cockpit.db");
        Self::open_with_paths(app_data_dir, db_path, None).await
    }

    pub async fn open_fixture() -> Result<Self> {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let fixture_db = fixture_root.join("cockpit_fixture.db");
        if !fixture_db.exists() {
            return Err(anyhow!(
                "fixture database missing at {}",
                fixture_db.display()
            ));
        }

        let tmp_dir = if Path::new("/dev/shm").exists() {
            tempfile::Builder::new()
                .prefix("pr-cockpit-fixture-")
                .tempdir_in("/dev/shm")
                .context("creating fixture tempdir in /dev/shm")?
        } else {
            tempfile::Builder::new()
                .prefix("pr-cockpit-fixture-")
                .tempdir()
                .context("creating fixture tempdir in system temp")?
        };

        let db_path = tmp_dir.path().join("cockpit_fixture.db");
        fs::copy(&fixture_db, &db_path).await.with_context(|| {
            format!(
                "copying fixture database {} -> {}",
                fixture_db.display(),
                db_path.display()
            )
        })?;

        let fixture_blobs = fixture_root.join("blobs");
        if fixture_blobs.exists() {
            copy_dir_recursive(&fixture_blobs, &tmp_dir.path().join("blobs"))?;
        }

        Self::open_with_paths(tmp_dir.path().to_path_buf(), db_path, Some(tmp_dir)).await
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn blob_store(&self) -> &BlobStore {
        &self.blob_store
    }

    pub async fn list_inbox(&self, account_id: &str) -> Result<Vec<InboxRow>> {
        let rows = sqlx::query(
            "SELECT account_id, account_login, account_host, pr_id, repo_id, repo_owner, repo_name, pr_number, title, state,
                    draft, head_sha, base_sha, mergeable_state, merge_state_status, updated_at,
                    author_login, unread_notification_count, latest_notification_at, pending_overlay
             FROM pr_inbox_rows
             WHERE account_id = ?1
             ORDER BY unread_notification_count DESC, updated_at DESC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(InboxRow {
                    account_id: row.try_get("account_id")?,
                    account_login: row.try_get("account_login")?,
                    account_host: row.try_get("account_host")?,
                    pr_id: row.try_get("pr_id")?,
                    repo_id: row.try_get("repo_id")?,
                    repo_owner: row.try_get("repo_owner")?,
                    repo_name: row.try_get("repo_name")?,
                    pr_number: row.try_get("pr_number")?,
                    title: row.try_get("title")?,
                    state: row.try_get("state")?,
                    draft: row.try_get("draft")?,
                    head_sha: row.try_get("head_sha")?,
                    base_sha: row.try_get("base_sha")?,
                    mergeable_state: row.try_get("mergeable_state")?,
                    merge_state_status: row.try_get("merge_state_status")?,
                    updated_at: row.try_get("updated_at")?,
                    author_login: row.try_get("author_login")?,
                    unread_notification_count: row.try_get("unread_notification_count")?,
                    latest_notification_at: row.try_get("latest_notification_at")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn list_inbox_all_accounts(
        &self,
        account_id_filter: Option<&str>,
    ) -> Result<Vec<InboxRow>> {
        let rows = sqlx::query(
            "SELECT account_id, account_login, account_host, pr_id, repo_id, repo_owner, repo_name, pr_number, title, state,
                    draft, head_sha, base_sha, mergeable_state, merge_state_status, updated_at,
                    author_login, unread_notification_count, latest_notification_at, pending_overlay
             FROM pr_inbox_rows
             WHERE (?1 IS NULL OR account_id = ?1)
             ORDER BY updated_at DESC, pr_id DESC",
        )
        .bind(account_id_filter)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(InboxRow {
                    account_id: row.try_get("account_id")?,
                    account_login: row.try_get("account_login")?,
                    account_host: row.try_get("account_host")?,
                    pr_id: row.try_get("pr_id")?,
                    repo_id: row.try_get("repo_id")?,
                    repo_owner: row.try_get("repo_owner")?,
                    repo_name: row.try_get("repo_name")?,
                    pr_number: row.try_get("pr_number")?,
                    title: row.try_get("title")?,
                    state: row.try_get("state")?,
                    draft: row.try_get("draft")?,
                    head_sha: row.try_get("head_sha")?,
                    base_sha: row.try_get("base_sha")?,
                    mergeable_state: row.try_get("mergeable_state")?,
                    merge_state_status: row.try_get("merge_state_status")?,
                    updated_at: row.try_get("updated_at")?,
                    author_login: row.try_get("author_login")?,
                    unread_notification_count: row.try_get("unread_notification_count")?,
                    latest_notification_at: row.try_get("latest_notification_at")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_detail_summary(
        &self,
        account_id: &str,
        pr_id: &str,
    ) -> Result<Option<PrDetailSummaryRow>> {
        let row = sqlx::query(
            "SELECT
                summary.account_id,
                summary.pr_id,
                summary.repo_id,
                summary.pr_number,
                summary.title,
                summary.body,
                summary.state,
                summary.draft,
                summary.base_ref,
                summary.base_sha,
                summary.head_ref,
                summary.head_sha,
                summary.mergeable_state,
                summary.merge_state_status,
                summary.merge_commit_allowed,
                summary.squash_merge_allowed,
                summary.rebase_merge_allowed,
                summary.delete_branch_on_merge_default,
                summary.viewer_can_merge,
                summary.viewer_can_enable_auto_merge,
                summary.viewer_can_disable_auto_merge,
                summary.viewer_can_update_branch,
                summary.viewer_can_delete_head_ref,
                summary.auto_merge_enabled,
                summary.auto_merge_method,
                summary.auto_merge_commit_headline,
                summary.auto_merge_commit_body,
                summary.auto_merge_enabled_by_login,
                summary.auto_merge_enabled_at,
                summary.merge_queue_entry_id,
                summary.merge_queue_entry_position,
                summary.merge_queue_entry_state,
                summary.merge_queue_entry_estimated_ms,
                summary.branch_protection_summary_json,
                summary.repo_has_merge_queue,
                summary.head_ref_state,
                summary.additions,
                summary.deletions,
                summary.changed_files,
                summary.comment_count,
                summary.review_count,
                summary.thread_count,
                summary.check_run_count,
                summary.file_count,
                summary.updated_at,
                pr.body_server_adjusted AS body_server_adjusted,
                summary.pending_overlay
             FROM pr_detail_summary summary
             JOIN pull_requests pr ON pr.account_id = summary.account_id AND pr.id = summary.pr_id
             WHERE summary.account_id = ?1 AND summary.pr_id = ?2",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(PrDetailSummaryRow {
                account_id: row.try_get("account_id")?,
                pr_id: row.try_get("pr_id")?,
                repo_id: row.try_get("repo_id")?,
                pr_number: row.try_get("pr_number")?,
                title: row.try_get("title")?,
                body: row.try_get("body")?,
                state: row.try_get("state")?,
                draft: row.try_get("draft")?,
                base_ref: row.try_get("base_ref")?,
                base_sha: row.try_get("base_sha")?,
                head_ref: row.try_get("head_ref")?,
                head_sha: row.try_get("head_sha")?,
                mergeable_state: row.try_get("mergeable_state")?,
                merge_state_status: row.try_get("merge_state_status")?,
                merge_commit_allowed: row.try_get("merge_commit_allowed")?,
                squash_merge_allowed: row.try_get("squash_merge_allowed")?,
                rebase_merge_allowed: row.try_get("rebase_merge_allowed")?,
                delete_branch_on_merge_default: row.try_get("delete_branch_on_merge_default")?,
                viewer_can_merge: row.try_get("viewer_can_merge")?,
                viewer_can_enable_auto_merge: row.try_get("viewer_can_enable_auto_merge")?,
                viewer_can_disable_auto_merge: row.try_get("viewer_can_disable_auto_merge")?,
                viewer_can_update_branch: row.try_get("viewer_can_update_branch")?,
                viewer_can_delete_head_ref: row.try_get("viewer_can_delete_head_ref")?,
                auto_merge_enabled: row.try_get("auto_merge_enabled")?,
                auto_merge_method: row.try_get("auto_merge_method")?,
                auto_merge_commit_headline: row.try_get("auto_merge_commit_headline")?,
                auto_merge_commit_body: row.try_get("auto_merge_commit_body")?,
                auto_merge_enabled_by_login: row.try_get("auto_merge_enabled_by_login")?,
                auto_merge_enabled_at: row.try_get("auto_merge_enabled_at")?,
                merge_queue_entry_id: row.try_get("merge_queue_entry_id")?,
                merge_queue_entry_position: row.try_get("merge_queue_entry_position")?,
                merge_queue_entry_state: row.try_get("merge_queue_entry_state")?,
                merge_queue_entry_estimated_ms: row.try_get("merge_queue_entry_estimated_ms")?,
                branch_protection_summary_json: row.try_get("branch_protection_summary_json")?,
                repo_has_merge_queue: row.try_get("repo_has_merge_queue")?,
                head_ref_state: row.try_get("head_ref_state")?,
                additions: row.try_get("additions")?,
                deletions: row.try_get("deletions")?,
                changed_files: row.try_get("changed_files")?,
                comment_count: row.try_get("comment_count")?,
                review_count: row.try_get("review_count")?,
                thread_count: row.try_get("thread_count")?,
                check_run_count: row.try_get("check_run_count")?,
                file_count: row.try_get("file_count")?,
                updated_at: row.try_get("updated_at")?,
                body_server_adjusted: row.try_get("body_server_adjusted")?,
                pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
            })
        })
        .transpose()
    }

    pub async fn pr_files(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Vec<PrFileRow>> {
        let rows = sqlx::query(
            "SELECT
                pf.account_id,
                pf.pr_id,
                pf.head_sha,
                pf.path,
                pf.old_path,
                COALESCE(pf.previous_path, pf.old_path) AS previous_path,
                pf.status,
                pf.additions,
                pf.deletions,
                pf.is_binary,
                COALESCE(
                  pf.kind,
                  CASE
                    WHEN pf.is_binary = 1 THEN 'binary'
                    ELSE 'text'
                  END
                ) AS kind,
                CASE
                  WHEN pf.rename_similarity IS NULL THEN NULL
                  ELSE CAST(pf.rename_similarity * 100.0 AS INTEGER)
                END AS rename_similarity,
                CASE
                  WHEN pf.viewed_by_account_id IS NOT NULL
                       AND pf.viewed_at_head_sha = pr.head_sha THEN 1
                  ELSE 0
                END AS is_viewed,
                pf.patch_blob_sha,
                pf.viewed_by_account_id,
                pf.viewed_at_head_sha,
                pf.pending_state AS pending_overlay
             FROM pr_files pf
             JOIN pull_requests pr
               ON pr.account_id = pf.account_id AND pr.id = pf.pr_id
             WHERE pf.account_id = ?1 AND pf.pr_id = ?2 AND pf.head_sha = ?3
             ORDER BY pf.path ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrFileRow {
                    account_id: row.try_get("account_id")?,
                    pr_id: row.try_get("pr_id")?,
                    head_sha: row.try_get("head_sha")?,
                    path: row.try_get("path")?,
                    old_path: row.try_get("old_path")?,
                    previous_path: row.try_get("previous_path")?,
                    status: row.try_get("status")?,
                    additions: row.try_get("additions")?,
                    deletions: row.try_get("deletions")?,
                    is_binary: row.try_get("is_binary")?,
                    kind: row.try_get("kind")?,
                    rename_similarity: row.try_get("rename_similarity")?,
                    is_viewed: row.try_get("is_viewed")?,
                    patch_blob_sha: row.try_get("patch_blob_sha")?,
                    viewed_by_account_id: row.try_get("viewed_by_account_id")?,
                    viewed_at_head_sha: row.try_get("viewed_at_head_sha")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_patch(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Option<Vec<u8>>> {
        let sha = sqlx::query_scalar::<_, String>(
            "SELECT patch_blob_sha
             FROM pr_patches
             WHERE account_id = ?1 AND pr_id = ?2 AND head_sha = ?3",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_optional(&self.pool)
        .await?;

        match sha {
            Some(sha) => self.blob_store.get(&sha).await,
            None => Ok(None),
        }
    }

    pub async fn pr_patch_blob_sha(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Option<String>> {
        let sha = sqlx::query_scalar::<_, String>(
            "SELECT patch_blob_sha
             FROM pr_patches
             WHERE account_id = ?1 AND pr_id = ?2 AND head_sha = ?3",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_optional(&self.pool)
        .await?;
        Ok(sha)
    }

    pub async fn unread_counts(&self, account_id: &str) -> Result<Option<UnreadCountsRow>> {
        let row = sqlx::query(
            "SELECT account_id, total_notifications, unread_notifications, prs_with_unread, pending_overlay
             FROM unread_counts
             WHERE account_id = ?1",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(UnreadCountsRow {
                account_id: row.try_get("account_id")?,
                total_notifications: row.try_get("total_notifications")?,
                unread_notifications: row.try_get("unread_notifications")?,
                prs_with_unread: row.try_get("prs_with_unread")?,
                pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
            })
        })
        .transpose()
    }

    pub async fn file_tree_summary(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Vec<FileTreeSummaryRow>> {
        let rows = sqlx::query(
            "SELECT account_id, pr_id, head_sha, directory, file_count, viewed_file_count, additions, deletions, pending_overlay
             FROM file_tree_summary
             WHERE account_id = ?1 AND pr_id = ?2 AND head_sha = ?3
             ORDER BY directory ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(FileTreeSummaryRow {
                    account_id: row.try_get("account_id")?,
                    pr_id: row.try_get("pr_id")?,
                    head_sha: row.try_get("head_sha")?,
                    directory: row.try_get("directory")?,
                    file_count: row.try_get("file_count")?,
                    viewed_file_count: row.try_get("viewed_file_count")?,
                    additions: row.try_get("additions")?,
                    deletions: row.try_get("deletions")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_timeline_page(
        &self,
        account_id: &str,
        pr_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TimelineRow>> {
        let rows = sqlx::query(
            "SELECT item_id, item_kind, body, author_login, created_at, updated_at, review_state, body_server_adjusted, pending_overlay
             FROM (
               SELECT
                 c.id AS item_id,
                 c.kind AS item_kind,
                 c.body AS body,
                 u.login AS author_login,
                 c.created_at AS created_at,
                 c.updated_at AS updated_at,
                 NULL AS review_state,
                 c.body_server_adjusted AS body_server_adjusted,
                 c.pending_state AS pending_overlay
               FROM comments c
               LEFT JOIN users u ON u.id = c.author_id
               WHERE c.account_id = ?1 AND c.pr_id = ?2

               UNION ALL

               SELECT
                 rv.id AS item_id,
                 'review' AS item_kind,
                 rv.body AS body,
                 u.login AS author_login,
                 rv.created_at AS created_at,
                 rv.updated_at AS updated_at,
                 rv.state AS review_state,
                 rv.body_server_adjusted AS body_server_adjusted,
                 rv.pending_state AS pending_overlay
               FROM reviews rv
               LEFT JOIN users u ON u.id = rv.author_id
               WHERE rv.account_id = ?1 AND rv.pr_id = ?2
             )
             ORDER BY created_at DESC, item_id DESC
             LIMIT ?3 OFFSET ?4",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(TimelineRow {
                    item_id: row.try_get("item_id")?,
                    item_kind: row.try_get("item_kind")?,
                    body: row.try_get("body")?,
                    author_login: row.try_get("author_login")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                    review_state: row.try_get("review_state")?,
                    body_server_adjusted: row.try_get("body_server_adjusted")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_review_threads(
        &self,
        account_id: &str,
        pr_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReviewThreadRow>> {
        let rows = sqlx::query(
            "SELECT
               rt.id,
               rt.path,
               rt.line,
               rt.side,
               rt.start_line,
               rt.start_side,
               rt.is_outdated,
               rt.is_resolved,
               u.login AS resolved_by_login,
               rt.updated_at,
               (
                 SELECT COUNT(*)
                 FROM comments c
                 WHERE c.account_id = rt.account_id AND c.thread_id = rt.id
               ) AS comment_count,
               rt.pending_state AS pending_overlay
             FROM review_threads rt
             LEFT JOIN users u ON u.id = rt.resolved_by_id
             WHERE rt.account_id = ?1 AND rt.pr_id = ?2
             ORDER BY rt.updated_at DESC, rt.id DESC
             LIMIT ?3 OFFSET ?4",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ReviewThreadRow {
                    id: row.try_get("id")?,
                    path: row.try_get("path")?,
                    line: row.try_get("line")?,
                    side: row.try_get("side")?,
                    start_line: row.try_get("start_line")?,
                    start_side: row.try_get("start_side")?,
                    is_outdated: row.try_get("is_outdated")?,
                    is_resolved: row.try_get("is_resolved")?,
                    resolved_by_login: row.try_get("resolved_by_login")?,
                    updated_at: row.try_get("updated_at")?,
                    comment_count: row.try_get("comment_count")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_check_runs(
        &self,
        account_id: &str,
        pr_id: &str,
    ) -> Result<Vec<CheckRunSummaryRow>> {
        let rows = sqlx::query_as::<_, CheckRunSummaryRow>(
            "SELECT
               cr.id,
               cr.name,
               cr.status,
               cr.conclusion,
               cr.details_url,
               cr.started_at,
               cr.completed_at,
               cs.app_name
             FROM check_runs cr
             LEFT JOIN check_suites cs ON cs.id = cr.check_suite_id
             WHERE cr.account_id = ?1 AND cr.pr_id = ?2
             ORDER BY cr.completed_at DESC, cr.updated_at DESC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_notifications(
        &self,
        account_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NotificationListRow>> {
        let rows = sqlx::query_as::<_, NotificationListRow>(
            "SELECT
               n.id,
               n.reason,
               n.title,
               n.unread,
               n.updated_at,
               n.pr_id,
               r.owner AS repo_owner,
               r.name AS repo_name
             FROM notifications n
             JOIN repos r ON r.id = n.repo_id
             WHERE n.account_id = ?1
             ORDER BY n.updated_at DESC, n.id DESC
             LIMIT ?2 OFFSET ?3",
        )
        .bind(account_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_repo_subscriptions(
        &self,
        account_id: &str,
    ) -> Result<Vec<RepoSubscriptionRow>> {
        let rows = sqlx::query_as::<_, RepoSubscriptionRow>(
            "SELECT
               rs.account_id,
               rs.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               rs.watch_tier,
               rs.last_full_sync_at,
               rs.updated_at
             FROM repo_subscriptions rs
             JOIN repos r ON r.id = rs.repo_id
             WHERE rs.account_id = ?1
             ORDER BY r.owner ASC, r.name ASC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn pr_labels(&self, account_id: &str, pr_id: &str) -> Result<Vec<PrLabelRow>> {
        let rows = sqlx::query(
            "SELECT label_name, label_color, description, pending_state AS pending_overlay
             FROM pr_labels
             WHERE account_id = ?1 AND pr_id = ?2
             ORDER BY label_name ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrLabelRow {
                    label_name: row.try_get("label_name")?,
                    label_color: row.try_get("label_color")?,
                    description: row.try_get("description")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_assignees(&self, account_id: &str, pr_id: &str) -> Result<Vec<PrAssigneeRow>> {
        let rows = sqlx::query(
            "SELECT
               pa.user_id,
               u.login,
               pa.assigned_at,
               pa.pending_state AS pending_overlay
             FROM pr_assignees pa
             LEFT JOIN users u ON u.id = pa.user_id
             WHERE pa.account_id = ?1 AND pa.pr_id = ?2
             ORDER BY pa.assigned_at ASC, pa.user_id ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrAssigneeRow {
                    user_id: row.try_get("user_id")?,
                    login: row.try_get("login")?,
                    assigned_at: row.try_get("assigned_at")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_reviewers(&self, account_id: &str, pr_id: &str) -> Result<Vec<PrReviewerRow>> {
        let rows = sqlx::query(
            "SELECT
               pr.user_id,
               u.login,
               pr.reviewer_type,
               pr.reviewer_state,
               pr.requested_at,
               pr.pending_state AS pending_overlay
             FROM pr_reviewers pr
             LEFT JOIN users u ON u.id = pr.user_id
             WHERE pr.account_id = ?1 AND pr.pr_id = ?2
             ORDER BY pr.requested_at ASC, pr.user_id ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrReviewerRow {
                    user_id: row.try_get("user_id")?,
                    login: row.try_get("login")?,
                    reviewer_type: row.try_get("reviewer_type")?,
                    reviewer_state: row.try_get("reviewer_state")?,
                    requested_at: row.try_get("requested_at")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_projects(&self, account_id: &str, pr_id: &str) -> Result<Vec<PrProjectRow>> {
        let rows = sqlx::query(
            "SELECT project_id, project_title, item_id, status, updated_at, pending_state AS pending_overlay
             FROM pr_projects
             WHERE account_id = ?1 AND pr_id = ?2
             ORDER BY project_title ASC, project_id ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrProjectRow {
                    project_id: row.try_get("project_id")?,
                    project_title: row.try_get("project_title")?,
                    item_id: row.try_get("item_id")?,
                    status: row.try_get("status")?,
                    updated_at: row.try_get("updated_at")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn pr_milestones(
        &self,
        account_id: &str,
        pr_id: &str,
    ) -> Result<Vec<PrMilestoneRow>> {
        let rows = sqlx::query(
            "SELECT milestone_id, title, state, due_on, description, pending_state AS pending_overlay
             FROM pr_milestones
             WHERE account_id = ?1 AND pr_id = ?2
             ORDER BY title ASC, milestone_id ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PrMilestoneRow {
                    milestone_id: row.try_get("milestone_id")?,
                    title: row.try_get("title")?,
                    state: row.try_get("state")?,
                    due_on: row.try_get("due_on")?,
                    description: row.try_get("description")?,
                    pending_overlay: parse_pending_overlay(row.try_get("pending_overlay")?)?,
                })
            })
            .collect()
    }

    pub async fn rate_limit_buckets_for_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<RateLimitBucketRow>> {
        let rows = sqlx::query_as::<_, RateLimitBucketRow>(
            "SELECT account_id, resource, remaining, used, limit_total, reset_at, updated_at
             FROM account_rate_limits
             WHERE account_id = ?1
             ORDER BY resource ASC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn rate_limit_buckets(
        &self,
        account_id_filter: Option<&str>,
    ) -> Result<Vec<RateLimitBucketRow>> {
        let rows = sqlx::query_as::<_, RateLimitBucketRow>(
            "SELECT account_id, resource, remaining, used, limit_total, reset_at, updated_at
             FROM account_rate_limits
             WHERE (?1 IS NULL OR account_id = ?1)
             ORDER BY account_id ASC, resource ASC",
        )
        .bind(account_id_filter)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn resolve_pr_id(
        &self,
        account_id: &str,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<Option<String>> {
        let pr_id = sqlx::query_scalar::<_, String>(
            "SELECT pr.id
             FROM pull_requests pr
             JOIN repos r ON r.id = pr.repo_id
             WHERE pr.account_id = ?1
               AND r.owner = ?2
               AND r.name = ?3
               AND pr.number = ?4
             LIMIT 1",
        )
        .bind(account_id)
        .bind(owner)
        .bind(repo)
        .bind(number)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pr_id)
    }

    pub async fn pr_range_diff_context(
        &self,
        pr_id: &str,
    ) -> Result<Option<PrRangeDiffContextRow>> {
        let row = sqlx::query_as::<_, PrRangeDiffContextRow>(
            "SELECT
               pr.id AS pr_id,
               pr.account_id,
               repo.owner AS repo_owner,
               repo.name AS repo_name
             FROM pull_requests pr
             JOIN repos repo ON repo.id = pr.repo_id
             WHERE pr.id = ?1
             LIMIT 1",
        )
        .bind(pr_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn latest_pr_push(&self, pr_id: &str) -> Result<Option<PrPushRow>> {
        let row = sqlx::query_as::<_, PrPushRow>(
            "SELECT
               id,
               pr_id,
               account_id,
               head_sha,
               base_sha,
               observed_at,
               push_kind,
               supersedes_head_sha
             FROM pr_pushes
             WHERE pr_id = ?1
             ORDER BY observed_at DESC, id DESC
             LIMIT 1",
        )
        .bind(pr_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn upsert_pr_push(&self, push: &PrPushRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO pr_pushes(
               pr_id,
               account_id,
               head_sha,
               base_sha,
               observed_at,
               push_kind,
               supersedes_head_sha
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(pr_id, head_sha) DO NOTHING",
        )
        .bind(&push.pr_id)
        .bind(&push.account_id)
        .bind(&push.head_sha)
        .bind(&push.base_sha)
        .bind(push.observed_at)
        .bind(&push.push_kind)
        .bind(&push.supersedes_head_sha)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_pr_pushes(&self, pr_id: &str) -> Result<Vec<PrPushRow>> {
        let rows = sqlx::query_as::<_, PrPushRow>(
            "SELECT
               id,
               pr_id,
               account_id,
               head_sha,
               base_sha,
               observed_at,
               push_kind,
               supersedes_head_sha
             FROM pr_pushes
             WHERE pr_id = ?1
             ORDER BY observed_at ASC, id ASC",
        )
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_pr_force_push_pairs(&self, pr_id: &str) -> Result<Vec<PrForcePushPairRow>> {
        let rows = sqlx::query_as::<_, PrForcePushPairRow>(
            "SELECT pr_id, base_sha, old_head_sha, new_head_sha, occurred_at
             FROM pr_force_push_pairs
             WHERE pr_id = ?1
             ORDER BY occurred_at ASC",
        )
        .bind(pr_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn best_worktree_for_pr(&self, pr_id: &str) -> Result<Option<WorktreeViewRow>> {
        let row = sqlx::query_as::<_, WorktreeViewRow>(
            "SELECT
               w.id,
               w.account_id,
               w.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               w.path,
               w.head_sha,
               w.branch,
               w.dirty,
               w.ahead,
               w.behind,
               w.untracked_count,
               w.staged_count,
               w.modified_count,
               w.mapped_pr_id,
               pr.number AS mapped_pr_number,
               w.mapping_confidence,
               w.mapping_source,
               w.is_app_managed,
               w.manual_override_pr_id,
               w.manual_override_at,
               w.last_cleanup_snapshot_id,
               w.created_at,
               w.updated_at
             FROM worktrees w
             JOIN repos r ON r.id = w.repo_id
             LEFT JOIN pull_requests pr ON pr.id = w.mapped_pr_id
             WHERE w.mapped_pr_id = ?1
             ORDER BY
               CASE
                 WHEN w.manual_override_pr_id = ?1 THEN 1
                 ELSE 0
               END DESC,
               COALESCE(w.mapping_confidence, 0.0) DESC,
               w.updated_at DESC
             LIMIT 1",
        )
        .bind(pr_id)
        .bind(pr_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn search(
        &self,
        account_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SearchHitRow>> {
        let rows = sqlx::query_as::<_, SearchHitRow>(
            "SELECT sd.doc_type, sd.doc_ref, sd.pr_id, sd.title, sd.body, sd.filename, sd.author
             FROM search_fts
             JOIN search_documents sd ON sd.id = search_fts.rowid
             WHERE sd.account_id = ?1
               AND search_fts MATCH ?2
             ORDER BY bm25(search_fts)
             LIMIT ?3",
        )
        .bind(account_id)
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn rebuild_search_index(&self) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM search_documents")
            .execute(tx.as_mut())
            .await?;

        sqlx::query(
            "INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
             SELECT
               pr.account_id,
               pr.id,
               'pr',
               pr.id,
               pr.title,
               pr.body,
               '',
               COALESCE(u.login, ''),
               pr.updated_at
             FROM pull_requests pr
             LEFT JOIN users u ON u.id = pr.author_id AND u.account_id = pr.account_id",
        )
        .execute(tx.as_mut())
        .await?;

        sqlx::query(
            "INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
             SELECT
               c.account_id,
               c.pr_id,
               'comment',
               c.id,
               '',
               c.body,
               COALESCE(c.path, ''),
               COALESCE(u.login, ''),
               c.updated_at
             FROM comments c
             LEFT JOIN users u ON u.id = c.author_id AND u.account_id = c.account_id",
        )
        .execute(tx.as_mut())
        .await?;

        sqlx::query(
            "INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
             SELECT
               rv.account_id,
               rv.pr_id,
               'review',
               rv.id,
               '',
               rv.body,
               '',
               COALESCE(u.login, ''),
               rv.updated_at
             FROM reviews rv
             LEFT JOIN users u ON u.id = rv.author_id AND u.account_id = rv.account_id",
        )
        .execute(tx.as_mut())
        .await?;

        sqlx::query(
            "INSERT INTO search_documents(account_id, pr_id, doc_type, doc_ref, title, body, filename, author, updated_at)
             SELECT
               pf.account_id,
               pf.pr_id,
               'file',
               pf.pr_id || ':' || pf.head_sha || ':' || pf.path,
               '',
               pf.status,
               pf.path,
               '',
               CAST(strftime('%s','now') AS INTEGER)
             FROM pr_files pf",
        )
        .execute(tx.as_mut())
        .await?;

        sqlx::query("INSERT INTO search_fts(search_fts) VALUES ('rebuild')")
            .execute(tx.as_mut())
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_account(&self, account: &AccountRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO accounts(id, host, login, token_kind, scopes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               host = excluded.host,
               login = excluded.login,
               token_kind = excluded.token_kind,
               scopes = excluded.scopes,
               updated_at = excluded.updated_at",
        )
        .bind(&account.id)
        .bind(&account.host)
        .bind(&account.login)
        .bind(&account.token_kind)
        .bind(&account.scopes)
        .bind(account.created_at)
        .bind(account.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_auth_account(
        &self,
        host: &str,
        login: &str,
        token_kind: &str,
        scopes: &str,
        now_epoch: i64,
    ) -> Result<AuthAccountRow> {
        let account_id = account_id_from_host_login(host, login);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO accounts(id, host, login, token_kind, scopes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(host, login) DO UPDATE SET
               token_kind = excluded.token_kind,
               scopes = excluded.scopes,
               updated_at = excluded.updated_at",
        )
        .bind(&account_id)
        .bind(host)
        .bind(login)
        .bind(token_kind)
        .bind(scopes)
        .bind(now_epoch)
        .execute(tx.as_mut())
        .await?;

        let account = sqlx::query_as::<_, AuthAccountRow>(
            "SELECT id, host, login, token_kind, scopes, created_at, updated_at
             FROM accounts
             WHERE host = ?1 AND login = ?2",
        )
        .bind(host)
        .bind(login)
        .fetch_one(tx.as_mut())
        .await?;

        let active_exists: Option<String> = sqlx::query_scalar(
            "SELECT value FROM app_settings WHERE key = 'active_account_id' LIMIT 1",
        )
        .fetch_optional(tx.as_mut())
        .await?;
        if active_exists.is_none() {
            sqlx::query(
                "INSERT INTO app_settings(key, value, updated_at)
                 VALUES ('active_account_id', ?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET
                   value = excluded.value,
                   updated_at = excluded.updated_at",
            )
            .bind(&account.id)
            .bind(now_epoch)
            .execute(tx.as_mut())
            .await?;
        }

        tx.commit().await?;
        Ok(account)
    }

    pub async fn list_auth_accounts(&self) -> Result<Vec<AuthAccountRow>> {
        let rows = sqlx::query_as::<_, AuthAccountRow>(
            "SELECT id, host, login, token_kind, scopes, created_at, updated_at
             FROM accounts
             ORDER BY host ASC, login ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn auth_account_by_id(&self, account_id: &str) -> Result<Option<AuthAccountRow>> {
        let row = sqlx::query_as::<_, AuthAccountRow>(
            "SELECT id, host, login, token_kind, scopes, created_at, updated_at
             FROM accounts
             WHERE id = ?1
             LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn active_account_id(&self) -> Result<Option<String>> {
        let active_id: Option<String> = sqlx::query_scalar(
            "SELECT value
             FROM app_settings
             WHERE key = 'active_account_id'
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(active_id)
    }

    pub async fn set_active_account_id(&self, account_id: &str, now_epoch: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_settings(key, value, updated_at)
             VALUES ('active_account_id', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
        )
        .bind(account_id)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_auth_account(
        &self,
        host: &str,
        login: &str,
    ) -> Result<Option<AuthAccountRow>> {
        let row = sqlx::query_as::<_, AuthAccountRow>(
            "SELECT id, host, login, token_kind, scopes, created_at, updated_at
             FROM accounts
             WHERE host = ?1 AND login = ?2
             LIMIT 1",
        )
        .bind(host)
        .bind(login)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn remove_auth_account(
        &self,
        host: &str,
        login: &str,
        now_epoch: i64,
    ) -> Result<Option<AuthAccountRow>> {
        let mut tx = self.pool.begin().await?;
        let account = sqlx::query_as::<_, AuthAccountRow>(
            "SELECT id, host, login, token_kind, scopes, created_at, updated_at
             FROM accounts
             WHERE host = ?1 AND login = ?2
             LIMIT 1",
        )
        .bind(host)
        .bind(login)
        .fetch_optional(tx.as_mut())
        .await?;
        let Some(account) = account else {
            tx.rollback().await?;
            return Ok(None);
        };

        sqlx::query("DELETE FROM accounts WHERE id = ?1")
            .bind(&account.id)
            .execute(tx.as_mut())
            .await?;

        let current_active: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'active_account_id'")
                .fetch_optional(tx.as_mut())
                .await?;
        if current_active.as_deref() == Some(account.id.as_str()) {
            let next_active: Option<String> =
                sqlx::query_scalar("SELECT id FROM accounts ORDER BY host ASC, login ASC LIMIT 1")
                    .fetch_optional(tx.as_mut())
                    .await?;
            match next_active {
                Some(next_id) => {
                    sqlx::query(
                        "INSERT INTO app_settings(key, value, updated_at)
                         VALUES ('active_account_id', ?1, ?2)
                         ON CONFLICT(key) DO UPDATE SET
                           value = excluded.value,
                           updated_at = excluded.updated_at",
                    )
                    .bind(next_id)
                    .bind(now_epoch)
                    .execute(tx.as_mut())
                    .await?;
                }
                None => {
                    sqlx::query("DELETE FROM app_settings WHERE key = 'active_account_id'")
                        .execute(tx.as_mut())
                        .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(Some(account))
    }

    pub async fn upsert_repo(&self, repo: &RepoRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO repos(id, account_id, owner, name, default_branch, description, html_url, is_private, is_archived, pushed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               owner = excluded.owner,
               name = excluded.name,
               default_branch = excluded.default_branch,
               description = excluded.description,
               html_url = excluded.html_url,
               is_private = excluded.is_private,
               is_archived = excluded.is_archived,
               pushed_at = excluded.pushed_at,
               updated_at = excluded.updated_at",
        )
        .bind(&repo.id)
        .bind(&repo.account_id)
        .bind(&repo.owner)
        .bind(&repo.name)
        .bind(&repo.default_branch)
        .bind(&repo.description)
        .bind(&repo.html_url)
        .bind(bool_to_i64(repo.is_private))
        .bind(bool_to_i64(repo.is_archived))
        .bind(repo.pushed_at)
        .bind(repo.created_at)
        .bind(repo.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_repo_subscription(
        &self,
        subscription: &RepoSubscriptionRecord,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO repo_subscriptions(repo_id, account_id, watch_tier, last_full_sync_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(repo_id, account_id) DO UPDATE SET
               watch_tier = excluded.watch_tier,
               last_full_sync_at = excluded.last_full_sync_at,
               updated_at = excluded.updated_at",
        )
        .bind(&subscription.repo_id)
        .bind(&subscription.account_id)
        .bind(&subscription.watch_tier)
        .bind(subscription.last_full_sync_at)
        .bind(subscription.created_at)
        .bind(subscription.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_user(&self, user: &UserRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO users(id, account_id, login, display_name, avatar_url, html_url, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               login = excluded.login,
               display_name = excluded.display_name,
               avatar_url = excluded.avatar_url,
               html_url = excluded.html_url,
               updated_at = excluded.updated_at",
        )
        .bind(&user.id)
        .bind(&user.account_id)
        .bind(&user.login)
        .bind(&user.display_name)
        .bind(&user.avatar_url)
        .bind(&user.html_url)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_org(&self, org: &OrgRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO orgs(id, account_id, login, display_name, avatar_url, html_url, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               login = excluded.login,
               display_name = excluded.display_name,
               avatar_url = excluded.avatar_url,
               html_url = excluded.html_url,
               updated_at = excluded.updated_at",
        )
        .bind(&org.id)
        .bind(&org.account_id)
        .bind(&org.login)
        .bind(&org.display_name)
        .bind(&org.avatar_url)
        .bind(&org.html_url)
        .bind(org.created_at)
        .bind(org.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_pull_request(&self, pr: &PullRequestRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pull_requests(
               id, account_id, repo_id, number, state, draft, title, body, author_id, base_ref, base_sha, head_ref, head_sha,
               head_repo_id, mergeable_state, merge_state_status, merge_commit_allowed, squash_merge_allowed,
               rebase_merge_allowed, delete_branch_on_merge_default, viewer_can_merge, viewer_can_enable_auto_merge,
               viewer_can_disable_auto_merge, viewer_can_update_branch, viewer_can_delete_head_ref, auto_merge_enabled,
               auto_merge_method, auto_merge_commit_headline, auto_merge_commit_body, auto_merge_enabled_by_login,
               auto_merge_enabled_at, merge_queue_entry_id, merge_queue_entry_position, merge_queue_entry_state,
               merge_queue_entry_estimated_ms, branch_protection_summary_json, repo_has_merge_queue, head_ref_state,
               additions, deletions, changed_files, comments_count, reviews_count, commits_count, is_read, html_url,
               created_at, updated_at, closed_at, merged_at
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
               ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
               ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50
             )
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               repo_id = excluded.repo_id,
               number = excluded.number,
               state = excluded.state,
               draft = excluded.draft,
               title = excluded.title,
               body = excluded.body,
               author_id = excluded.author_id,
               base_ref = excluded.base_ref,
               base_sha = excluded.base_sha,
               head_ref = excluded.head_ref,
               head_sha = excluded.head_sha,
               head_repo_id = excluded.head_repo_id,
               mergeable_state = excluded.mergeable_state,
               merge_state_status = excluded.merge_state_status,
               merge_commit_allowed = excluded.merge_commit_allowed,
               squash_merge_allowed = excluded.squash_merge_allowed,
               rebase_merge_allowed = excluded.rebase_merge_allowed,
               delete_branch_on_merge_default = excluded.delete_branch_on_merge_default,
               viewer_can_merge = excluded.viewer_can_merge,
               viewer_can_enable_auto_merge = excluded.viewer_can_enable_auto_merge,
               viewer_can_disable_auto_merge = excluded.viewer_can_disable_auto_merge,
               viewer_can_update_branch = excluded.viewer_can_update_branch,
               viewer_can_delete_head_ref = excluded.viewer_can_delete_head_ref,
               auto_merge_enabled = excluded.auto_merge_enabled,
               auto_merge_method = excluded.auto_merge_method,
               auto_merge_commit_headline = excluded.auto_merge_commit_headline,
               auto_merge_commit_body = excluded.auto_merge_commit_body,
               auto_merge_enabled_by_login = excluded.auto_merge_enabled_by_login,
               auto_merge_enabled_at = excluded.auto_merge_enabled_at,
               merge_queue_entry_id = excluded.merge_queue_entry_id,
               merge_queue_entry_position = excluded.merge_queue_entry_position,
               merge_queue_entry_state = excluded.merge_queue_entry_state,
               merge_queue_entry_estimated_ms = excluded.merge_queue_entry_estimated_ms,
               branch_protection_summary_json = excluded.branch_protection_summary_json,
               repo_has_merge_queue = excluded.repo_has_merge_queue,
               head_ref_state = excluded.head_ref_state,
               additions = excluded.additions,
               deletions = excluded.deletions,
               changed_files = excluded.changed_files,
               comments_count = excluded.comments_count,
               reviews_count = excluded.reviews_count,
               commits_count = excluded.commits_count,
               is_read = excluded.is_read,
               html_url = excluded.html_url,
               updated_at = excluded.updated_at,
               closed_at = excluded.closed_at,
               merged_at = excluded.merged_at",
        )
        .bind(&pr.id)
        .bind(&pr.account_id)
        .bind(&pr.repo_id)
        .bind(pr.number)
        .bind(&pr.state)
        .bind(bool_to_i64(pr.draft))
        .bind(&pr.title)
        .bind(&pr.body)
        .bind(&pr.author_id)
        .bind(&pr.base_ref)
        .bind(&pr.base_sha)
        .bind(&pr.head_ref)
        .bind(&pr.head_sha)
        .bind(&pr.head_repo_id)
        .bind(&pr.mergeable_state)
        .bind(&pr.merge_state_status)
        .bind(pr.merge_commit_allowed.map(bool_to_i64))
        .bind(pr.squash_merge_allowed.map(bool_to_i64))
        .bind(pr.rebase_merge_allowed.map(bool_to_i64))
        .bind(pr.delete_branch_on_merge_default.map(bool_to_i64))
        .bind(pr.viewer_can_merge.map(bool_to_i64))
        .bind(pr.viewer_can_enable_auto_merge.map(bool_to_i64))
        .bind(pr.viewer_can_disable_auto_merge.map(bool_to_i64))
        .bind(pr.viewer_can_update_branch.map(bool_to_i64))
        .bind(pr.viewer_can_delete_head_ref.map(bool_to_i64))
        .bind(pr.auto_merge_enabled.map(bool_to_i64))
        .bind(&pr.auto_merge_method)
        .bind(&pr.auto_merge_commit_headline)
        .bind(&pr.auto_merge_commit_body)
        .bind(&pr.auto_merge_enabled_by_login)
        .bind(pr.auto_merge_enabled_at)
        .bind(&pr.merge_queue_entry_id)
        .bind(pr.merge_queue_entry_position)
        .bind(&pr.merge_queue_entry_state)
        .bind(pr.merge_queue_entry_estimated_ms)
        .bind(&pr.branch_protection_summary_json)
        .bind(pr.repo_has_merge_queue.map(bool_to_i64))
        .bind(&pr.head_ref_state)
        .bind(pr.additions)
        .bind(pr.deletions)
        .bind(pr.changed_files)
        .bind(pr.comments_count)
        .bind(pr.reviews_count)
        .bind(pr.commits_count)
        .bind(bool_to_i64(pr.is_read))
        .bind(&pr.html_url)
        .bind(pr.created_at)
        .bind(pr.updated_at)
        .bind(pr.closed_at)
        .bind(pr.merged_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_pr_labels(
        &self,
        account_id: &str,
        pr_id: &str,
        labels: &[PrLabelRecord],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM pr_labels WHERE account_id = ?1 AND pr_id = ?2")
            .bind(account_id)
            .bind(pr_id)
            .execute(tx.as_mut())
            .await?;

        for label in labels {
            sqlx::query(
                "INSERT INTO pr_labels(account_id, pr_id, label_name, label_color, description)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(&label.account_id)
            .bind(&label.pr_id)
            .bind(&label.label_name)
            .bind(&label.label_color)
            .bind(&label.description)
            .execute(tx.as_mut())
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_pr_assignees(
        &self,
        account_id: &str,
        pr_id: &str,
        assignees: &[PrAssigneeRecord],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM pr_assignees WHERE account_id = ?1 AND pr_id = ?2")
            .bind(account_id)
            .bind(pr_id)
            .execute(tx.as_mut())
            .await?;

        for assignee in assignees {
            sqlx::query(
                "INSERT INTO pr_assignees(account_id, pr_id, user_id, assigned_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(&assignee.account_id)
            .bind(&assignee.pr_id)
            .bind(&assignee.user_id)
            .bind(assignee.assigned_at)
            .execute(tx.as_mut())
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_pr_reviewers(
        &self,
        account_id: &str,
        pr_id: &str,
        reviewers: &[PrReviewerRecord],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM pr_reviewers WHERE account_id = ?1 AND pr_id = ?2")
            .bind(account_id)
            .bind(pr_id)
            .execute(tx.as_mut())
            .await?;

        for reviewer in reviewers {
            sqlx::query(
                "INSERT INTO pr_reviewers(
                   account_id, pr_id, user_id, reviewer_type, reviewer_state, requested_at
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&reviewer.account_id)
            .bind(&reviewer.pr_id)
            .bind(&reviewer.user_id)
            .bind(&reviewer.reviewer_type)
            .bind(&reviewer.reviewer_state)
            .bind(reviewer.requested_at)
            .execute(tx.as_mut())
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_commit(&self, commit: &CommitRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO commits(id, account_id, repo_id, author_id, message_headline, message_body, committed_at, parents_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               repo_id = excluded.repo_id,
               author_id = excluded.author_id,
               message_headline = excluded.message_headline,
               message_body = excluded.message_body,
               committed_at = excluded.committed_at,
               parents_json = excluded.parents_json",
        )
        .bind(&commit.id)
        .bind(&commit.account_id)
        .bind(&commit.repo_id)
        .bind(&commit.author_id)
        .bind(&commit.message_headline)
        .bind(&commit.message_body)
        .bind(commit.committed_at)
        .bind(&commit.parents_json)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_pr_commit(&self, pr_commit: &PrCommitRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pr_commits(account_id, pr_id, commit_id, commit_order)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_id, pr_id, commit_id) DO UPDATE SET
               commit_order = excluded.commit_order",
        )
        .bind(&pr_commit.account_id)
        .bind(&pr_commit.pr_id)
        .bind(&pr_commit.commit_id)
        .bind(pr_commit.commit_order)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_comment(&self, comment: &CommentRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO comments(
               id, account_id, pr_id, kind, author_id, body, created_at, updated_at, deleted_at,
               in_reply_to_id, review_id, thread_id, path, line, side, start_line, start_side, original_commit_sha
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
             )
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               pr_id = excluded.pr_id,
               kind = excluded.kind,
               author_id = excluded.author_id,
               body = excluded.body,
               updated_at = excluded.updated_at,
               deleted_at = excluded.deleted_at,
               in_reply_to_id = excluded.in_reply_to_id,
               review_id = excluded.review_id,
               thread_id = excluded.thread_id,
               path = excluded.path,
               line = excluded.line,
               side = excluded.side,
               start_line = excluded.start_line,
               start_side = excluded.start_side,
               original_commit_sha = excluded.original_commit_sha",
        )
        .bind(&comment.id)
        .bind(&comment.account_id)
        .bind(&comment.pr_id)
        .bind(&comment.kind)
        .bind(&comment.author_id)
        .bind(&comment.body)
        .bind(comment.created_at)
        .bind(comment.updated_at)
        .bind(comment.deleted_at)
        .bind(&comment.in_reply_to_id)
        .bind(&comment.review_id)
        .bind(&comment.thread_id)
        .bind(&comment.path)
        .bind(comment.line)
        .bind(&comment.side)
        .bind(comment.start_line)
        .bind(&comment.start_side)
        .bind(&comment.original_commit_sha)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_review_thread(&self, thread: &ReviewThreadRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO review_threads(
               id, account_id, pr_id, path, line, side, start_line, start_side, original_commit_sha,
               original_path, original_position, original_line, is_outdated, is_resolved, resolved_by_id, created_at, updated_at
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
             )
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               pr_id = excluded.pr_id,
               path = excluded.path,
               line = excluded.line,
               side = excluded.side,
               start_line = excluded.start_line,
               start_side = excluded.start_side,
               original_commit_sha = excluded.original_commit_sha,
               original_path = excluded.original_path,
               original_position = excluded.original_position,
               original_line = excluded.original_line,
               is_outdated = excluded.is_outdated,
               is_resolved = excluded.is_resolved,
               resolved_by_id = excluded.resolved_by_id,
               updated_at = excluded.updated_at",
        )
        .bind(&thread.id)
        .bind(&thread.account_id)
        .bind(&thread.pr_id)
        .bind(&thread.path)
        .bind(thread.line)
        .bind(&thread.side)
        .bind(thread.start_line)
        .bind(&thread.start_side)
        .bind(&thread.original_commit_sha)
        .bind(&thread.original_path)
        .bind(thread.original_position)
        .bind(thread.original_line)
        .bind(bool_to_i64(thread.is_outdated))
        .bind(bool_to_i64(thread.is_resolved))
        .bind(&thread.resolved_by_id)
        .bind(thread.created_at)
        .bind(thread.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_review(&self, review: &ReviewRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO reviews(id, account_id, pr_id, author_id, state, body, commit_sha, submitted_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               pr_id = excluded.pr_id,
               author_id = excluded.author_id,
               state = excluded.state,
               body = excluded.body,
               commit_sha = excluded.commit_sha,
               submitted_at = excluded.submitted_at,
               updated_at = excluded.updated_at",
        )
        .bind(&review.id)
        .bind(&review.account_id)
        .bind(&review.pr_id)
        .bind(&review.author_id)
        .bind(&review.state)
        .bind(&review.body)
        .bind(&review.commit_sha)
        .bind(review.submitted_at)
        .bind(review.created_at)
        .bind(review.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_check_suite(&self, suite: &CheckSuiteRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO check_suites(id, account_id, pr_id, head_sha, app_name, status, conclusion, details_url, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               pr_id = excluded.pr_id,
               head_sha = excluded.head_sha,
               app_name = excluded.app_name,
               status = excluded.status,
               conclusion = excluded.conclusion,
               details_url = excluded.details_url,
               updated_at = excluded.updated_at",
        )
        .bind(&suite.id)
        .bind(&suite.account_id)
        .bind(&suite.pr_id)
        .bind(&suite.head_sha)
        .bind(&suite.app_name)
        .bind(&suite.status)
        .bind(&suite.conclusion)
        .bind(&suite.details_url)
        .bind(suite.created_at)
        .bind(suite.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_check_run(&self, run: &CheckRunRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO check_runs(
               id, account_id, check_suite_id, pr_id, name, status, conclusion, details_url, output_title,
               output_summary, started_at, completed_at, created_at, updated_at
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
             )
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               check_suite_id = excluded.check_suite_id,
               pr_id = excluded.pr_id,
               name = excluded.name,
               status = excluded.status,
               conclusion = excluded.conclusion,
               details_url = excluded.details_url,
               output_title = excluded.output_title,
               output_summary = excluded.output_summary,
               started_at = excluded.started_at,
               completed_at = excluded.completed_at,
               updated_at = excluded.updated_at",
        )
        .bind(&run.id)
        .bind(&run.account_id)
        .bind(&run.check_suite_id)
        .bind(&run.pr_id)
        .bind(&run.name)
        .bind(&run.status)
        .bind(&run.conclusion)
        .bind(&run.details_url)
        .bind(&run.output_title)
        .bind(&run.output_summary)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(run.created_at)
        .bind(run.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_check_annotation(&self, annotation: &CheckAnnotationRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO check_annotations(
               id, account_id, check_run_id, pr_id, path, start_line, end_line, start_column, end_column,
               annotation_level, title, message, raw_details
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               check_run_id = excluded.check_run_id,
               pr_id = excluded.pr_id,
               path = excluded.path,
               start_line = excluded.start_line,
               end_line = excluded.end_line,
               start_column = excluded.start_column,
               end_column = excluded.end_column,
               annotation_level = excluded.annotation_level,
               title = excluded.title,
               message = excluded.message,
               raw_details = excluded.raw_details",
        )
        .bind(&annotation.id)
        .bind(&annotation.account_id)
        .bind(&annotation.check_run_id)
        .bind(&annotation.pr_id)
        .bind(&annotation.path)
        .bind(annotation.start_line)
        .bind(annotation.end_line)
        .bind(annotation.start_column)
        .bind(annotation.end_column)
        .bind(&annotation.annotation_level)
        .bind(&annotation.title)
        .bind(&annotation.message)
        .bind(&annotation.raw_details)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_pr_file(&self, file: &PrFileRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pr_files(
               account_id, pr_id, head_sha, path, old_path, status, additions, deletions, is_binary,
               kind, previous_path, rename_similarity, patch_blob_sha, viewed_by_account_id, viewed_at_head_sha
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(account_id, pr_id, head_sha, path) DO UPDATE SET
               old_path = excluded.old_path,
               previous_path = COALESCE(excluded.previous_path, excluded.old_path),
               status = excluded.status,
               additions = excluded.additions,
               deletions = excluded.deletions,
               is_binary = excluded.is_binary,
               kind = excluded.kind,
               rename_similarity = excluded.rename_similarity,
               patch_blob_sha = excluded.patch_blob_sha,
               viewed_by_account_id = excluded.viewed_by_account_id,
               viewed_at_head_sha = excluded.viewed_at_head_sha",
        )
        .bind(&file.account_id)
        .bind(&file.pr_id)
        .bind(&file.head_sha)
        .bind(&file.path)
        .bind(&file.old_path)
        .bind(&file.status)
        .bind(file.additions)
        .bind(file.deletions)
        .bind(bool_to_i64(file.is_binary))
        .bind(&file.kind)
        .bind(&file.previous_path)
        .bind(file.rename_similarity)
        .bind(&file.patch_blob_sha)
        .bind(&file.viewed_by_account_id)
        .bind(&file.viewed_at_head_sha)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_pr_patch(&self, patch: &PrPatchRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pr_patches(account_id, pr_id, head_sha, patch_blob_sha, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, pr_id, head_sha) DO UPDATE SET
               patch_blob_sha = excluded.patch_blob_sha,
               fetched_at = excluded.fetched_at",
        )
        .bind(&patch.account_id)
        .bind(&patch.pr_id)
        .bind(&patch.head_sha)
        .bind(&patch.patch_blob_sha)
        .bind(patch.fetched_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_notification(&self, notification: &NotificationRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO notifications(
               id, account_id, repo_id, pr_id, reason, subject_type, subject_id, title, unread, updated_at, last_read_at, url
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               repo_id = excluded.repo_id,
               pr_id = excluded.pr_id,
               reason = excluded.reason,
               subject_type = excluded.subject_type,
               subject_id = excluded.subject_id,
               title = excluded.title,
               unread = excluded.unread,
               updated_at = excluded.updated_at,
               last_read_at = excluded.last_read_at,
               url = excluded.url",
        )
        .bind(&notification.id)
        .bind(&notification.account_id)
        .bind(&notification.repo_id)
        .bind(&notification.pr_id)
        .bind(&notification.reason)
        .bind(&notification.subject_type)
        .bind(&notification.subject_id)
        .bind(&notification.title)
        .bind(bool_to_i64(notification.unread))
        .bind(notification.updated_at)
        .bind(notification.last_read_at)
        .bind(&notification.url)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn worktree_by_id(&self, worktree_id: &str) -> Result<Option<WorktreeViewRow>> {
        let row = sqlx::query_as::<_, WorktreeViewRow>(
            "SELECT
               w.id,
               w.account_id,
               w.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               w.path,
               w.head_sha,
               w.branch,
               w.dirty,
               w.ahead,
               w.behind,
               w.untracked_count,
               w.staged_count,
               w.modified_count,
               w.mapped_pr_id,
               pr.number AS mapped_pr_number,
               w.mapping_confidence,
               w.mapping_source,
               w.is_app_managed,
               w.manual_override_pr_id,
               w.manual_override_at,
               w.last_cleanup_snapshot_id,
               w.created_at,
               w.updated_at
             FROM worktrees w
             JOIN repos r ON r.id = w.repo_id
             LEFT JOIN pull_requests pr ON pr.id = w.mapped_pr_id
             WHERE w.id = ?1
             LIMIT 1",
        )
        .bind(worktree_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_worktrees(&self, account_id: &str) -> Result<Vec<WorktreeViewRow>> {
        let rows = sqlx::query_as::<_, WorktreeViewRow>(
            "SELECT
               w.id,
               w.account_id,
               w.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               w.path,
               w.head_sha,
               w.branch,
               w.dirty,
               w.ahead,
               w.behind,
               w.untracked_count,
               w.staged_count,
               w.modified_count,
               w.mapped_pr_id,
               pr.number AS mapped_pr_number,
               w.mapping_confidence,
               w.mapping_source,
               w.is_app_managed,
               w.manual_override_pr_id,
               w.manual_override_at,
               w.last_cleanup_snapshot_id,
               w.created_at,
               w.updated_at
             FROM worktrees w
             JOIN repos r ON r.id = w.repo_id
             LEFT JOIN pull_requests pr ON pr.id = w.mapped_pr_id
             WHERE w.account_id = ?1
             ORDER BY w.updated_at DESC, w.path ASC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn worktree_by_account_path(
        &self,
        account_id: &str,
        path: &str,
    ) -> Result<Option<WorktreeViewRow>> {
        let row = sqlx::query_as::<_, WorktreeViewRow>(
            "SELECT
               w.id,
               w.account_id,
               w.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               w.path,
               w.head_sha,
               w.branch,
               w.dirty,
               w.ahead,
               w.behind,
               w.untracked_count,
               w.staged_count,
               w.modified_count,
               w.mapped_pr_id,
               pr.number AS mapped_pr_number,
               w.mapping_confidence,
               w.mapping_source,
               w.is_app_managed,
               w.manual_override_pr_id,
               w.manual_override_at,
               w.last_cleanup_snapshot_id,
               w.created_at,
               w.updated_at
             FROM worktrees w
             JOIN repos r ON r.id = w.repo_id
             LEFT JOIN pull_requests pr ON pr.id = w.mapped_pr_id
             WHERE w.account_id = ?1 AND w.path = ?2
             LIMIT 1",
        )
        .bind(account_id)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_worktrees_by_path(&self, path: &str) -> Result<Vec<WorktreeViewRow>> {
        let rows = sqlx::query_as::<_, WorktreeViewRow>(
            "SELECT
               w.id,
               w.account_id,
               w.repo_id,
               r.owner AS repo_owner,
               r.name AS repo_name,
               w.path,
               w.head_sha,
               w.branch,
               w.dirty,
               w.ahead,
               w.behind,
               w.untracked_count,
               w.staged_count,
               w.modified_count,
               w.mapped_pr_id,
               pr.number AS mapped_pr_number,
               w.mapping_confidence,
               w.mapping_source,
               w.is_app_managed,
               w.manual_override_pr_id,
               w.manual_override_at,
               w.last_cleanup_snapshot_id,
               w.created_at,
               w.updated_at
             FROM worktrees w
             JOIN repos r ON r.id = w.repo_id
             LEFT JOIN pull_requests pr ON pr.id = w.mapped_pr_id
             WHERE w.path = ?1
             ORDER BY w.updated_at DESC",
        )
        .bind(path)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_repo_owner_name_rows(&self) -> Result<Vec<RepoOwnerNameRow>> {
        let rows = sqlx::query_as::<_, RepoOwnerNameRow>(
            "SELECT account_id, id AS repo_id, owner, name
             FROM repos
             ORDER BY account_id ASC, owner ASC, name ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn list_pull_request_mapping_candidates(
        &self,
        account_id: &str,
        repo_id: &str,
    ) -> Result<Vec<PullRequestMappingCandidateRow>> {
        let rows = sqlx::query_as::<_, PullRequestMappingCandidateRow>(
            "SELECT
               pr.id,
               pr.account_id,
               pr.repo_id,
               repo.owner AS repo_owner,
               repo.name AS repo_name,
               pr.number,
               pr.head_ref,
               pr.head_sha,
               pr.head_repo_id,
               hr.owner AS head_repo_owner
             FROM pull_requests pr
             JOIN repos repo ON repo.id = pr.repo_id
             LEFT JOIN repos hr ON hr.id = pr.head_repo_id
             WHERE pr.account_id = ?1
               AND pr.repo_id = ?2
               AND pr.state = 'open'
             ORDER BY pr.updated_at DESC, pr.number DESC",
        )
        .bind(account_id)
        .bind(repo_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn resolve_pr_id_by_number(
        &self,
        account_id: &str,
        repo_id: &str,
        number: i64,
    ) -> Result<Option<String>> {
        let pr_id = sqlx::query_scalar::<_, String>(
            "SELECT id
             FROM pull_requests
             WHERE account_id = ?1 AND repo_id = ?2 AND number = ?3
             LIMIT 1",
        )
        .bind(account_id)
        .bind(repo_id)
        .bind(number)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pr_id)
    }

    pub async fn set_worktree_manual_override(
        &self,
        worktree_id: &str,
        manual_override_pr_id: Option<&str>,
        now_epoch: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE worktrees
             SET manual_override_pr_id = ?2,
                 manual_override_at = CASE
                   WHEN ?2 IS NULL THEN NULL
                   ELSE ?3
                 END,
                 updated_at = ?3
             WHERE id = ?1",
        )
        .bind(worktree_id)
        .bind(manual_override_pr_id)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_worktree_mapping(
        &self,
        worktree_id: &str,
        mapped_pr_id: Option<&str>,
        mapping_confidence: Option<f64>,
        mapping_source: Option<&str>,
        now_epoch: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE worktrees
             SET mapped_pr_id = ?2,
                 mapping_confidence = ?3,
                 mapping_source = ?4,
                 updated_at = ?5
             WHERE id = ?1",
        )
        .bind(worktree_id)
        .bind(mapped_pr_id)
        .bind(mapping_confidence)
        .bind(mapping_source)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_worktree_cleanup_snapshot(
        &self,
        worktree_id: &str,
        snapshot_sha: &str,
        now_epoch: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE worktrees
             SET last_cleanup_snapshot_id = ?2, updated_at = ?3
             WHERE id = ?1",
        )
        .bind(worktree_id)
        .bind(snapshot_sha)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_worktree_roots(&self) -> Result<Option<Vec<String>>> {
        let row = sqlx::query_as::<_, WorktreeSettingRow>(
            "SELECT id, key, value_json, created_at, updated_at
             FROM worktree_settings
             WHERE key = 'roots'
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let roots: Vec<String> = serde_json::from_str(&row.value_json)
            .with_context(|| format!("parsing worktree roots json for setting `{}`", row.id))?;
        Ok(Some(roots))
    }

    pub async fn set_worktree_roots(&self, roots: &[String], now_epoch: i64) -> Result<()> {
        let value_json =
            serde_json::to_string(roots).context("serializing worktree roots setting")?;
        sqlx::query(
            "INSERT INTO worktree_settings(id, key, value_json, created_at, updated_at)
             VALUES ('worktree_settings:roots', 'roots', ?1, ?2, ?2)
             ON CONFLICT(key) DO UPDATE SET
               value_json = excluded.value_json,
               updated_at = excluded.updated_at",
        )
        .bind(value_json)
        .bind(now_epoch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_worktree(&self, worktree: &WorktreeRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO worktrees(
               id, account_id, repo_id, path, head_sha, branch, dirty, ahead, behind, mapped_pr_id,
               mapping_confidence, mapping_source, is_app_managed, manual_override_pr_id,
               manual_override_at, last_cleanup_snapshot_id, untracked_count, staged_count,
               modified_count, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               repo_id = excluded.repo_id,
               path = excluded.path,
               head_sha = excluded.head_sha,
               branch = excluded.branch,
               dirty = excluded.dirty,
               ahead = excluded.ahead,
               behind = excluded.behind,
               mapped_pr_id = excluded.mapped_pr_id,
               mapping_confidence = excluded.mapping_confidence,
               mapping_source = excluded.mapping_source,
               is_app_managed = excluded.is_app_managed,
               manual_override_pr_id = excluded.manual_override_pr_id,
               manual_override_at = excluded.manual_override_at,
               last_cleanup_snapshot_id = excluded.last_cleanup_snapshot_id,
               untracked_count = excluded.untracked_count,
               staged_count = excluded.staged_count,
               modified_count = excluded.modified_count,
               updated_at = excluded.updated_at",
        )
        .bind(&worktree.id)
        .bind(&worktree.account_id)
        .bind(&worktree.repo_id)
        .bind(&worktree.path)
        .bind(&worktree.head_sha)
        .bind(&worktree.branch)
        .bind(bool_to_i64(worktree.dirty))
        .bind(worktree.ahead)
        .bind(worktree.behind)
        .bind(&worktree.mapped_pr_id)
        .bind(worktree.mapping_confidence)
        .bind(&worktree.mapping_source)
        .bind(bool_to_i64(worktree.is_app_managed))
        .bind(&worktree.manual_override_pr_id)
        .bind(worktree.manual_override_at)
        .bind(&worktree.last_cleanup_snapshot_id)
        .bind(worktree.untracked_count)
        .bind(worktree.staged_count)
        .bind(worktree.modified_count)
        .bind(worktree.created_at)
        .bind(worktree.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_draft(&self, draft: &DraftRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO drafts(id, account_id, target_type, target_id, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               account_id = excluded.account_id,
               target_type = excluded.target_type,
               target_id = excluded.target_id,
               body = excluded.body,
               updated_at = excluded.updated_at",
        )
        .bind(&draft.id)
        .bind(&draft.account_id)
        .bind(&draft.target_type)
        .bind(&draft.target_id)
        .bind(&draft.body)
        .bind(draft.created_at)
        .bind(draft.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_drafts(
        &self,
        account_id: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
    ) -> Result<Vec<DraftRow>> {
        let rows = sqlx::query_as::<_, DraftRow>(
            "SELECT id, account_id, target_type, target_id, body, created_at, updated_at
             FROM drafts
             WHERE account_id = ?1
               AND (?2 IS NULL OR target_type = ?2)
               AND (?3 IS NULL OR target_id = ?3)
             ORDER BY updated_at DESC, id DESC",
        )
        .bind(account_id)
        .bind(target_type)
        .bind(target_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn delete_draft(&self, draft_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM drafts WHERE id = ?1")
            .bind(draft_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_pending_mutations(
        &self,
        account_id: &str,
        include_pending: bool,
    ) -> Result<Vec<PendingMutationRow>> {
        let rows = sqlx::query_as::<_, PendingMutationRow>(
            "SELECT
               id,
               account_id,
               kind,
               target_type,
               target_id,
               status,
               retries,
               created_at,
               updated_at,
               last_error,
               optimism_level,
               optimistic_patch_json,
               requires_connection_confirmation
             FROM pending_mutations
             WHERE account_id = ?1
               AND status IN ('failed', 'pending')
               AND (?2 = 1 OR status = 'failed')
             ORDER BY updated_at DESC, id DESC",
        )
        .bind(account_id)
        .bind(if include_pending { 1_i64 } else { 0_i64 })
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn apply_pending_mutation(&self, mutation: &PendingMutationRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pending_mutations(
               id, account_id, kind, target_type, target_id, idempotency_key, input_json,
               optimistic_patch_json, inverse_patch_json, status, retries, created_at, updated_at,
               last_error, requires_connection_confirmation
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
               kind = excluded.kind,
               target_type = excluded.target_type,
               target_id = excluded.target_id,
               idempotency_key = excluded.idempotency_key,
               input_json = excluded.input_json,
               optimistic_patch_json = excluded.optimistic_patch_json,
               inverse_patch_json = excluded.inverse_patch_json,
               status = excluded.status,
               retries = excluded.retries,
               updated_at = excluded.updated_at,
               last_error = excluded.last_error,
               requires_connection_confirmation = excluded.requires_connection_confirmation",
        )
        .bind(&mutation.id)
        .bind(&mutation.account_id)
        .bind(&mutation.kind)
        .bind(&mutation.target_type)
        .bind(&mutation.target_id)
        .bind(&mutation.idempotency_key)
        .bind(&mutation.input_json)
        .bind(&mutation.optimistic_patch_json)
        .bind(&mutation.inverse_patch_json)
        .bind(&mutation.status)
        .bind(mutation.retries)
        .bind(mutation.created_at)
        .bind(mutation.updated_at)
        .bind(&mutation.last_error)
        .bind(bool_to_i64(mutation.requires_connection_confirmation))
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_id_mapping(&self, mapping: &IdMappingRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO id_mappings(account_id, kind, local_id, server_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account_id, kind, local_id) DO UPDATE SET
               server_id = excluded.server_id",
        )
        .bind(&mapping.account_id)
        .bind(&mapping.kind)
        .bind(&mapping.local_id)
        .bind(&mapping.server_id)
        .bind(mapping.created_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn update_sync_cursor(&self, update: &SyncCursorUpdate) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let existing_etag: Option<String> = sqlx::query_scalar(
            "SELECT etag FROM sync_cursors WHERE account_id = ?1 AND resource = ?2",
        )
        .bind(&update.account_id)
        .bind(&update.resource)
        .fetch_optional(tx.as_mut())
        .await?;

        if let Some(expected) = &update.expected_previous_etag {
            if existing_etag.as_deref() != Some(expected.as_str()) {
                tx.rollback().await?;
                return Ok(false);
            }
        }

        sqlx::query(
            "INSERT INTO sync_cursors(account_id, resource, cursor, etag, last_fetched_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(account_id, resource) DO UPDATE SET
               cursor = excluded.cursor,
               etag = excluded.etag,
               last_fetched_at = excluded.last_fetched_at,
               updated_at = excluded.updated_at",
        )
        .bind(&update.account_id)
        .bind(&update.resource)
        .bind(&update.cursor)
        .bind(&update.etag)
        .bind(update.fetched_at)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        Ok(true)
    }

    pub async fn sync_cursor(
        &self,
        account_id: &str,
        resource: &str,
    ) -> Result<Option<SyncCursorRow>> {
        let row = sqlx::query_as::<_, SyncCursorRow>(
            "SELECT account_id, resource, cursor, etag, last_fetched_at, updated_at
             FROM sync_cursors
             WHERE account_id = ?1 AND resource = ?2",
        )
        .bind(account_id)
        .bind(resource)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn update_rate_limit_bucket(&self, bucket: &RateLimitBucketUpdate) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO rate_limit_buckets(account_id, resource, remaining, used, limit_total, reset_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(account_id, resource) DO UPDATE SET
               remaining = excluded.remaining,
               used = excluded.used,
               limit_total = excluded.limit_total,
               reset_at = excluded.reset_at,
               updated_at = excluded.updated_at",
        )
        .bind(&bucket.account_id)
        .bind(&bucket.resource)
        .bind(bucket.remaining)
        .bind(bucket.used)
        .bind(bucket.limit_total)
        .bind(bucket.reset_at)
        .bind(bucket.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn rate_limit_bucket(
        &self,
        account_id: &str,
        resource: &str,
    ) -> Result<Option<RateLimitBucketRow>> {
        let row = sqlx::query_as::<_, RateLimitBucketRow>(
            "SELECT account_id, resource, remaining, used, limit_total, reset_at, updated_at
             FROM account_rate_limits
             WHERE account_id = ?1 AND resource = ?2",
        )
        .bind(account_id)
        .bind(resource)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn open_with_paths(
        app_data_dir: PathBuf,
        db_path: PathBuf,
        fixture_guard: Option<TempDir>,
    ) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating sqlite parent dir {}", parent.display()))?;
        }

        let connect_options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .pragma("temp_store", "MEMORY");

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(connect_options)
            .await
            .with_context(|| format!("opening sqlite database {}", db_path.display()))?;

        MIGRATOR.run(&pool).await?;
        maybe_vacuum(&pool, &app_data_dir).await?;

        let blob_store = BlobStore::new(app_data_dir.join("blobs"), pool.clone(), 3).await?;
        Ok(Self {
            pool,
            blob_store,
            _fixture_guard: fixture_guard,
        })
    }
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn parse_pending_overlay(value: Option<String>) -> Result<Option<PendingOverlay>> {
    value
        .map(|raw| {
            serde_json::from_str::<PendingOverlay>(&raw)
                .with_context(|| format!("parsing pending overlay json `{raw}`"))
        })
        .transpose()
}

fn account_id_from_host_login(host: &str, login: &str) -> String {
    format!("{host}:{login}")
}

fn now_epoch_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(duration.as_secs()).context("unix timestamp exceeds i64")
}

async fn maybe_vacuum(pool: &SqlitePool, app_data_dir: &Path) -> Result<()> {
    let marker_path = app_data_dir.join(".last_vacuum_epoch");
    let now = now_epoch_seconds()?;
    let previous = read_vacuum_marker(&marker_path).await.unwrap_or(0);
    if now - previous < 7 * 24 * 60 * 60 {
        return Ok(());
    }

    sqlx::query("VACUUM").execute(pool).await?;
    fs::write(&marker_path, now.to_string())
        .await
        .with_context(|| format!("writing vacuum marker {}", marker_path.display()))?;
    Ok(())
}

async fn read_vacuum_marker(path: &Path) -> Result<i64> {
    let contents = fs::read_to_string(path)
        .await
        .with_context(|| format!("reading vacuum marker {}", path.display()))?;
    contents
        .trim()
        .parse::<i64>()
        .with_context(|| format!("parsing vacuum marker {}", path.display()))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("creating destination dir {}", dst.display()))?;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        std::fs::create_dir_all(&to)
            .with_context(|| format!("creating destination dir {}", to.display()))?;
        for entry in std::fs::read_dir(&from)
            .with_context(|| format!("reading source dir {}", from.display()))?
        {
            let entry = entry?;
            let source_path = entry.path();
            let target_path = to.join(entry.file_name());
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push((source_path, target_path));
            } else if file_type.is_file() {
                std::fs::copy(&source_path, &target_path).with_context(|| {
                    format!(
                        "copying fixture blob {} -> {}",
                        source_path.display(),
                        target_path.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

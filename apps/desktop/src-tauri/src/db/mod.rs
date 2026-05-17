pub mod blob_store;
pub mod types;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
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
        let rows = sqlx::query_as::<_, InboxRow>(
            "SELECT account_id, pr_id, repo_id, repo_owner, repo_name, pr_number, title, state,
                    draft, head_sha, base_sha, mergeable_state, merge_state_status, updated_at,
                    author_login, unread_notification_count, latest_notification_at
             FROM pr_inbox_rows
             WHERE account_id = ?1
             ORDER BY unread_notification_count DESC, updated_at DESC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn pr_detail_summary(
        &self,
        account_id: &str,
        pr_id: &str,
    ) -> Result<Option<PrDetailSummaryRow>> {
        let row = sqlx::query_as::<_, PrDetailSummaryRow>(
            "SELECT account_id, pr_id, repo_id, pr_number, title, body, state, draft, base_ref,
                    base_sha, head_ref, head_sha, mergeable_state, merge_state_status, additions,
                    deletions, changed_files, comment_count, review_count, thread_count,
                    check_run_count, file_count, updated_at
             FROM pr_detail_summary
             WHERE account_id = ?1 AND pr_id = ?2",
        )
        .bind(account_id)
        .bind(pr_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn pr_files(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Vec<PrFileRow>> {
        let rows = sqlx::query_as::<_, PrFileRow>(
            "SELECT account_id, pr_id, head_sha, path, old_path, status, additions, deletions,
                    is_binary, patch_blob_sha, viewed_by_account_id, viewed_at_head_sha
             FROM pr_files
             WHERE account_id = ?1 AND pr_id = ?2 AND head_sha = ?3
             ORDER BY path ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
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

    pub async fn unread_counts(&self, account_id: &str) -> Result<Option<UnreadCountsRow>> {
        let row = sqlx::query_as::<_, UnreadCountsRow>(
            "SELECT account_id, total_notifications, unread_notifications, prs_with_unread
             FROM unread_counts
             WHERE account_id = ?1",
        )
        .bind(account_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn file_tree_summary(
        &self,
        account_id: &str,
        pr_id: &str,
        head_sha: &str,
    ) -> Result<Vec<FileTreeSummaryRow>> {
        let rows = sqlx::query_as::<_, FileTreeSummaryRow>(
            "SELECT account_id, pr_id, head_sha, directory, file_count, additions, deletions
             FROM file_tree_summary
             WHERE account_id = ?1 AND pr_id = ?2 AND head_sha = ?3
             ORDER BY directory ASC",
        )
        .bind(account_id)
        .bind(pr_id)
        .bind(head_sha)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
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
               head_repo_id, mergeable_state, merge_state_status, additions, deletions, changed_files, comments_count, reviews_count,
               commits_count, is_read, html_url, created_at, updated_at, closed_at, merged_at
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
               ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28
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
               patch_blob_sha, viewed_by_account_id, viewed_at_head_sha
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(account_id, pr_id, head_sha, path) DO UPDATE SET
               old_path = excluded.old_path,
               status = excluded.status,
               additions = excluded.additions,
               deletions = excluded.deletions,
               is_binary = excluded.is_binary,
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

    pub async fn upsert_worktree(&self, worktree: &WorktreeRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO worktrees(
               id, account_id, repo_id, path, head_sha, branch, dirty, ahead, behind, mapped_pr_id,
               mapping_confidence, mapping_source, is_app_managed, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
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
        .bind(worktree.created_at)
        .bind(worktree.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn apply_pending_mutation(&self, mutation: &PendingMutationRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO pending_mutations(
               id, account_id, kind, target_type, target_id, idempotency_key, input_json,
               optimistic_patch_json, inverse_patch_json, status, retries, created_at, updated_at, last_error
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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
               last_error = excluded.last_error",
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

    pub async fn update_rate_limit_bucket(&self, bucket: &RateLimitBucketUpdate) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO rate_limit_buckets(account_id, resource, remaining, limit_total, reset_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, resource) DO UPDATE SET
               remaining = excluded.remaining,
               limit_total = excluded.limit_total,
               reset_at = excluded.reset_at,
               updated_at = excluded.updated_at",
        )
        .bind(&bucket.account_id)
        .bind(&bucket.resource)
        .bind(bucket.remaining)
        .bind(bucket.limit_total)
        .bind(bucket.reset_at)
        .bind(bucket.updated_at)
        .execute(tx.as_mut())
        .await?;
        tx.commit().await?;
        Ok(())
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

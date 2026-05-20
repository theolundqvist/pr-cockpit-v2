#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use desktop_lib::db::{
    Db, PrReviewerRecord, PullRequestRecord, RepoRecord, ReviewRecord, UserRecord,
};
use desktop_lib::mutations::{ErrorKind, MutationEvent};
use desktop_lib::notify::dispatcher::RecordingNotificationSender;
use desktop_lib::notify::rules;
use desktop_lib::notify::{
    DebugNotificationInput, NotificationEngine, NotificationEventEmitter, NotificationEventPayload,
};

#[derive(Clone, Default)]
struct NoopNotificationEmitter;

impl NotificationEventEmitter for NoopNotificationEmitter {
    fn emit_notification_event(&self, _payload: NotificationEventPayload) {}
}

pub struct NotificationHarness {
    _temp: tempfile::TempDir,
    pub db: Arc<Db>,
    pub engine: Arc<NotificationEngine>,
    pub sender: RecordingNotificationSender,
    pub account_id: String,
    pub repo_id: String,
    pub pr_id: String,
    pub viewer_user_id: String,
    pub other_user_id: String,
}

impl NotificationHarness {
    pub async fn new() -> Result<Self> {
        let temp = tempfile::TempDir::new()?;
        let db = Arc::new(Db::open(temp.path()).await?);
        let account = db
            .upsert_auth_account("github.com", "fixture-user", "pat", "repo,read:org", 1)
            .await?;

        let repo_id = "repo_1".to_string();
        db.upsert_repo(&RepoRecord {
            id: repo_id.clone(),
            account_id: account.id.clone(),
            owner: "acme".to_string(),
            name: "repo".to_string(),
            default_branch: Some("main".to_string()),
            description: Some("fixture".to_string()),
            html_url: Some("https://example.test/acme/repo".to_string()),
            is_private: false,
            is_archived: false,
            pushed_at: None,
            created_at: 1,
            updated_at: 1,
        })
        .await?;

        let viewer_user_id = "user_viewer".to_string();
        db.upsert_user(&UserRecord {
            id: viewer_user_id.clone(),
            account_id: account.id.clone(),
            login: "fixture-user".to_string(),
            display_name: None,
            avatar_url: None,
            html_url: None,
            created_at: 1,
            updated_at: 1,
        })
        .await?;
        let other_user_id = "user_other".to_string();
        db.upsert_user(&UserRecord {
            id: other_user_id.clone(),
            account_id: account.id.clone(),
            login: "reviewer".to_string(),
            display_name: None,
            avatar_url: None,
            html_url: None,
            created_at: 1,
            updated_at: 1,
        })
        .await?;

        let pr_id = "pr_1".to_string();
        db.upsert_pull_request(&PullRequestRecord {
            id: pr_id.clone(),
            account_id: account.id.clone(),
            repo_id: repo_id.clone(),
            number: 1,
            state: "open".to_string(),
            draft: false,
            title: "fixture".to_string(),
            body: String::new(),
            author_id: Some(viewer_user_id.clone()),
            base_ref: "main".to_string(),
            base_sha: "base".to_string(),
            head_ref: "feature".to_string(),
            head_sha: "head".to_string(),
            head_repo_id: Some(repo_id.clone()),
            mergeable_state: Some("clean".to_string()),
            merge_state_status: Some("success".to_string()),
            merge_commit_allowed: None,
            squash_merge_allowed: None,
            rebase_merge_allowed: None,
            delete_branch_on_merge_default: None,
            viewer_can_merge: None,
            viewer_can_enable_auto_merge: None,
            viewer_can_disable_auto_merge: None,
            viewer_can_update_branch: None,
            viewer_can_delete_head_ref: None,
            auto_merge_enabled: None,
            auto_merge_method: None,
            auto_merge_commit_headline: None,
            auto_merge_commit_body: None,
            auto_merge_enabled_by_login: None,
            auto_merge_enabled_at: None,
            merge_queue_entry_id: None,
            merge_queue_entry_position: None,
            merge_queue_entry_state: None,
            merge_queue_entry_estimated_ms: None,
            branch_protection_summary_json: None,
            repo_has_merge_queue: None,
            head_ref_state: None,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            comments_count: 0,
            reviews_count: 0,
            commits_count: 0,
            is_read: true,
            html_url: None,
            created_at: 1,
            updated_at: 1,
            closed_at: None,
            merged_at: None,
        })
        .await?;

        let sender = RecordingNotificationSender::default();
        let engine = Arc::new(
            NotificationEngine::new(
                Arc::clone(&db),
                Arc::new(sender.clone()),
                Arc::new(NoopNotificationEmitter),
            )
            .await?,
        );
        Ok(Self {
            _temp: temp,
            db,
            engine,
            sender,
            account_id: account.id,
            repo_id,
            pr_id,
            viewer_user_id,
            other_user_id,
        })
    }

    pub async fn trigger_sync(&self) -> Result<()> {
        self.engine.process_sync_reconciled(&self.account_id).await
    }

    pub async fn add_review_request_for_viewer(&self, requested_at: i64) -> Result<()> {
        self.db
            .replace_pr_reviewers(
                &self.account_id,
                &self.pr_id,
                &[PrReviewerRecord {
                    account_id: self.account_id.clone(),
                    pr_id: self.pr_id.clone(),
                    user_id: self.viewer_user_id.clone(),
                    reviewer_type: "user".to_string(),
                    reviewer_state: "requested".to_string(),
                    requested_at,
                }],
            )
            .await
    }

    pub async fn add_review(&self, review_id: &str, state: &str, updated_at: i64) -> Result<()> {
        self.db
            .upsert_review(&ReviewRecord {
                id: review_id.to_string(),
                account_id: self.account_id.clone(),
                pr_id: self.pr_id.clone(),
                author_id: self.other_user_id.clone(),
                state: state.to_string(),
                body: String::new(),
                commit_sha: None,
                submitted_at: Some(updated_at),
                created_at: updated_at,
                updated_at,
            })
            .await
    }

    pub async fn add_comment(&self, comment_id: &str, body: &str, updated_at: i64) -> Result<()> {
        self.db
            .upsert_comment(&desktop_lib::db::CommentRecord {
                id: comment_id.to_string(),
                account_id: self.account_id.clone(),
                pr_id: self.pr_id.clone(),
                kind: "issue".to_string(),
                author_id: self.other_user_id.clone(),
                body: body.to_string(),
                created_at: updated_at,
                updated_at,
                deleted_at: None,
                in_reply_to_id: None,
                review_id: None,
                thread_id: None,
                path: None,
                line: None,
                side: None,
                start_line: None,
                start_side: None,
                original_commit_sha: None,
            })
            .await
    }

    pub async fn set_check_red(&self, red: bool, updated_at: i64) -> Result<()> {
        self.db
            .upsert_check_suite(&desktop_lib::db::CheckSuiteRecord {
                id: "suite_1".to_string(),
                account_id: self.account_id.clone(),
                pr_id: self.pr_id.clone(),
                head_sha: "head".to_string(),
                app_name: "ci".to_string(),
                status: "completed".to_string(),
                conclusion: Some(if red { "FAILURE" } else { "SUCCESS" }.to_string()),
                details_url: None,
                created_at: updated_at,
                updated_at,
            })
            .await
    }

    pub async fn set_mergeable_state(&self, state: &str, updated_at: i64) -> Result<()> {
        sqlx::query(
            "UPDATE pull_requests
             SET mergeable_state = ?3, updated_at = ?4
             WHERE account_id = ?1 AND id = ?2",
        )
        .bind(&self.account_id)
        .bind(&self.pr_id)
        .bind(state)
        .bind(updated_at)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn send_non_network_mutation_failure(&self, mutation_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO pending_mutations(
               id,
               account_id,
               kind,
               target_type,
               target_id,
               idempotency_key,
               input_json,
               optimistic_patch_json,
               inverse_patch_json,
               status,
               retries,
               created_at,
               updated_at,
               last_error,
               requires_connection_confirmation
             )
             VALUES (?1, ?2, 'add_comment', 'pull_request', ?3, ?4, '{}', '{\"operations\":[]}', '{\"operations\":[]}', 'failed', 1, 1, 1, NULL, 0)",
        )
        .bind(mutation_id)
        .bind(&self.account_id)
        .bind(&self.pr_id)
        .bind(format!("idem-{mutation_id}"))
        .execute(self.db.pool())
        .await?;
        self.engine
            .process_mutation_event(MutationEvent::Failed {
                mutation_id: mutation_id.to_string(),
                error_kind: ErrorKind::Server,
                retryable: false,
                hard_conflict: None,
            })
            .await
    }

    pub async fn simulate_debug_event(&self, payload: DebugNotificationInput) -> Result<()> {
        let _ = self
            .engine
            .simulate_event(&self.account_id, payload)
            .await?;
        Ok(())
    }

    pub async fn set_focus_mode(&self, on: bool) -> Result<()> {
        rules::set_focus_mode(self.db.as_ref(), &self.account_id, on).await
    }

    pub async fn set_repo_filters(&self, allow: &[String], deny: &[String]) -> Result<()> {
        rules::set_per_repo_filters(self.db.as_ref(), &self.account_id, allow, deny).await
    }

    pub async fn set_quiet_hours(&self, json: &str) -> Result<()> {
        rules::set_quiet_hours(self.db.as_ref(), &self.account_id, Some(json)).await
    }

    pub async fn notification_event_count(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM notification_events WHERE account_id = ?1",
        )
        .bind(&self.account_id)
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }

    pub async fn suppressed_event_count(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM notification_events WHERE account_id = ?1 AND deduped = 1",
        )
        .bind(&self.account_id)
        .fetch_one(self.db.pool())
        .await?;
        Ok(count)
    }
}

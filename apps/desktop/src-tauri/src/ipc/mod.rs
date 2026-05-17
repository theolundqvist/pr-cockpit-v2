use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result as AnyResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Emitter;

use crate::auth::{
    self, AccountLocator, AccountsListResponse, AuthAccount, AuthCommandError, AuthService,
};
use crate::db::{
    CheckRunSummaryRow, Db, FileTreeSummaryRow, InboxRow, NotificationListRow, PrAssigneeRow,
    PrDetailSummaryRow, PrFileRow, PrLabelRow, PrMilestoneRow, PrProjectRow, PrReviewerRow,
    RateLimitBucketRow, RepoSubscriptionRow, ReviewThreadRow, TimelineRow,
};
use crate::mutations::{HardConflictPayload, NetState};
use crate::render::{self, RenderCtx};
use crate::sync::{CacheInvalidationEmitter, SyncSystemSnapshot, SyncTierStateStore};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    fn db(error: anyhow::Error) -> Self {
        Self {
            code: "DatabaseFailure".to_string(),
            message: error.to_string(),
        }
    }
}

impl From<AuthCommandError> for IpcError {
    fn from(value: AuthCommandError) -> Self {
        Self {
            code: format!("{:?}", value.code),
            message: value.message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AccountSwitchInput {
    pub host: String,
    pub login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct InboxListInput {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct InboxItem {
    pub account_id: String,
    pub pr_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub pr_number: i64,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub head_sha: String,
    pub base_sha: String,
    pub mergeable_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub updated_at: i64,
    pub author_login: String,
    pub unread_notification_count: i64,
    pub latest_notification_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrHandleInput {
    pub account_id: String,
    pub pr_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrDetailSummary {
    pub account_id: String,
    pub pr_id: String,
    pub repo_id: String,
    pub pr_number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub draft: bool,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
    pub mergeable_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    pub changed_files: i64,
    pub comment_count: i64,
    pub review_count: i64,
    pub thread_count: i64,
    pub check_run_count: i64,
    pub file_count: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PagedPrInput {
    pub account_id: String,
    pub pr_id: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct TimelineItem {
    pub item_id: String,
    pub item_kind: String,
    pub body: String,
    pub author_login: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub review_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct TimelinePage {
    pub items: Vec<TimelineItem>,
    pub next_offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub start_line: Option<i64>,
    pub start_side: Option<String>,
    pub is_outdated: bool,
    pub is_resolved: bool,
    pub resolved_by_login: Option<String>,
    pub updated_at: i64,
    pub comment_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ReviewThreadsPage {
    pub threads: Vec<ReviewThread>,
    pub next_offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CheckSummaryInput {
    pub account_id: String,
    pub pr_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CheckRunSummary {
    pub id: String,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrCheckSummary {
    pub total_runs: i64,
    pub successful_runs: i64,
    pub failed_runs: i64,
    pub pending_runs: i64,
    pub runs: Vec<CheckRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrFilesInput {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrFile {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
    pub is_binary: bool,
    pub patch_blob_sha: Option<String>,
    pub viewed_by_account_id: Option<String>,
    pub viewed_at_head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct FileTreeSummary {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub directory: String,
    pub file_count: i64,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrFilesResponse {
    pub files: Vec<PrFile>,
    pub tree: Vec<FileTreeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrPatchInput {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrPatchResponse {
    pub patch_blob_sha: Option<String>,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RenderedCommentInput {
    pub body: String,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RenderedCommentHtml {
    pub html: String,
    pub cache_hit: bool,
    pub content_hash: String,
    pub cache_key: String,
    pub renderer_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationsListInput {
    pub account_id: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationItem {
    pub id: String,
    pub reason: String,
    pub title: String,
    pub unread: bool,
    pub updated_at: i64,
    pub pr_id: Option<String>,
    pub repo_owner: String,
    pub repo_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct SystemStatusInput {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RateLimitBucket {
    pub account_id: String,
    pub resource: String,
    pub remaining: i64,
    pub limit_total: i64,
    pub reset_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct SystemStatusResponse {
    pub rate_limits: Vec<RateLimitBucket>,
    pub sync: SyncSystemSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RepoSubscriptionItem {
    pub account_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub watch_tier: String,
    pub last_full_sync_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RepoSubscriptionsInput {
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrLabel {
    pub label_name: String,
    pub label_color: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrParticipant {
    pub user_id: String,
    pub login: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrReviewer {
    pub user_id: String,
    pub login: Option<String>,
    pub reviewer_type: String,
    pub reviewer_state: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrProject {
    pub project_id: String,
    pub project_title: String,
    pub item_id: Option<String>,
    pub status: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrMilestone {
    pub milestone_id: String,
    pub title: String,
    pub state: String,
    pub due_on: Option<i64>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrMetadataResponse {
    pub labels: Vec<PrLabel>,
    pub assignees: Vec<PrParticipant>,
    pub requested_reviewers: Vec<PrReviewer>,
    pub projects: Vec<PrProject>,
    pub milestones: Vec<PrMilestone>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct InitInboxResponse {
    pub active_account_id: Option<String>,
    pub accounts: AccountsListResponse,
    pub subscriptions: Vec<RepoSubscriptionItem>,
    pub inbox: Vec<InboxItem>,
    pub status: Option<SystemStatusResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrChangedEventPayload {
    pub pr_id: String,
}

impl tauri_specta::Event for PrChangedEventPayload {
    const NAME: &'static str = "pr:<id> changed";
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct InboxChangedEventPayload {
    pub account_id: String,
}

impl tauri_specta::Event for InboxChangedEventPayload {
    const NAME: &'static str = "inbox:account:<id> changed";
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RateLimitChangedEventPayload {
    pub account_id: String,
}

impl tauri_specta::Event for RateLimitChangedEventPayload {
    const NAME: &'static str = "rate_limit:account:<id> changed";
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationsChangedEventPayload {
    pub account_id: String,
}

impl tauri_specta::Event for NotificationsChangedEventPayload {
    const NAME: &'static str = "notifications:account:<id> changed";
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NetworkChangedEventPayload {
    pub account_id: String,
    pub state: NetState,
}

impl tauri_specta::Event for NetworkChangedEventPayload {
    const NAME: &'static str = "network:<account_id> changed";
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct MutationHardConflictEventPayload {
    pub mutation_id: String,
    pub conflict: HardConflictPayload,
}

impl tauri_specta::Event for MutationHardConflictEventPayload {
    const NAME: &'static str = "mutation:<id> hard-conflict";
}

fn normalize_page(input_limit: Option<i64>, input_offset: Option<i64>) -> (i64, i64) {
    let limit = input_limit.unwrap_or(50).clamp(1, 200);
    let offset = input_offset.unwrap_or(0).max(0);
    (limit, offset)
}

fn account_id_from_locator(locator: &AccountLocator) -> String {
    format!("{}:{}", locator.host, locator.login)
}

fn map_inbox_row(row: InboxRow) -> InboxItem {
    InboxItem {
        account_id: row.account_id,
        pr_id: row.pr_id,
        repo_id: row.repo_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        pr_number: row.pr_number,
        title: row.title,
        state: row.state,
        draft: row.draft == 1,
        head_sha: row.head_sha,
        base_sha: row.base_sha,
        mergeable_state: row.mergeable_state,
        merge_state_status: row.merge_state_status,
        updated_at: row.updated_at,
        author_login: row.author_login,
        unread_notification_count: row.unread_notification_count,
        latest_notification_at: row.latest_notification_at,
    }
}

fn map_pr_detail_summary(row: PrDetailSummaryRow) -> PrDetailSummary {
    PrDetailSummary {
        account_id: row.account_id,
        pr_id: row.pr_id,
        repo_id: row.repo_id,
        pr_number: row.pr_number,
        title: row.title,
        body: row.body,
        state: row.state,
        draft: row.draft == 1,
        base_ref: row.base_ref,
        base_sha: row.base_sha,
        head_ref: row.head_ref,
        head_sha: row.head_sha,
        mergeable_state: row.mergeable_state,
        merge_state_status: row.merge_state_status,
        additions: row.additions,
        deletions: row.deletions,
        changed_files: row.changed_files,
        comment_count: row.comment_count,
        review_count: row.review_count,
        thread_count: row.thread_count,
        check_run_count: row.check_run_count,
        file_count: row.file_count,
        updated_at: row.updated_at,
    }
}

fn map_timeline_row(row: TimelineRow) -> TimelineItem {
    TimelineItem {
        item_id: row.item_id,
        item_kind: row.item_kind,
        body: row.body,
        author_login: row.author_login,
        created_at: row.created_at,
        updated_at: row.updated_at,
        review_state: row.review_state,
    }
}

fn map_review_thread_row(row: ReviewThreadRow) -> ReviewThread {
    ReviewThread {
        id: row.id,
        path: row.path,
        line: row.line,
        side: row.side,
        start_line: row.start_line,
        start_side: row.start_side,
        is_outdated: row.is_outdated == 1,
        is_resolved: row.is_resolved == 1,
        resolved_by_login: row.resolved_by_login,
        updated_at: row.updated_at,
        comment_count: row.comment_count,
    }
}

fn map_check_run_row(row: CheckRunSummaryRow) -> CheckRunSummary {
    CheckRunSummary {
        id: row.id,
        name: row.name,
        status: row.status,
        conclusion: row.conclusion,
        details_url: row.details_url,
        started_at: row.started_at,
        completed_at: row.completed_at,
        app_name: row.app_name,
    }
}

fn map_pr_file_row(row: PrFileRow) -> PrFile {
    PrFile {
        account_id: row.account_id,
        pr_id: row.pr_id,
        head_sha: row.head_sha,
        path: row.path,
        old_path: row.old_path,
        status: row.status,
        additions: row.additions,
        deletions: row.deletions,
        is_binary: row.is_binary == 1,
        patch_blob_sha: row.patch_blob_sha,
        viewed_by_account_id: row.viewed_by_account_id,
        viewed_at_head_sha: row.viewed_at_head_sha,
    }
}

fn map_file_tree_row(row: FileTreeSummaryRow) -> FileTreeSummary {
    FileTreeSummary {
        account_id: row.account_id,
        pr_id: row.pr_id,
        head_sha: row.head_sha,
        directory: row.directory,
        file_count: row.file_count,
        additions: row.additions,
        deletions: row.deletions,
    }
}

fn map_notification_row(row: NotificationListRow) -> NotificationItem {
    NotificationItem {
        id: row.id,
        reason: row.reason,
        title: row.title,
        unread: row.unread == 1,
        updated_at: row.updated_at,
        pr_id: row.pr_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
    }
}

fn map_repo_subscription_row(row: RepoSubscriptionRow) -> RepoSubscriptionItem {
    RepoSubscriptionItem {
        account_id: row.account_id,
        repo_id: row.repo_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        watch_tier: row.watch_tier,
        last_full_sync_at: row.last_full_sync_at,
        updated_at: row.updated_at,
    }
}

fn map_pr_label_row(row: PrLabelRow) -> PrLabel {
    PrLabel {
        label_name: row.label_name,
        label_color: row.label_color,
        description: row.description,
    }
}

fn map_pr_assignee_row(row: PrAssigneeRow) -> PrParticipant {
    PrParticipant {
        user_id: row.user_id,
        login: row.login,
    }
}

fn map_pr_reviewer_row(row: PrReviewerRow) -> PrReviewer {
    PrReviewer {
        user_id: row.user_id,
        login: row.login,
        reviewer_type: row.reviewer_type,
        reviewer_state: row.reviewer_state,
        requested_at: row.requested_at,
    }
}

fn map_pr_project_row(row: PrProjectRow) -> PrProject {
    PrProject {
        project_id: row.project_id,
        project_title: row.project_title,
        item_id: row.item_id,
        status: row.status,
        updated_at: row.updated_at,
    }
}

fn map_pr_milestone_row(row: PrMilestoneRow) -> PrMilestone {
    PrMilestone {
        milestone_id: row.milestone_id,
        title: row.title,
        state: row.state,
        due_on: row.due_on,
        description: row.description,
    }
}

fn map_rate_limit_bucket(row: RateLimitBucketRow) -> RateLimitBucket {
    RateLimitBucket {
        account_id: row.account_id,
        resource: row.resource,
        remaining: row.remaining,
        limit_total: row.limit_total,
        reset_at: row.reset_at,
        updated_at: row.updated_at,
    }
}

pub fn pr_changed_event_name(pr_id: &str) -> String {
    format!("pr:{pr_id} changed")
}

pub fn inbox_changed_event_name(account_id: &str) -> String {
    format!("inbox:account:{account_id} changed")
}

pub fn rate_limit_changed_event_name(account_id: &str) -> String {
    format!("rate_limit:account:{account_id} changed")
}

pub fn notifications_changed_event_name(account_id: &str) -> String {
    format!("notifications:account:{account_id} changed")
}

pub fn network_changed_event_name(account_id: &str) -> String {
    format!("network:{account_id} changed")
}

pub fn mutation_hard_conflict_event_name(mutation_id: &str) -> String {
    format!("mutation:{mutation_id} hard-conflict")
}

#[derive(Clone)]
pub struct TauriCacheInvalidationEmitter<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> TauriCacheInvalidationEmitter<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self { app }
    }

    pub fn emit_network_changed(&self, account_id: &str, state: NetState) {
        let payload = NetworkChangedEventPayload {
            account_id: account_id.to_string(),
            state,
        };
        let _ = self
            .app
            .emit(&network_changed_event_name(account_id), payload.clone());
        let _ = self.app.emit(
            <NetworkChangedEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }

    pub fn emit_mutation_hard_conflict(&self, mutation_id: &str, conflict: HardConflictPayload) {
        let payload = MutationHardConflictEventPayload {
            mutation_id: mutation_id.to_string(),
            conflict,
        };
        let _ = self.app.emit(
            &mutation_hard_conflict_event_name(mutation_id),
            payload.clone(),
        );
        let _ = self.app.emit(
            <MutationHardConflictEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }
}

impl<R: tauri::Runtime> CacheInvalidationEmitter for TauriCacheInvalidationEmitter<R> {
    fn emit_pr_changed(&self, pr_id: &str) {
        let payload = PrChangedEventPayload {
            pr_id: pr_id.to_string(),
        };
        let _ = self
            .app
            .emit(&pr_changed_event_name(pr_id), payload.clone());
        let _ = self.app.emit(
            <PrChangedEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }

    fn emit_inbox_changed(&self, account_id: &str) {
        let payload = InboxChangedEventPayload {
            account_id: account_id.to_string(),
        };
        let _ = self
            .app
            .emit(&inbox_changed_event_name(account_id), payload.clone());
        let _ = self.app.emit(
            <InboxChangedEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }

    fn emit_rate_limit_changed(&self, account_id: &str) {
        let payload = RateLimitChangedEventPayload {
            account_id: account_id.to_string(),
        };
        let _ = self
            .app
            .emit(&rate_limit_changed_event_name(account_id), payload.clone());
        let _ = self.app.emit(
            <RateLimitChangedEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }

    fn emit_notifications_changed(&self, account_id: &str) {
        let payload = NotificationsChangedEventPayload {
            account_id: account_id.to_string(),
        };
        let _ = self.app.emit(
            &notifications_changed_event_name(account_id),
            payload.clone(),
        );
        let _ = self.app.emit(
            <NotificationsChangedEventPayload as tauri_specta::Event>::NAME,
            payload,
        );
    }
}

pub async fn ipc_accounts_list_impl(auth: &AuthService) -> Result<AccountsListResponse, IpcError> {
    auth::auth_list_accounts_impl(auth)
        .await
        .map_err(Into::into)
}

pub async fn ipc_account_switch_impl(
    auth: &AuthService,
    input: AccountSwitchInput,
) -> Result<AuthAccount, IpcError> {
    auth::auth_switch_account_impl(
        auth,
        AccountLocator {
            host: input.host,
            login: input.login,
        },
    )
    .await
    .map_err(Into::into)
}

pub async fn ipc_inbox_list_impl(
    db: &Db,
    input: InboxListInput,
) -> Result<Vec<InboxItem>, IpcError> {
    let rows = db
        .list_inbox(&input.account_id)
        .await
        .map_err(IpcError::db)?;
    Ok(rows.into_iter().map(map_inbox_row).collect())
}

pub async fn ipc_pr_detail_summary_impl(
    db: &Db,
    input: PrHandleInput,
) -> Result<Option<PrDetailSummary>, IpcError> {
    let row = db
        .pr_detail_summary(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?;
    Ok(row.map(map_pr_detail_summary))
}

pub async fn ipc_pr_timeline_impl(db: &Db, input: PagedPrInput) -> Result<TimelinePage, IpcError> {
    let (limit, offset) = normalize_page(input.limit, input.offset);
    let mut rows = db
        .pr_timeline_page(&input.account_id, &input.pr_id, limit + 1, offset)
        .await
        .map_err(IpcError::db)?;
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.truncate(limit as usize);
    }
    Ok(TimelinePage {
        items: rows.into_iter().map(map_timeline_row).collect(),
        next_offset: has_more.then_some(offset + limit),
    })
}

pub async fn ipc_pr_review_threads_impl(
    db: &Db,
    input: PagedPrInput,
) -> Result<ReviewThreadsPage, IpcError> {
    let (limit, offset) = normalize_page(input.limit, input.offset);
    let mut rows = db
        .pr_review_threads(&input.account_id, &input.pr_id, limit + 1, offset)
        .await
        .map_err(IpcError::db)?;
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.truncate(limit as usize);
    }
    let threads = rows
        .into_iter()
        .map(map_review_thread_row)
        .collect::<Vec<_>>();
    Ok(ReviewThreadsPage {
        threads,
        next_offset: has_more.then_some(offset + limit),
    })
}

pub async fn ipc_pr_check_summary_impl(
    db: &Db,
    input: CheckSummaryInput,
) -> Result<PrCheckSummary, IpcError> {
    let runs = db
        .pr_check_runs(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_check_run_row)
        .collect::<Vec<_>>();

    let total_runs = i64::try_from(runs.len()).unwrap_or(i64::MAX);
    let successful_runs = i64::try_from(
        runs.iter()
            .filter(|run| run.conclusion.as_deref() == Some("SUCCESS"))
            .count(),
    )
    .unwrap_or(i64::MAX);
    let pending_runs = i64::try_from(
        runs.iter()
            .filter(|run| !run.status.eq_ignore_ascii_case("completed"))
            .count(),
    )
    .unwrap_or(i64::MAX);
    let failed_runs = total_runs
        .saturating_sub(successful_runs)
        .saturating_sub(pending_runs);

    Ok(PrCheckSummary {
        total_runs,
        successful_runs,
        failed_runs,
        pending_runs,
        runs,
    })
}

pub async fn ipc_pr_files_impl(db: &Db, input: PrFilesInput) -> Result<PrFilesResponse, IpcError> {
    let files = db
        .pr_files(&input.account_id, &input.pr_id, &input.head_sha)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_file_row)
        .collect::<Vec<_>>();
    let tree = db
        .file_tree_summary(&input.account_id, &input.pr_id, &input.head_sha)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_file_tree_row)
        .collect::<Vec<_>>();
    Ok(PrFilesResponse { files, tree })
}

pub async fn ipc_pr_patch_impl(db: &Db, input: PrPatchInput) -> Result<PrPatchResponse, IpcError> {
    let patch_blob_sha = db
        .pr_patch_blob_sha(&input.account_id, &input.pr_id, &input.head_sha)
        .await
        .map_err(IpcError::db)?;
    let patch = db
        .pr_patch(&input.account_id, &input.pr_id, &input.head_sha)
        .await
        .map_err(IpcError::db)?
        .map(String::from_utf8)
        .transpose()
        .map_err(|error| IpcError {
            code: "InvalidPatchEncoding".to_string(),
            message: error.to_string(),
        })?;
    Ok(PrPatchResponse {
        patch_blob_sha,
        patch,
    })
}

pub fn ipc_rendered_comment_html_impl(input: RenderedCommentInput) -> RenderedCommentHtml {
    let rendered = render::render_comment(
        &input.body,
        &RenderCtx {
            repo: input.repo.as_deref(),
            cache: None,
        },
    );
    RenderedCommentHtml {
        html: rendered.html,
        cache_hit: rendered.cache_hit,
        content_hash: rendered.content_hash,
        cache_key: rendered.cache_key,
        renderer_version: rendered.renderer_version.to_string(),
    }
}

pub async fn ipc_notifications_list_impl(
    db: &Db,
    input: NotificationsListInput,
) -> Result<Vec<NotificationItem>, IpcError> {
    let (limit, offset) = normalize_page(input.limit, input.offset);
    let rows = db
        .list_notifications(&input.account_id, limit, offset)
        .await
        .map_err(IpcError::db)?;
    Ok(rows.into_iter().map(map_notification_row).collect())
}

pub async fn ipc_system_status_impl(
    db: &Db,
    sync: &SyncTierStateStore,
    input: SystemStatusInput,
) -> Result<SystemStatusResponse, IpcError> {
    let rate_limits = db
        .rate_limit_buckets_for_account(&input.account_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_rate_limit_bucket)
        .collect::<Vec<_>>();
    let sync = sync.snapshot().await;
    Ok(SystemStatusResponse { rate_limits, sync })
}

pub async fn ipc_repo_subscriptions_impl(
    db: &Db,
    input: RepoSubscriptionsInput,
) -> Result<Vec<RepoSubscriptionItem>, IpcError> {
    let rows = db
        .list_repo_subscriptions(&input.account_id)
        .await
        .map_err(IpcError::db)?;
    Ok(rows.into_iter().map(map_repo_subscription_row).collect())
}

pub async fn ipc_pr_metadata_impl(
    db: &Db,
    input: PrHandleInput,
) -> Result<PrMetadataResponse, IpcError> {
    let labels = db
        .pr_labels(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_label_row)
        .collect::<Vec<_>>();
    let assignees = db
        .pr_assignees(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_assignee_row)
        .collect::<Vec<_>>();
    let requested_reviewers = db
        .pr_reviewers(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_reviewer_row)
        .collect::<Vec<_>>();
    let projects = db
        .pr_projects(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_project_row)
        .collect::<Vec<_>>();
    let milestones = db
        .pr_milestones(&input.account_id, &input.pr_id)
        .await
        .map_err(IpcError::db)?
        .into_iter()
        .map(map_pr_milestone_row)
        .collect::<Vec<_>>();
    Ok(PrMetadataResponse {
        labels,
        assignees,
        requested_reviewers,
        projects,
        milestones,
    })
}

pub async fn ipc_init_inbox_impl(
    db: &Db,
    auth: &AuthService,
    sync: &SyncTierStateStore,
) -> Result<InitInboxResponse, IpcError> {
    let accounts = ipc_accounts_list_impl(auth).await?;
    let active_locator = accounts.active.clone().or_else(|| {
        accounts.accounts.first().map(|account| AccountLocator {
            host: account.host.clone(),
            login: account.login.clone(),
        })
    });
    let active_account_id = active_locator.as_ref().map(account_id_from_locator);

    let (subscriptions, inbox, status) = if let Some(account_id) = active_account_id.as_ref() {
        let subscriptions = ipc_repo_subscriptions_impl(
            db,
            RepoSubscriptionsInput {
                account_id: account_id.clone(),
            },
        )
        .await?;
        let inbox = ipc_inbox_list_impl(
            db,
            InboxListInput {
                account_id: account_id.clone(),
            },
        )
        .await?;
        let status = Some(
            ipc_system_status_impl(
                db,
                sync,
                SystemStatusInput {
                    account_id: account_id.clone(),
                },
            )
            .await?,
        );
        (subscriptions, inbox, status)
    } else {
        (Vec::new(), Vec::new(), None)
    };

    Ok(InitInboxResponse {
        active_account_id,
        accounts,
        subscriptions,
        inbox,
        status,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_accounts_list(
    auth: tauri::State<'_, Arc<AuthService>>,
) -> Result<AccountsListResponse, IpcError> {
    ipc_accounts_list_impl(auth.inner()).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_account_switch(
    auth: tauri::State<'_, Arc<AuthService>>,
    input: AccountSwitchInput,
) -> Result<AuthAccount, IpcError> {
    ipc_account_switch_impl(auth.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_inbox_list(
    db: tauri::State<'_, Arc<Db>>,
    input: InboxListInput,
) -> Result<Vec<InboxItem>, IpcError> {
    ipc_inbox_list_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_detail_summary(
    db: tauri::State<'_, Arc<Db>>,
    input: PrHandleInput,
) -> Result<Option<PrDetailSummary>, IpcError> {
    ipc_pr_detail_summary_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_timeline(
    db: tauri::State<'_, Arc<Db>>,
    input: PagedPrInput,
) -> Result<TimelinePage, IpcError> {
    ipc_pr_timeline_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_review_threads(
    db: tauri::State<'_, Arc<Db>>,
    input: PagedPrInput,
) -> Result<ReviewThreadsPage, IpcError> {
    ipc_pr_review_threads_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_check_summary(
    db: tauri::State<'_, Arc<Db>>,
    input: CheckSummaryInput,
) -> Result<PrCheckSummary, IpcError> {
    ipc_pr_check_summary_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_files(
    db: tauri::State<'_, Arc<Db>>,
    input: PrFilesInput,
) -> Result<PrFilesResponse, IpcError> {
    ipc_pr_files_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_patch(
    db: tauri::State<'_, Arc<Db>>,
    input: PrPatchInput,
) -> Result<PrPatchResponse, IpcError> {
    ipc_pr_patch_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub fn ipc_rendered_comment_html(
    input: RenderedCommentInput,
) -> Result<RenderedCommentHtml, IpcError> {
    Ok(ipc_rendered_comment_html_impl(input))
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_notifications_list(
    db: tauri::State<'_, Arc<Db>>,
    input: NotificationsListInput,
) -> Result<Vec<NotificationItem>, IpcError> {
    ipc_notifications_list_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_system_status(
    db: tauri::State<'_, Arc<Db>>,
    sync: tauri::State<'_, Arc<SyncTierStateStore>>,
    input: SystemStatusInput,
) -> Result<SystemStatusResponse, IpcError> {
    ipc_system_status_impl(db.inner(), sync.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_repo_subscriptions(
    db: tauri::State<'_, Arc<Db>>,
    input: RepoSubscriptionsInput,
) -> Result<Vec<RepoSubscriptionItem>, IpcError> {
    ipc_repo_subscriptions_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_pr_metadata(
    db: tauri::State<'_, Arc<Db>>,
    input: PrHandleInput,
) -> Result<PrMetadataResponse, IpcError> {
    ipc_pr_metadata_impl(db.inner(), input).await
}

#[tauri::command]
#[specta::specta]
pub async fn ipc_init_inbox(
    db: tauri::State<'_, Arc<Db>>,
    auth: tauri::State<'_, Arc<AuthService>>,
    sync: tauri::State<'_, Arc<SyncTierStateStore>>,
) -> Result<InitInboxResponse, IpcError> {
    ipc_init_inbox_impl(db.inner(), auth.inner(), sync.inner()).await
}

pub fn specta_builder<R: tauri::Runtime>() -> tauri_specta::Builder<R> {
    tauri_specta::Builder::<R>::new()
        .dangerously_cast_bigints_to_number()
        .commands(tauri_specta::collect_commands![
            ipc_accounts_list,
            ipc_account_switch,
            ipc_inbox_list,
            ipc_pr_detail_summary,
            ipc_pr_timeline,
            ipc_pr_review_threads,
            ipc_pr_check_summary,
            ipc_pr_files,
            ipc_pr_patch,
            ipc_rendered_comment_html,
            ipc_notifications_list,
            ipc_system_status,
            ipc_repo_subscriptions,
            ipc_pr_metadata,
            ipc_init_inbox
        ])
        .events(tauri_specta::collect_events![
            PrChangedEventPayload,
            InboxChangedEventPayload,
            RateLimitChangedEventPayload,
            NotificationsChangedEventPayload,
            NetworkChangedEventPayload,
            MutationHardConflictEventPayload
        ])
}

pub fn bindings_output_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/lib/ipc/bindings.ts")
        .to_path_buf()
}

pub fn export_bindings(path: impl AsRef<Path>) -> AnyResult<()> {
    let builder = specta_builder::<tauri::Wry>();
    builder
        .export(specta_typescript::Typescript::default(), path.as_ref())
        .context("exporting tauri-specta bindings")?;
    Ok(())
}

pub fn export_default_bindings() -> AnyResult<()> {
    export_bindings(bindings_output_path())
}

pub fn command_names() -> &'static [&'static str] {
    &[
        "ipc_accounts_list",
        "ipc_account_switch",
        "ipc_inbox_list",
        "ipc_pr_detail_summary",
        "ipc_pr_timeline",
        "ipc_pr_review_threads",
        "ipc_pr_check_summary",
        "ipc_pr_files",
        "ipc_pr_patch",
        "ipc_rendered_comment_html",
        "ipc_notifications_list",
        "ipc_system_status",
        "ipc_repo_subscriptions",
        "ipc_pr_metadata",
        "ipc_init_inbox",
    ]
}

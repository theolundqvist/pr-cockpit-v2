use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingOverlay {
    pub mutation_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlobKind {
    Patch,
    MarkdownHtml,
    CheckLog,
    Asset,
}

impl BlobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::MarkdownHtml => "markdown_html",
            Self::CheckLog => "check_log",
            Self::Asset => "asset",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxRow {
    pub account_id: String,
    pub account_login: String,
    pub account_host: String,
    pub pr_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub pr_number: i64,
    pub title: String,
    pub state: String,
    pub draft: i64,
    pub head_sha: String,
    pub base_sha: String,
    pub mergeable_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub updated_at: i64,
    pub author_login: String,
    pub unread_notification_count: i64,
    pub latest_notification_at: Option<i64>,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrDetailSummaryRow {
    pub account_id: String,
    pub pr_id: String,
    pub repo_id: String,
    pub pr_number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub draft: i64,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
    pub mergeable_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub merge_commit_allowed: Option<i64>,
    pub squash_merge_allowed: Option<i64>,
    pub rebase_merge_allowed: Option<i64>,
    pub delete_branch_on_merge_default: Option<i64>,
    pub viewer_can_merge: Option<i64>,
    pub viewer_can_enable_auto_merge: Option<i64>,
    pub viewer_can_disable_auto_merge: Option<i64>,
    pub viewer_can_update_branch: Option<i64>,
    pub viewer_can_delete_head_ref: Option<i64>,
    pub auto_merge_enabled: Option<i64>,
    pub auto_merge_method: Option<String>,
    pub auto_merge_commit_headline: Option<String>,
    pub auto_merge_commit_body: Option<String>,
    pub auto_merge_enabled_by_login: Option<String>,
    pub auto_merge_enabled_at: Option<i64>,
    pub merge_queue_entry_id: Option<String>,
    pub merge_queue_entry_position: Option<i64>,
    pub merge_queue_entry_state: Option<String>,
    pub merge_queue_entry_estimated_ms: Option<i64>,
    pub branch_protection_summary_json: Option<String>,
    pub repo_has_merge_queue: Option<i64>,
    pub head_ref_state: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    pub changed_files: i64,
    pub comment_count: i64,
    pub review_count: i64,
    pub thread_count: i64,
    pub check_run_count: i64,
    pub file_count: i64,
    pub updated_at: i64,
    pub body_server_adjusted: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnreadCountsRow {
    pub account_id: String,
    pub total_notifications: i64,
    pub unread_notifications: i64,
    pub prs_with_unread: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileTreeSummaryRow {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub directory: String,
    pub file_count: i64,
    pub viewed_file_count: i64,
    pub additions: i64,
    pub deletions: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrFileRow {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub path: String,
    pub old_path: Option<String>,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
    pub is_binary: i64,
    pub kind: String,
    pub rename_similarity: Option<i64>,
    pub is_viewed: i64,
    pub patch_blob_sha: Option<String>,
    pub viewed_by_account_id: Option<String>,
    pub viewed_at_head_sha: Option<String>,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct TimelineRow {
    pub item_id: String,
    pub item_kind: String,
    pub body: String,
    pub author_login: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub review_state: Option<String>,
    pub body_server_adjusted: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct ReviewThreadRow {
    pub id: String,
    pub path: String,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub start_line: Option<i64>,
    pub start_side: Option<String>,
    pub is_outdated: i64,
    pub is_resolved: i64,
    pub resolved_by_login: Option<String>,
    pub updated_at: i64,
    pub comment_count: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct CheckRunSummaryRow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct NotificationListRow {
    pub id: String,
    pub reason: String,
    pub title: String,
    pub unread: i64,
    pub updated_at: i64,
    pub pr_id: Option<String>,
    pub repo_owner: String,
    pub repo_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct RepoSubscriptionRow {
    pub account_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub watch_tier: String,
    pub last_full_sync_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PrLabelRow {
    pub label_name: String,
    pub label_color: String,
    pub description: Option<String>,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PrAssigneeRow {
    pub user_id: String,
    pub login: Option<String>,
    pub assigned_at: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PrReviewerRow {
    pub user_id: String,
    pub login: Option<String>,
    pub reviewer_type: String,
    pub reviewer_state: String,
    pub requested_at: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PrProjectRow {
    pub project_id: String,
    pub project_title: String,
    pub item_id: Option<String>,
    pub status: Option<String>,
    pub updated_at: i64,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PrMilestoneRow {
    pub milestone_id: String,
    pub title: String,
    pub state: String,
    pub due_on: Option<i64>,
    pub description: Option<String>,
    pub pending_overlay: Option<PendingOverlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SearchHitRow {
    pub doc_type: String,
    pub doc_ref: String,
    pub pr_id: Option<String>,
    pub title: String,
    pub body: String,
    pub filename: String,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    pub id: String,
    pub host: String,
    pub login: String,
    pub token_kind: String,
    pub scopes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct AuthAccountRow {
    pub id: String,
    pub host: String,
    pub login: String,
    pub token_kind: String,
    pub scopes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub id: String,
    pub account_id: String,
    pub owner: String,
    pub name: String,
    pub default_branch: Option<String>,
    pub description: Option<String>,
    pub html_url: Option<String>,
    pub is_private: bool,
    pub is_archived: bool,
    pub pushed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSubscriptionRecord {
    pub repo_id: String,
    pub account_id: String,
    pub watch_tier: String,
    pub last_full_sync_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub account_id: String,
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgRecord {
    pub id: String,
    pub account_id: String,
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub html_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestRecord {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub number: i64,
    pub state: String,
    pub draft: bool,
    pub title: String,
    pub body: String,
    pub author_id: Option<String>,
    pub base_ref: String,
    pub base_sha: String,
    pub head_ref: String,
    pub head_sha: String,
    pub head_repo_id: Option<String>,
    pub mergeable_state: Option<String>,
    pub merge_state_status: Option<String>,
    pub merge_commit_allowed: Option<bool>,
    pub squash_merge_allowed: Option<bool>,
    pub rebase_merge_allowed: Option<bool>,
    pub delete_branch_on_merge_default: Option<bool>,
    pub viewer_can_merge: Option<bool>,
    pub viewer_can_enable_auto_merge: Option<bool>,
    pub viewer_can_disable_auto_merge: Option<bool>,
    pub viewer_can_update_branch: Option<bool>,
    pub viewer_can_delete_head_ref: Option<bool>,
    pub auto_merge_enabled: Option<bool>,
    pub auto_merge_method: Option<String>,
    pub auto_merge_commit_headline: Option<String>,
    pub auto_merge_commit_body: Option<String>,
    pub auto_merge_enabled_by_login: Option<String>,
    pub auto_merge_enabled_at: Option<i64>,
    pub merge_queue_entry_id: Option<String>,
    pub merge_queue_entry_position: Option<i64>,
    pub merge_queue_entry_state: Option<String>,
    pub merge_queue_entry_estimated_ms: Option<i64>,
    pub branch_protection_summary_json: Option<String>,
    pub repo_has_merge_queue: Option<bool>,
    pub head_ref_state: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    pub changed_files: i64,
    pub comments_count: i64,
    pub reviews_count: i64,
    pub commits_count: i64,
    pub is_read: bool,
    pub html_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub merged_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrLabelRecord {
    pub account_id: String,
    pub pr_id: String,
    pub label_name: String,
    pub label_color: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrAssigneeRecord {
    pub account_id: String,
    pub pr_id: String,
    pub user_id: String,
    pub assigned_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrReviewerRecord {
    pub account_id: String,
    pub pr_id: String,
    pub user_id: String,
    pub reviewer_type: String,
    pub reviewer_state: String,
    pub requested_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRecord {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub author_id: Option<String>,
    pub message_headline: String,
    pub message_body: String,
    pub committed_at: i64,
    pub parents_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCommitRecord {
    pub account_id: String,
    pub pr_id: String,
    pub commit_id: String,
    pub commit_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentRecord {
    pub id: String,
    pub account_id: String,
    pub pr_id: String,
    pub kind: String,
    pub author_id: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub in_reply_to_id: Option<String>,
    pub review_id: Option<String>,
    pub thread_id: Option<String>,
    pub path: Option<String>,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub start_line: Option<i64>,
    pub start_side: Option<String>,
    pub original_commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadRecord {
    pub id: String,
    pub account_id: String,
    pub pr_id: String,
    pub path: String,
    pub line: Option<i64>,
    pub side: Option<String>,
    pub start_line: Option<i64>,
    pub start_side: Option<String>,
    pub original_commit_sha: Option<String>,
    pub original_path: Option<String>,
    pub original_position: Option<i64>,
    pub original_line: Option<i64>,
    pub is_outdated: bool,
    pub is_resolved: bool,
    pub resolved_by_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: String,
    pub account_id: String,
    pub pr_id: String,
    pub author_id: String,
    pub state: String,
    pub body: String,
    pub commit_sha: Option<String>,
    pub submitted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckSuiteRecord {
    pub id: String,
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub app_name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRunRecord {
    pub id: String,
    pub account_id: String,
    pub check_suite_id: String,
    pub pr_id: String,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub details_url: Option<String>,
    pub output_title: Option<String>,
    pub output_summary: Option<String>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckAnnotationRecord {
    pub id: String,
    pub account_id: String,
    pub check_run_id: String,
    pub pr_id: String,
    pub path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub start_column: Option<i64>,
    pub end_column: Option<i64>,
    pub annotation_level: String,
    pub title: Option<String>,
    pub message: String,
    pub raw_details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrFileRecord {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub path: String,
    pub old_path: Option<String>,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
    pub is_binary: bool,
    pub kind: String,
    pub rename_similarity: Option<f64>,
    pub patch_blob_sha: Option<String>,
    pub viewed_by_account_id: Option<String>,
    pub viewed_at_head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrPatchRecord {
    pub account_id: String,
    pub pr_id: String,
    pub head_sha: String,
    pub patch_blob_sha: String,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub pr_id: Option<String>,
    pub reason: String,
    pub subject_type: String,
    pub subject_id: String,
    pub title: String,
    pub unread: bool,
    pub updated_at: i64,
    pub last_read_at: Option<i64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRecord {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub path: String,
    pub head_sha: String,
    pub branch: String,
    pub dirty: bool,
    pub ahead: i64,
    pub behind: i64,
    pub mapped_pr_id: Option<String>,
    pub mapping_confidence: Option<f64>,
    pub mapping_source: Option<String>,
    pub is_app_managed: bool,
    pub manual_override_pr_id: Option<String>,
    pub manual_override_at: Option<i64>,
    pub last_cleanup_snapshot_id: Option<String>,
    pub untracked_count: i64,
    pub staged_count: i64,
    pub modified_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq)]
pub struct WorktreeViewRow {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub path: String,
    pub head_sha: String,
    pub branch: String,
    pub dirty: i64,
    pub ahead: i64,
    pub behind: i64,
    pub untracked_count: i64,
    pub staged_count: i64,
    pub modified_count: i64,
    pub mapped_pr_id: Option<String>,
    pub mapped_pr_number: Option<i64>,
    pub mapping_confidence: Option<f64>,
    pub mapping_source: Option<String>,
    pub is_app_managed: i64,
    pub manual_override_pr_id: Option<String>,
    pub manual_override_at: Option<i64>,
    pub last_cleanup_snapshot_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct RepoOwnerNameRow {
    pub account_id: String,
    pub repo_id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PullRequestMappingCandidateRow {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub number: i64,
    pub head_ref: String,
    pub head_sha: String,
    pub head_repo_id: Option<String>,
    pub head_repo_owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct WorktreeSettingRow {
    pub id: String,
    pub key: String,
    pub value_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftRecord {
    pub id: String,
    pub account_id: String,
    pub target_type: String,
    pub target_id: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct DraftRow {
    pub id: String,
    pub account_id: String,
    pub target_type: String,
    pub target_id: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct PendingMutationRow {
    pub id: String,
    pub account_id: String,
    pub kind: String,
    pub target_type: String,
    pub target_id: String,
    pub status: String,
    pub retries: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_error: Option<String>,
    pub optimism_level: Option<String>,
    pub optimistic_patch_json: String,
    pub requires_connection_confirmation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMutationRecord {
    pub id: String,
    pub account_id: String,
    pub kind: String,
    pub target_type: String,
    pub target_id: String,
    pub idempotency_key: String,
    pub input_json: String,
    pub optimistic_patch_json: String,
    pub inverse_patch_json: String,
    pub status: String,
    pub retries: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_error: Option<String>,
    pub requires_connection_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdMappingRecord {
    pub account_id: String,
    pub kind: String,
    pub local_id: String,
    pub server_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCursorUpdate {
    pub account_id: String,
    pub resource: String,
    pub cursor: Option<String>,
    pub etag: Option<String>,
    pub expected_previous_etag: Option<String>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct SyncCursorRow {
    pub account_id: String,
    pub resource: String,
    pub cursor: Option<String>,
    pub etag: Option<String>,
    pub last_fetched_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitBucketUpdate {
    pub account_id: String,
    pub resource: String,
    pub remaining: i64,
    pub used: Option<i64>,
    pub limit_total: i64,
    pub reset_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct RateLimitBucketRow {
    pub account_id: String,
    pub resource: String,
    pub remaining: i64,
    pub used: Option<i64>,
    pub limit_total: i64,
    pub reset_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
pub struct BlobRefRow {
    pub sha256: String,
    pub kind: String,
    pub size: i64,
    pub ref_count: i64,
    pub last_accessed_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobEvictionResult {
    pub bytes_before: i64,
    pub bytes_after: i64,
    pub bytes_evicted: i64,
    pub blobs_evicted: usize,
}

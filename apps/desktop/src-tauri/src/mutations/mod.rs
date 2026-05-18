use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::api::GithubClient;
use crate::db::blob_store::BlobStore;
use crate::db::{
    CommentRecord, PrAssigneeRecord, PrLabelRecord, PrReviewerRecord, PullRequestRecord,
    ReviewRecord, ReviewThreadRecord,
};
use crate::sync::{CacheInvalidationEmitter, SyncHandle};

pub mod dispatch;
pub mod engine;
pub mod handlers;
pub mod ipc_types;
pub mod net;
pub mod patch;
pub mod projector;
pub mod reconciler;

pub use engine::MutationEngine;
pub use ipc_types::*;
pub use net::{NetProbe, NetState, NetworkMonitor, ReqwestNetProbe};
pub use patch::Patch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OptimismLevel {
    Full,
    Cautious,
    None,
}

impl OptimismLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Cautious => "cautious",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    AddComment,
    AddReviewComment,
    EditComment,
    DeleteComment,
    AddReaction,
    RemoveReaction,
    AddLabel,
    RemoveLabel,
    SetAssignees,
    RequestReview,
    RemoveReviewRequest,
    SubmitReview,
    ResolveThread,
    UnresolveThread,
    MarkFileViewed,
    UnmarkFileViewed,
    UpdatePrTitle,
    UpdatePrDescription,
    SetMilestone,
    SetProject,
    ConvertToDraft,
    MarkReadyForReview,
    EnableAutoMerge,
    DisableAutoMerge,
    UpdateBranch,
    Merge,
    DeleteHeadRef,
    EnqueueMergeQueue,
    DequeueMergeQueue,
    ReorderMergeQueue,
    ClosePr,
    ReopenPr,
    ApplySuggestion,
    ApplySuggestionBatch,
}

impl MutationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AddComment => "add_comment",
            Self::AddReviewComment => "add_review_comment",
            Self::EditComment => "edit_comment",
            Self::DeleteComment => "delete_comment",
            Self::AddReaction => "add_reaction",
            Self::RemoveReaction => "remove_reaction",
            Self::AddLabel => "add_label",
            Self::RemoveLabel => "remove_label",
            Self::SetAssignees => "set_assignees",
            Self::RequestReview => "request_review",
            Self::RemoveReviewRequest => "remove_review_request",
            Self::SubmitReview => "submit_review",
            Self::ResolveThread => "resolve_thread",
            Self::UnresolveThread => "unresolve_thread",
            Self::MarkFileViewed => "mark_file_viewed",
            Self::UnmarkFileViewed => "unmark_file_viewed",
            Self::UpdatePrTitle => "update_pr_title",
            Self::UpdatePrDescription => "update_pr_description",
            Self::SetMilestone => "set_milestone",
            Self::SetProject => "set_project",
            Self::ConvertToDraft => "convert_to_draft",
            Self::MarkReadyForReview => "mark_ready_for_review",
            Self::EnableAutoMerge => "enable_auto_merge",
            Self::DisableAutoMerge => "disable_auto_merge",
            Self::UpdateBranch => "update_branch",
            Self::Merge => "merge",
            Self::DeleteHeadRef => "delete_head_ref",
            Self::EnqueueMergeQueue => "enqueue_merge_queue",
            Self::DequeueMergeQueue => "dequeue_merge_queue",
            Self::ReorderMergeQueue => "reorder_merge_queue",
            Self::ClosePr => "close_pr",
            Self::ReopenPr => "reopen_pr",
            Self::ApplySuggestion => "apply_suggestion",
            Self::ApplySuggestionBatch => "apply_suggestion_batch",
        }
    }
}

impl FromStr for MutationKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let kind = match value {
            "add_comment" => Self::AddComment,
            "add_review_comment" => Self::AddReviewComment,
            "edit_comment" => Self::EditComment,
            "delete_comment" => Self::DeleteComment,
            "add_reaction" => Self::AddReaction,
            "remove_reaction" => Self::RemoveReaction,
            "add_label" => Self::AddLabel,
            "remove_label" => Self::RemoveLabel,
            "set_assignees" => Self::SetAssignees,
            "request_review" => Self::RequestReview,
            "remove_review_request" => Self::RemoveReviewRequest,
            "submit_review" => Self::SubmitReview,
            "resolve_thread" => Self::ResolveThread,
            "unresolve_thread" => Self::UnresolveThread,
            "mark_file_viewed" => Self::MarkFileViewed,
            "unmark_file_viewed" => Self::UnmarkFileViewed,
            "update_pr_title" => Self::UpdatePrTitle,
            "update_pr_description" => Self::UpdatePrDescription,
            "set_milestone" => Self::SetMilestone,
            "set_project" => Self::SetProject,
            "convert_to_draft" => Self::ConvertToDraft,
            "mark_ready_for_review" => Self::MarkReadyForReview,
            "enable_auto_merge" => Self::EnableAutoMerge,
            "disable_auto_merge" => Self::DisableAutoMerge,
            "update_branch" => Self::UpdateBranch,
            "merge" => Self::Merge,
            "delete_head_ref" => Self::DeleteHeadRef,
            "enqueue_merge_queue" => Self::EnqueueMergeQueue,
            "dequeue_merge_queue" => Self::DequeueMergeQueue,
            "reorder_merge_queue" => Self::ReorderMergeQueue,
            "close_pr" => Self::ClosePr,
            "reopen_pr" => Self::ReopenPr,
            "apply_suggestion" => Self::ApplySuggestion,
            "apply_suggestion_batch" => Self::ApplySuggestionBatch,
            other => return Err(anyhow!("unknown mutation kind `{other}`")),
        };
        Ok(kind)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ServerCallShape {
    None,
    Opaque(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct IdMappingDraft {
    pub kind: String,
    pub local_id: String,
    pub server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkdownOverlayTarget {
    Comment { comment_id: String },
    Review { review_id: String },
    PullRequest { pr_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct MarkdownOverlay {
    pub target: MarkdownOverlayTarget,
    pub predicted_body: String,
    pub server_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredictedEffect {
    pub forward_patch: Patch,
    pub inverse_patch: Patch,
    pub id_mappings: Vec<IdMappingDraft>,
    pub server_call: ServerCallShape,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerNode {
    PullRequest(Box<PullRequestRecord>),
    Comment(CommentRecord),
    Review(ReviewRecord),
    ReviewThread(ReviewThreadRecord),
    PrLabels {
        account_id: String,
        pr_id: String,
        labels: Vec<PrLabelRecord>,
    },
    PrAssignees {
        account_id: String,
        pr_id: String,
        assignees: Vec<PrAssigneeRecord>,
    },
    PrReviewers {
        account_id: String,
        pr_id: String,
        reviewers: Vec<PrReviewerRecord>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    #[serde(default)]
    pub upserts: Vec<ServerNode>,
    #[serde(default)]
    pub markdown_overlays: Vec<MarkdownOverlay>,
    #[serde(default)]
    pub id_mappings: Vec<IdMappingDraft>,
    #[serde(default)]
    pub refetch_pr_ids: Vec<String>,
    pub hard_conflict: Option<HardConflictDiff>,
}

impl ServerResponse {
    pub fn empty() -> Self {
        Self {
            upserts: Vec::new(),
            markdown_overlays: Vec::new(),
            id_mappings: Vec::new(),
            refetch_pr_ids: Vec::new(),
            hard_conflict: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ErrorKind {
    Network,
    RateLimited,
    Auth,
    NotFound,
    Conflict,
    Server,
    Other(String),
}

pub type BoxMutationFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct PredictCtx<'a> {
    pub db: &'a crate::db::Db,
    pub github: &'a GithubClient,
    pub blob_store: &'a BlobStore,
    pub account_id: &'a str,
    pub mutation_id: &'a str,
    pub idempotency_key: &'a str,
    pub input_json: &'a serde_json::Value,
}

pub struct ApplyCtx<'a> {
    pub db: &'a crate::db::Db,
    pub github: &'a GithubClient,
    pub blob_store: &'a BlobStore,
    pub account_id: &'a str,
    pub mutation_id: &'a str,
    pub idempotency_key: &'a str,
    pub input_json: &'a serde_json::Value,
    pub server_call: &'a ServerCallShape,
}

pub struct RollbackCtx<'a> {
    pub db: &'a crate::db::Db,
    pub github: &'a GithubClient,
    pub blob_store: &'a BlobStore,
    pub account_id: &'a str,
    pub mutation_id: &'a str,
    pub idempotency_key: &'a str,
    pub input_json: &'a serde_json::Value,
    pub inverse_patch: &'a Patch,
}

pub struct ReconcileCtx<'a> {
    pub db: &'a crate::db::Db,
    pub github: &'a GithubClient,
    pub blob_store: &'a BlobStore,
    pub account_id: &'a str,
    pub mutation_id: &'a str,
    pub idempotency_key: &'a str,
    pub input_json: &'a serde_json::Value,
    pub sync_handle: Option<&'a SyncHandle>,
    pub cache_invalidation_emitter: Option<&'a dyn CacheInvalidationEmitter>,
}

pub trait Mutation: Send + Sync + 'static {
    fn kind(&self) -> MutationKind;
    fn optimism(&self) -> OptimismLevel;
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect>;
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>>;

    fn rollback<'a>(&'a self, ctx: &'a RollbackCtx<'_>) -> BoxMutationFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut tx = ctx.db.pool().begin().await?;
            projector::apply_patch(
                &mut tx,
                ctx.inverse_patch,
                projector::PatchSource::Rollback {
                    mutation_id: ctx.mutation_id.to_string(),
                },
            )
            .await?;
            tx.commit().await?;
            Ok(())
        })
    }

    fn reconcile<'a>(
        &'a self,
        ctx: &'a ReconcileCtx<'_>,
        response: ServerResponse,
    ) -> BoxMutationFuture<'a, Result<()>> {
        Box::pin(async move { reconciler::reconcile(ctx, response).await })
    }
}

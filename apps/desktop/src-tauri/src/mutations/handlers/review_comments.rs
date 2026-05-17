use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::api::ADD_PULL_REQUEST_REVIEW_THREAD_REPLY_MUTATION;
use crate::db::{CommentRecord, ReviewThreadRecord};
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, IdMappingDraft, MarkdownOverlay, MarkdownOverlayTarget, Mutation,
    MutationKind, OptimismLevel, PredictCtx, PredictedEffect, ReconcileCtx, RollbackCtx,
    ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, now_epoch_seconds, optional_i64, optional_str, predicted_effect, required_i64,
    required_str, single_pk,
};

#[derive(Debug)]
pub struct AddReviewComment;

const ADD_REVIEW_COMMENT_MUTATION: &str =
    include_str!("../../api/queries/mutations/addReviewComment.graphql");
const ADD_REVIEW_THREAD_MUTATION: &str =
    include_str!("../../api/queries/mutations/addReviewThread.graphql");

impl Mutation for AddReviewComment {
    fn kind(&self) -> MutationKind {
        MutationKind::AddReviewComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let author_id = optional_str(ctx.input_json, "author_id").unwrap_or(ctx.account_id);
        let body = required_str(ctx.input_json, "body")?;
        let path = required_str(ctx.input_json, "path")?;
        let now = now_epoch_seconds()?;

        let local_comment_id = ctx
            .input_json
            .get("local_id")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "local-review-comment-{}-{}",
                    now,
                    &ctx.mutation_id[0..ctx.mutation_id.len().min(8)]
                )
            });

        let has_reply_target = optional_str(ctx.input_json, "thread_id").is_some()
            || optional_str(ctx.input_json, "in_reply_to_id").is_some();
        let kind = if has_reply_target {
            "review_thread_reply"
        } else {
            "review"
        };

        let mut pending_state = serde_json::json!({
            "mutation_id": ctx.mutation_id,
            "kind": "add-review-comment",
            "path": path,
        });
        if let Some(line) = optional_i64(ctx.input_json, "line") {
            pending_state["line"] = serde_json::json!(line);
        }
        if let Some(start_line) = optional_i64(ctx.input_json, "start_line") {
            pending_state["start_line"] = serde_json::json!(start_line);
        }
        if let Some(side) = optional_str(ctx.input_json, "side") {
            pending_state["side"] = serde_json::json!(side);
        }
        if let Some(start_side) = optional_str(ctx.input_json, "start_side") {
            pending_state["start_side"] = serde_json::json!(start_side);
        }

        let mut comment_row = BTreeMap::from([
            ("id".to_string(), PatchValue::from(local_comment_id.clone())),
            ("account_id".to_string(), PatchValue::from(ctx.account_id)),
            ("pr_id".to_string(), PatchValue::from(pr_id)),
            ("kind".to_string(), PatchValue::from(kind)),
            ("author_id".to_string(), PatchValue::from(author_id)),
            ("body".to_string(), PatchValue::from(body)),
            ("created_at".to_string(), PatchValue::from(now)),
            ("updated_at".to_string(), PatchValue::from(now)),
            ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
            (
                "pending_state".to_string(),
                PatchValue::Json(pending_state.clone()),
            ),
            ("path".to_string(), PatchValue::from(path)),
        ]);
        if let Some(line) = optional_i64(ctx.input_json, "line") {
            comment_row.insert("line".to_string(), PatchValue::from(line));
        }
        if let Some(side) = optional_str(ctx.input_json, "side") {
            comment_row.insert("side".to_string(), PatchValue::from(side.to_string()));
        }
        if let Some(start_line) = optional_i64(ctx.input_json, "start_line") {
            comment_row.insert("start_line".to_string(), PatchValue::from(start_line));
        }
        if let Some(start_side) = optional_str(ctx.input_json, "start_side") {
            comment_row.insert(
                "start_side".to_string(),
                PatchValue::from(start_side.to_string()),
            );
        }
        if let Some(in_reply_to_id) = optional_str(ctx.input_json, "in_reply_to_id") {
            comment_row.insert(
                "in_reply_to_id".to_string(),
                PatchValue::from(in_reply_to_id.to_string()),
            );
        }
        if let Some(review_id) = optional_str(ctx.input_json, "review_id") {
            comment_row.insert(
                "review_id".to_string(),
                PatchValue::from(review_id.to_string()),
            );
        }
        if let Some(original_commit_sha) = optional_str(ctx.input_json, "original_commit_sha") {
            comment_row.insert(
                "original_commit_sha".to_string(),
                PatchValue::from(original_commit_sha.to_string()),
            );
        }

        let mut operations = vec![RowMutation {
            table: "comments".to_string(),
            pk: single_pk("id", PatchValue::from(local_comment_id.clone())),
            before: None,
            after: Some(comment_row),
        }];

        let local_thread_id = if !has_reply_target {
            Some(
                ctx.input_json
                    .get("local_thread_id")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "local-review-thread-{}-{}",
                            now,
                            &ctx.mutation_id[0..ctx.mutation_id.len().min(8)]
                        )
                    }),
            )
        } else {
            optional_str(ctx.input_json, "thread_id").map(ToString::to_string)
        };

        if let Some(thread_id) = &local_thread_id {
            if !has_reply_target {
                let mut thread_row = BTreeMap::from([
                    ("id".to_string(), PatchValue::from(thread_id.clone())),
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    ("path".to_string(), PatchValue::from(path)),
                    (
                        "is_outdated".to_string(),
                        PatchValue::from(optional_i64(ctx.input_json, "is_outdated").unwrap_or(0)),
                    ),
                    ("is_resolved".to_string(), PatchValue::from(0_i64)),
                    ("created_at".to_string(), PatchValue::from(now)),
                    ("updated_at".to_string(), PatchValue::from(now)),
                    (
                        "pending_state".to_string(),
                        PatchValue::Json(pending_state.clone()),
                    ),
                ]);
                if let Some(line) = optional_i64(ctx.input_json, "line") {
                    thread_row.insert("line".to_string(), PatchValue::from(line));
                }
                if let Some(side) = optional_str(ctx.input_json, "side") {
                    thread_row.insert("side".to_string(), PatchValue::from(side.to_string()));
                }
                if let Some(start_line) = optional_i64(ctx.input_json, "start_line") {
                    thread_row.insert("start_line".to_string(), PatchValue::from(start_line));
                }
                if let Some(start_side) = optional_str(ctx.input_json, "start_side") {
                    thread_row.insert(
                        "start_side".to_string(),
                        PatchValue::from(start_side.to_string()),
                    );
                }
                if let Some(original_commit_sha) =
                    optional_str(ctx.input_json, "original_commit_sha")
                {
                    thread_row.insert(
                        "original_commit_sha".to_string(),
                        PatchValue::from(original_commit_sha.to_string()),
                    );
                }
                operations.push(RowMutation {
                    table: "review_threads".to_string(),
                    pk: single_pk("id", PatchValue::from(thread_id.clone())),
                    before: None,
                    after: Some(thread_row),
                });
            }
            let comment_after = operations[0]
                .after
                .as_mut()
                .context("comment row must be present")?;
            comment_after.insert("thread_id".to_string(), PatchValue::from(thread_id.clone()));
        }
        if !has_reply_target && operations.len() > 1 {
            operations.swap(0, 1);
        }

        Ok(predicted_effect(
            Patch {
                operations,
                pending_overlay_kind: None,
            },
            ServerCallShape::Opaque(serde_json::json!({
                "local_comment_id": local_comment_id,
                "local_thread_id": local_thread_id,
            })),
            Vec::new(),
        ))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let author_id = optional_str(ctx.input_json, "author_id").unwrap_or(ctx.account_id);
            let body = required_str(ctx.input_json, "body")?;
            let path = required_str(ctx.input_json, "path")?;
            let line = required_i64(ctx.input_json, "line")?;
            let side = optional_str(ctx.input_json, "side").unwrap_or("RIGHT");
            let start_line = optional_i64(ctx.input_json, "start_line");
            let start_side = optional_str(ctx.input_json, "start_side");
            let subject_type = optional_str(ctx.input_json, "subject_type").unwrap_or("LINE");
            let local_call = match ctx.server_call {
                ServerCallShape::Opaque(value) => value.clone(),
                _ => serde_json::json!({}),
            };
            let local_comment_id = local_call
                .get("local_comment_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let local_thread_id = local_call
                .get("local_thread_id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string);

            if let Some(review_id) = optional_str(ctx.input_json, "review_id") {
                #[derive(serde::Deserialize)]
                struct CommentData {
                    #[serde(rename = "addPullRequestReviewComment")]
                    add_review_comment: CommentPayload,
                }
                #[derive(serde::Deserialize)]
                struct CommentPayload {
                    comment: ReviewCommentNode,
                }
                #[derive(serde::Deserialize)]
                struct ReviewCommentNode {
                    id: String,
                    body: String,
                    #[serde(rename = "createdAt")]
                    created_at: String,
                    #[serde(rename = "updatedAt")]
                    updated_at: String,
                    path: Option<String>,
                    side: Option<String>,
                    line: Option<i64>,
                    #[serde(rename = "startSide")]
                    start_side: Option<String>,
                    #[serde(rename = "startLine")]
                    start_line: Option<i64>,
                    #[serde(rename = "pullRequestReviewThread")]
                    thread: Option<ThreadNode>,
                }
                #[derive(serde::Deserialize)]
                struct ThreadNode {
                    id: String,
                    path: Option<String>,
                    side: Option<String>,
                    line: Option<i64>,
                    #[serde(rename = "startSide")]
                    start_side: Option<String>,
                    #[serde(rename = "startLine")]
                    start_line: Option<i64>,
                    #[serde(rename = "isOutdated")]
                    is_outdated: bool,
                    #[serde(rename = "isResolved")]
                    is_resolved: bool,
                    #[serde(rename = "updatedAt")]
                    updated_at: Option<String>,
                }

                let (payload, _rate_limit) = ctx
                    .github
                    .graphql_mutation::<CommentData>(
                        ctx.account_id,
                        ADD_REVIEW_COMMENT_MUTATION,
                        serde_json::json!({
                            "pullRequestReviewId": review_id,
                            "body": body,
                            "path": path,
                            "side": side,
                            "line": line,
                            "startSide": start_side,
                            "startLine": start_line,
                            "subjectType": subject_type,
                        }),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
                let node = payload.add_review_comment.comment;
                let thread_node = node.thread;
                let thread_id_for_comment = thread_node.as_ref().map(|thread| thread.id.clone());
                let mut upserts = Vec::new();
                if let Some(thread) = thread_node.as_ref() {
                    let now = thread
                        .updated_at
                        .as_deref()
                        .map(crate::sync::reconcile::parse_timestamp)
                        .unwrap_or_else(|| now_epoch_seconds().unwrap_or_default());
                    upserts.push(ServerNode::ReviewThread(ReviewThreadRecord {
                        id: thread.id.clone(),
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        path: thread.path.clone().unwrap_or_else(|| path.to_string()),
                        line: thread.line,
                        side: thread.side.clone(),
                        start_line: thread.start_line,
                        start_side: thread.start_side.clone(),
                        original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                            .map(ToString::to_string),
                        original_path: optional_str(ctx.input_json, "original_path")
                            .map(ToString::to_string),
                        original_position: optional_i64(ctx.input_json, "original_position"),
                        original_line: optional_i64(ctx.input_json, "original_line"),
                        is_outdated: thread.is_outdated,
                        is_resolved: thread.is_resolved,
                        resolved_by_id: None,
                        created_at: now,
                        updated_at: now,
                    }));
                }
                upserts.push(ServerNode::Comment(CommentRecord {
                    id: node.id.clone(),
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    kind: if optional_str(ctx.input_json, "in_reply_to_id").is_some() {
                        "review_thread_reply".to_string()
                    } else {
                        "review".to_string()
                    },
                    author_id: author_id.to_string(),
                    body: node.body.clone(),
                    created_at: crate::sync::reconcile::parse_timestamp(&node.created_at),
                    updated_at: crate::sync::reconcile::parse_timestamp(&node.updated_at),
                    deleted_at: None,
                    in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                        .map(ToString::to_string),
                    review_id: optional_str(ctx.input_json, "review_id").map(ToString::to_string),
                    thread_id: thread_id_for_comment.clone(),
                    path: node.path.or_else(|| Some(path.to_string())),
                    line: node.line.or(Some(line)),
                    side: node.side.or(Some(side.to_string())),
                    start_line: node.start_line.or(start_line),
                    start_side: node
                        .start_side
                        .or_else(|| start_side.map(ToString::to_string)),
                    original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                        .map(ToString::to_string),
                }));

                let mut id_mappings = vec![IdMappingDraft {
                    kind: "comment".to_string(),
                    local_id: local_comment_id,
                    server_id: node.id.clone(),
                }];
                if let (Some(local_thread_id), Some(thread_id)) =
                    (local_thread_id, thread_id_for_comment)
                {
                    id_mappings.push(IdMappingDraft {
                        kind: "thread".to_string(),
                        local_id: local_thread_id,
                        server_id: thread_id,
                    });
                }

                return Ok(ServerResponse {
                    upserts,
                    markdown_overlays: vec![MarkdownOverlay {
                        target: MarkdownOverlayTarget::Comment {
                            comment_id: node.id.clone(),
                        },
                        predicted_body: body.to_string(),
                        server_body: node.body,
                    }],
                    id_mappings,
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                });
            }

            if optional_str(ctx.input_json, "thread_id").is_some()
                && optional_str(ctx.input_json, "in_reply_to_id").is_some()
            {
                #[derive(serde::Deserialize)]
                struct ReplyData {
                    #[serde(rename = "addPullRequestReviewThreadReply")]
                    add_reply: ReplyPayload,
                }
                #[derive(serde::Deserialize)]
                struct ReplyPayload {
                    comment: ReplyCommentNode,
                }
                #[derive(serde::Deserialize)]
                struct ReplyCommentNode {
                    id: String,
                    body: String,
                    #[serde(rename = "createdAt")]
                    created_at: String,
                    #[serde(rename = "updatedAt")]
                    updated_at: String,
                }

                let thread_id = required_str(ctx.input_json, "thread_id")?;
                let (payload, _rate_limit) = ctx
                    .github
                    .graphql_mutation::<ReplyData>(
                        ctx.account_id,
                        ADD_PULL_REQUEST_REVIEW_THREAD_REPLY_MUTATION,
                        serde_json::json!({
                            "threadId": thread_id,
                            "body": body,
                        }),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
                let node = payload.add_reply.comment;
                return Ok(ServerResponse {
                    upserts: vec![ServerNode::Comment(CommentRecord {
                        id: node.id.clone(),
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        kind: "review_thread_reply".to_string(),
                        author_id: author_id.to_string(),
                        body: node.body.clone(),
                        created_at: crate::sync::reconcile::parse_timestamp(&node.created_at),
                        updated_at: crate::sync::reconcile::parse_timestamp(&node.updated_at),
                        deleted_at: None,
                        in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                            .map(ToString::to_string),
                        review_id: optional_str(ctx.input_json, "review_id")
                            .map(ToString::to_string),
                        thread_id: optional_str(ctx.input_json, "thread_id")
                            .map(ToString::to_string),
                        path: Some(path.to_string()),
                        line: Some(line),
                        side: Some(side.to_string()),
                        start_line,
                        start_side: start_side.map(ToString::to_string),
                        original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                            .map(ToString::to_string),
                    })],
                    markdown_overlays: vec![MarkdownOverlay {
                        target: MarkdownOverlayTarget::Comment {
                            comment_id: node.id.clone(),
                        },
                        predicted_body: body.to_string(),
                        server_body: node.body,
                    }],
                    id_mappings: vec![IdMappingDraft {
                        kind: "comment".to_string(),
                        local_id: local_comment_id,
                        server_id: node.id,
                    }],
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                });
            }

            #[derive(serde::Deserialize)]
            struct ThreadData {
                #[serde(rename = "addPullRequestReviewThread")]
                add_review_thread: ThreadPayload,
            }
            #[derive(serde::Deserialize)]
            struct ThreadPayload {
                thread: ThreadNode,
            }
            #[derive(serde::Deserialize)]
            struct ThreadNode {
                id: String,
                path: String,
                side: Option<String>,
                line: Option<i64>,
                #[serde(rename = "startSide")]
                start_side: Option<String>,
                #[serde(rename = "startLine")]
                start_line: Option<i64>,
                #[serde(rename = "isOutdated")]
                is_outdated: bool,
                #[serde(rename = "isResolved")]
                is_resolved: bool,
                #[serde(rename = "updatedAt")]
                updated_at: Option<String>,
                comments: ThreadComments,
            }
            #[derive(serde::Deserialize)]
            struct ThreadComments {
                nodes: Option<Vec<Option<ThreadComment>>>,
            }
            #[derive(serde::Deserialize)]
            struct ThreadComment {
                id: String,
                body: String,
                #[serde(rename = "createdAt")]
                created_at: String,
                #[serde(rename = "updatedAt")]
                updated_at: String,
                path: Option<String>,
                side: Option<String>,
                line: Option<i64>,
                #[serde(rename = "startSide")]
                start_side: Option<String>,
                #[serde(rename = "startLine")]
                start_line: Option<i64>,
            }

            let pull_request_id = optional_str(ctx.input_json, "pull_request_id").unwrap_or(pr_id);
            let (payload, _rate_limit) = ctx
                .github
                .graphql_mutation::<ThreadData>(
                    ctx.account_id,
                    ADD_REVIEW_THREAD_MUTATION,
                    serde_json::json!({
                        "pullRequestId": pull_request_id,
                        "body": body,
                        "path": path,
                        "side": side,
                        "line": line,
                        "startSide": start_side,
                        "startLine": start_line,
                        "subjectType": subject_type,
                    }),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let thread = payload.add_review_thread.thread;
            let thread_updated_at = thread
                .updated_at
                .as_deref()
                .map(crate::sync::reconcile::parse_timestamp)
                .unwrap_or_else(|| now_epoch_seconds().unwrap_or_default());
            let comment = thread
                .comments
                .nodes
                .unwrap_or_default()
                .into_iter()
                .flatten()
                .next()
                .context("add review thread response missing thread comment")?;
            let comment_id = comment.id.clone();
            Ok(ServerResponse {
                upserts: vec![
                    ServerNode::ReviewThread(ReviewThreadRecord {
                        id: thread.id.clone(),
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        path: thread.path.clone(),
                        line: thread.line,
                        side: thread.side.clone(),
                        start_line: thread.start_line,
                        start_side: thread.start_side.clone(),
                        original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                            .map(ToString::to_string),
                        original_path: optional_str(ctx.input_json, "original_path")
                            .map(ToString::to_string),
                        original_position: optional_i64(ctx.input_json, "original_position"),
                        original_line: optional_i64(ctx.input_json, "original_line"),
                        is_outdated: thread.is_outdated,
                        is_resolved: thread.is_resolved,
                        resolved_by_id: None,
                        created_at: thread_updated_at,
                        updated_at: thread_updated_at,
                    }),
                    ServerNode::Comment(CommentRecord {
                        id: comment.id,
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        kind: "review".to_string(),
                        author_id: author_id.to_string(),
                        body: comment.body.clone(),
                        created_at: crate::sync::reconcile::parse_timestamp(&comment.created_at),
                        updated_at: crate::sync::reconcile::parse_timestamp(&comment.updated_at),
                        deleted_at: None,
                        in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                            .map(ToString::to_string),
                        review_id: optional_str(ctx.input_json, "review_id")
                            .map(ToString::to_string),
                        thread_id: Some(thread.id.clone()),
                        path: comment.path.or(Some(path.to_string())),
                        line: comment.line.or(Some(line)),
                        side: comment.side.or(Some(side.to_string())),
                        start_line: comment.start_line.or(start_line),
                        start_side: comment
                            .start_side
                            .or_else(|| start_side.map(ToString::to_string)),
                        original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                            .map(ToString::to_string),
                    }),
                ],
                markdown_overlays: vec![MarkdownOverlay {
                    target: MarkdownOverlayTarget::Comment {
                        comment_id: comment_id.clone(),
                    },
                    predicted_body: body.to_string(),
                    server_body: comment.body,
                }],
                id_mappings: vec![
                    IdMappingDraft {
                        kind: "comment".to_string(),
                        local_id: local_comment_id,
                        server_id: comment_id,
                    },
                    IdMappingDraft {
                        kind: "thread".to_string(),
                        local_id: local_thread_id
                            .unwrap_or_else(|| "local-thread-missing".to_string()),
                        server_id: thread.id,
                    },
                ],
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }

    fn rollback<'a>(&'a self, ctx: &'a RollbackCtx<'_>) -> BoxMutationFuture<'a, Result<()>> {
        Box::pin(async move {
            let mut tx = ctx.db.pool().begin().await?;
            crate::mutations::projector::apply_patch(
                &mut tx,
                ctx.inverse_patch,
                crate::mutations::projector::PatchSource::Rollback {
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
        Box::pin(async move { crate::mutations::reconciler::reconcile(ctx, response).await })
    }
}

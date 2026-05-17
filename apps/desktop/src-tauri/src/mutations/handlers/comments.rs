use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::api::ADD_PULL_REQUEST_REVIEW_THREAD_REPLY_MUTATION;
use crate::db::CommentRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, IdMappingDraft, MarkdownOverlay, MarkdownOverlayTarget, Mutation,
    MutationKind, OptimismLevel, PredictCtx, PredictedEffect, ReconcileCtx, ServerCallShape,
    ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, now_epoch_seconds, optional_i64, optional_str,
    predicted_effect, required_i64, required_str, single_pk,
};

#[derive(Debug)]
pub struct AddComment;

#[derive(Debug)]
pub struct EditComment;

#[derive(Debug)]
pub struct DeleteComment;

impl Mutation for AddComment {
    fn kind(&self) -> MutationKind {
        MutationKind::AddComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let author_id = required_str(ctx.input_json, "author_id")?;
        let body = required_str(ctx.input_json, "body")?;
        let kind = optional_str(ctx.input_json, "kind").unwrap_or("issue");
        let local_id = ctx
            .input_json
            .get("local_id")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "local-comment-{}-{}",
                    now_epoch_seconds().unwrap_or_default(),
                    &ctx.mutation_id[0..ctx.mutation_id.len().min(8)]
                )
            });
        let now = now_epoch_seconds()?;

        let mut row = BTreeMap::new();
        row.insert("id".to_string(), PatchValue::from(local_id.clone()));
        row.insert("account_id".to_string(), PatchValue::from(ctx.account_id));
        row.insert("pr_id".to_string(), PatchValue::from(pr_id));
        row.insert("kind".to_string(), PatchValue::from(kind));
        row.insert("author_id".to_string(), PatchValue::from(author_id));
        row.insert("body".to_string(), PatchValue::from(body));
        row.insert("created_at".to_string(), PatchValue::from(now));
        row.insert("updated_at".to_string(), PatchValue::from(now));
        row.insert("body_server_adjusted".to_string(), PatchValue::from(0_i64));
        row.insert(
            "pending_state".to_string(),
            PatchValue::Json(mutation_state_json(ctx.mutation_id, "add-comment")),
        );
        if let Some(in_reply_to_id) = optional_str(ctx.input_json, "in_reply_to_id") {
            row.insert(
                "in_reply_to_id".to_string(),
                PatchValue::from(in_reply_to_id.to_string()),
            );
        }
        if let Some(review_id) = optional_str(ctx.input_json, "review_id") {
            row.insert(
                "review_id".to_string(),
                PatchValue::from(review_id.to_string()),
            );
        }
        if let Some(thread_id) = optional_str(ctx.input_json, "thread_id") {
            row.insert(
                "thread_id".to_string(),
                PatchValue::from(thread_id.to_string()),
            );
        }
        if let Some(path) = optional_str(ctx.input_json, "path") {
            row.insert("path".to_string(), PatchValue::from(path.to_string()));
        }
        if let Some(line) = optional_i64(ctx.input_json, "line") {
            row.insert("line".to_string(), PatchValue::from(line));
        }
        if let Some(side) = optional_str(ctx.input_json, "side") {
            row.insert("side".to_string(), PatchValue::from(side.to_string()));
        }
        if let Some(start_line) = optional_i64(ctx.input_json, "start_line") {
            row.insert("start_line".to_string(), PatchValue::from(start_line));
        }
        if let Some(start_side) = optional_str(ctx.input_json, "start_side") {
            row.insert(
                "start_side".to_string(),
                PatchValue::from(start_side.to_string()),
            );
        }
        if let Some(original_commit_sha) = optional_str(ctx.input_json, "original_commit_sha") {
            row.insert(
                "original_commit_sha".to_string(),
                PatchValue::from(original_commit_sha.to_string()),
            );
        }

        let patch = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: single_pk("id", PatchValue::from(local_id.clone())),
                before: None,
                after: Some(row),
            }],
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(
            patch,
            ServerCallShape::Opaque(serde_json::json!({
                "local_id": local_id,
            })),
            Vec::new(),
        ))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let author_id = required_str(ctx.input_json, "author_id")?;
            let body = required_str(ctx.input_json, "body")?;
            let local_id = ctx.server_call.clone();
            let local_id = match local_id {
                ServerCallShape::Opaque(value) => value
                    .get("local_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                _ => ctx
                    .input_json
                    .get("local_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            };

            let response = if optional_str(ctx.input_json, "thread_id").is_some()
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
                let created_at = crate::sync::reconcile::parse_timestamp(&node.created_at);
                let updated_at = crate::sync::reconcile::parse_timestamp(&node.updated_at);
                ServerResponse {
                    upserts: vec![ServerNode::Comment(CommentRecord {
                        id: node.id.clone(),
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        kind: optional_str(ctx.input_json, "kind")
                            .unwrap_or("review_thread_reply")
                            .to_string(),
                        author_id: author_id.to_string(),
                        body: node.body.clone(),
                        created_at,
                        updated_at,
                        deleted_at: None,
                        in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                            .map(ToString::to_string),
                        review_id: optional_str(ctx.input_json, "review_id")
                            .map(ToString::to_string),
                        thread_id: optional_str(ctx.input_json, "thread_id")
                            .map(ToString::to_string),
                        path: optional_str(ctx.input_json, "path").map(ToString::to_string),
                        line: optional_i64(ctx.input_json, "line"),
                        side: optional_str(ctx.input_json, "side").map(ToString::to_string),
                        start_line: optional_i64(ctx.input_json, "start_line"),
                        start_side: optional_str(ctx.input_json, "start_side")
                            .map(ToString::to_string),
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
                        local_id: local_id.clone(),
                        server_id: node.id,
                    }],
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                }
            } else {
                #[derive(serde::Deserialize)]
                struct RestComment {
                    id: i64,
                    #[serde(rename = "node_id")]
                    node_id: Option<String>,
                    body: String,
                    #[serde(rename = "created_at")]
                    created_at: String,
                    #[serde(rename = "updated_at")]
                    updated_at: String,
                }
                let path = format!("/repos/{owner}/{repo}/issues/{number}/comments");
                let (payload, _rate_limit) = ctx
                    .github
                    .rest_mutation_json::<RestComment>(
                        ctx.account_id,
                        reqwest::Method::POST,
                        &path,
                        Some(serde_json::json!({ "body": body })),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
                let payload = payload.context("github add comment response missing body")?;
                let server_id = payload
                    .node_id
                    .unwrap_or_else(|| format!("comment-{}", payload.id));
                ServerResponse {
                    upserts: vec![ServerNode::Comment(CommentRecord {
                        id: server_id.clone(),
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        kind: optional_str(ctx.input_json, "kind")
                            .unwrap_or("issue")
                            .to_string(),
                        author_id: author_id.to_string(),
                        body: payload.body.clone(),
                        created_at: crate::sync::reconcile::parse_timestamp(&payload.created_at),
                        updated_at: crate::sync::reconcile::parse_timestamp(&payload.updated_at),
                        deleted_at: None,
                        in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                            .map(ToString::to_string),
                        review_id: optional_str(ctx.input_json, "review_id")
                            .map(ToString::to_string),
                        thread_id: optional_str(ctx.input_json, "thread_id")
                            .map(ToString::to_string),
                        path: optional_str(ctx.input_json, "path").map(ToString::to_string),
                        line: optional_i64(ctx.input_json, "line"),
                        side: optional_str(ctx.input_json, "side").map(ToString::to_string),
                        start_line: optional_i64(ctx.input_json, "start_line"),
                        start_side: optional_str(ctx.input_json, "start_side")
                            .map(ToString::to_string),
                        original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                            .map(ToString::to_string),
                    })],
                    markdown_overlays: vec![MarkdownOverlay {
                        target: MarkdownOverlayTarget::Comment {
                            comment_id: server_id.clone(),
                        },
                        predicted_body: body.to_string(),
                        server_body: payload.body,
                    }],
                    id_mappings: vec![IdMappingDraft {
                        kind: "comment".to_string(),
                        local_id,
                        server_id,
                    }],
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                }
            };

            Ok(response)
        })
    }
}

impl Mutation for EditComment {
    fn kind(&self) -> MutationKind {
        MutationKind::EditComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let comment_id = required_str(ctx.input_json, "comment_id")?;
        let pr_id = required_str(ctx.input_json, "pr_id")?.to_string();
        let kind = optional_str(ctx.input_json, "kind")
            .unwrap_or("issue")
            .to_string();
        let author_id = required_str(ctx.input_json, "author_id")?.to_string();
        let previous_body = required_str(ctx.input_json, "previous_body")?.to_string();
        let created_at = optional_i64(ctx.input_json, "created_at").unwrap_or(0);
        let previous_updated_at = optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(0);
        let next_body = required_str(ctx.input_json, "next_body")
            .or_else(|_| required_str(ctx.input_json, "body"))?;
        let now = now_epoch_seconds()?;
        let base = BTreeMap::from([
            ("id".to_string(), PatchValue::from(comment_id.to_string())),
            ("account_id".to_string(), PatchValue::from(ctx.account_id)),
            ("pr_id".to_string(), PatchValue::from(pr_id.clone())),
            ("kind".to_string(), PatchValue::from(kind.clone())),
            ("author_id".to_string(), PatchValue::from(author_id.clone())),
            ("created_at".to_string(), PatchValue::from(created_at)),
            ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
        ]);
        let patch = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: single_pk("id", PatchValue::from(comment_id.to_string())),
                before: Some({
                    let mut row = base.clone();
                    row.insert("body".to_string(), PatchValue::from(previous_body));
                    row.insert(
                        "updated_at".to_string(),
                        PatchValue::from(previous_updated_at),
                    );
                    row
                }),
                after: Some({
                    let mut row = base;
                    row.insert("body".to_string(), PatchValue::from(next_body.to_string()));
                    row.insert("updated_at".to_string(), PatchValue::from(now));
                    row.insert(
                        "pending_state".to_string(),
                        PatchValue::Json(mutation_state_json(ctx.mutation_id, "edit-comment")),
                    );
                    row
                }),
            }],
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(patch, ServerCallShape::None, Vec::new()))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct RestComment {
                id: i64,
                #[serde(rename = "node_id")]
                node_id: Option<String>,
                body: String,
                #[serde(rename = "created_at")]
                created_at: String,
                #[serde(rename = "updated_at")]
                updated_at: String,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let comment_id = required_str(ctx.input_json, "comment_id")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let author_id = required_str(ctx.input_json, "author_id")?;
            let body = required_str(ctx.input_json, "next_body")
                .or_else(|_| required_str(ctx.input_json, "body"))?;
            let path = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
            let (payload, _rate_limit) = ctx
                .github
                .rest_mutation_json::<RestComment>(
                    ctx.account_id,
                    reqwest::Method::PATCH,
                    &path,
                    Some(serde_json::json!({ "body": body })),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let payload = payload.context("github edit comment response missing body")?;
            let server_id = payload
                .node_id
                .unwrap_or_else(|| format!("comment-{}", payload.id));
            Ok(ServerResponse {
                upserts: vec![ServerNode::Comment(CommentRecord {
                    id: server_id.clone(),
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    kind: optional_str(ctx.input_json, "kind")
                        .unwrap_or("issue")
                        .to_string(),
                    author_id: author_id.to_string(),
                    body: payload.body.clone(),
                    created_at: crate::sync::reconcile::parse_timestamp(&payload.created_at),
                    updated_at: crate::sync::reconcile::parse_timestamp(&payload.updated_at),
                    deleted_at: None,
                    in_reply_to_id: optional_str(ctx.input_json, "in_reply_to_id")
                        .map(ToString::to_string),
                    review_id: optional_str(ctx.input_json, "review_id").map(ToString::to_string),
                    thread_id: optional_str(ctx.input_json, "thread_id").map(ToString::to_string),
                    path: optional_str(ctx.input_json, "path").map(ToString::to_string),
                    line: optional_i64(ctx.input_json, "line"),
                    side: optional_str(ctx.input_json, "side").map(ToString::to_string),
                    start_line: optional_i64(ctx.input_json, "start_line"),
                    start_side: optional_str(ctx.input_json, "start_side").map(ToString::to_string),
                    original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                        .map(ToString::to_string),
                })],
                markdown_overlays: vec![MarkdownOverlay {
                    target: MarkdownOverlayTarget::Comment {
                        comment_id: server_id,
                    },
                    predicted_body: body.to_string(),
                    server_body: payload.body,
                }],
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

impl Mutation for DeleteComment {
    fn kind(&self) -> MutationKind {
        MutationKind::DeleteComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let comment_id = required_str(ctx.input_json, "comment_id")?;
        let pr_id = required_str(ctx.input_json, "pr_id")?.to_string();
        let kind = optional_str(ctx.input_json, "kind")
            .unwrap_or("issue")
            .to_string();
        let author_id = required_str(ctx.input_json, "author_id")?.to_string();
        let body = required_str(ctx.input_json, "body")?.to_string();
        let created_at = optional_i64(ctx.input_json, "created_at").unwrap_or(0);
        let updated_at = optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(0);
        let in_reply_to_id =
            optional_str(ctx.input_json, "in_reply_to_id").map(ToString::to_string);
        let review_id = optional_str(ctx.input_json, "review_id").map(ToString::to_string);
        let thread_id = optional_str(ctx.input_json, "thread_id").map(ToString::to_string);
        let path = optional_str(ctx.input_json, "path").map(ToString::to_string);
        let line = optional_i64(ctx.input_json, "line");
        let side = optional_str(ctx.input_json, "side").map(ToString::to_string);
        let start_line = optional_i64(ctx.input_json, "start_line");
        let start_side = optional_str(ctx.input_json, "start_side").map(ToString::to_string);
        let original_commit_sha =
            optional_str(ctx.input_json, "original_commit_sha").map(ToString::to_string);
        let mut before = BTreeMap::from([
            ("id".to_string(), PatchValue::from(comment_id.to_string())),
            ("account_id".to_string(), PatchValue::from(ctx.account_id)),
            ("pr_id".to_string(), PatchValue::from(pr_id)),
            ("kind".to_string(), PatchValue::from(kind)),
            ("author_id".to_string(), PatchValue::from(author_id)),
            ("body".to_string(), PatchValue::from(body)),
            ("created_at".to_string(), PatchValue::from(created_at)),
            ("updated_at".to_string(), PatchValue::from(updated_at)),
            ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
        ]);
        if let Some(value) = in_reply_to_id {
            before.insert("in_reply_to_id".to_string(), PatchValue::from(value));
        }
        if let Some(value) = review_id {
            before.insert("review_id".to_string(), PatchValue::from(value));
        }
        if let Some(value) = thread_id {
            before.insert("thread_id".to_string(), PatchValue::from(value));
        }
        if let Some(value) = path {
            before.insert("path".to_string(), PatchValue::from(value));
        }
        if let Some(value) = line {
            before.insert("line".to_string(), PatchValue::from(value));
        }
        if let Some(value) = side {
            before.insert("side".to_string(), PatchValue::from(value));
        }
        if let Some(value) = start_line {
            before.insert("start_line".to_string(), PatchValue::from(value));
        }
        if let Some(value) = start_side {
            before.insert("start_side".to_string(), PatchValue::from(value));
        }
        if let Some(value) = original_commit_sha {
            before.insert("original_commit_sha".to_string(), PatchValue::from(value));
        }

        let patch = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: single_pk("id", PatchValue::from(comment_id.to_string())),
                before: Some(before),
                after: None,
            }],
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(patch, ServerCallShape::None, Vec::new()))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let comment_id = required_str(ctx.input_json, "comment_id")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
            ctx.github
                .rest_mutation_no_response(
                    ctx.account_id,
                    reqwest::Method::DELETE,
                    &path,
                    None,
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            Ok(ServerResponse {
                upserts: Vec::new(),
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }

    fn reconcile<'a>(
        &'a self,
        ctx: &'a ReconcileCtx<'_>,
        response: ServerResponse,
    ) -> BoxMutationFuture<'a, Result<()>> {
        Box::pin(async move { super::super::reconciler::reconcile(ctx, response).await })
    }
}

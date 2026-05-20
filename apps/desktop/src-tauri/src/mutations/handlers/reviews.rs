use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::api::SUBMIT_PULL_REQUEST_REVIEW_MUTATION;
use crate::db::{CommentRecord, ReviewRecord};
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, IdMappingDraft, MarkdownOverlay, MarkdownOverlayTarget, Mutation,
    MutationKind, OptimismLevel, PredictCtx, PredictedEffect, ServerCallShape, ServerNode,
    ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, now_epoch_seconds, optional_str, predicted_effect,
    required_str, single_pk,
};

#[derive(Debug)]
pub struct SubmitReview;

impl Mutation for SubmitReview {
    fn kind(&self) -> MutationKind {
        MutationKind::SubmitReview
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let author_id = required_str(ctx.input_json, "author_id")?;
        let body = optional_str(ctx.input_json, "body").unwrap_or_default();
        let review_state = optional_str(ctx.input_json, "event").unwrap_or("COMMENT");
        let local_review_id = ctx
            .input_json
            .get("local_review_id")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                format!(
                    "local-review-{}-{}",
                    now_epoch_seconds().unwrap_or_default(),
                    &ctx.mutation_id[0..ctx.mutation_id.len().min(8)]
                )
            });
        let now = now_epoch_seconds()?;

        let row = BTreeMap::from([
            ("id".to_string(), PatchValue::from(local_review_id.clone())),
            ("account_id".to_string(), PatchValue::from(ctx.account_id)),
            ("pr_id".to_string(), PatchValue::from(pr_id)),
            ("author_id".to_string(), PatchValue::from(author_id)),
            ("state".to_string(), PatchValue::from("SUBMITTING")),
            ("body".to_string(), PatchValue::from(body)),
            ("created_at".to_string(), PatchValue::from(now)),
            ("updated_at".to_string(), PatchValue::from(now)),
            (
                "pending_state".to_string(),
                PatchValue::Json(mutation_state_json(ctx.mutation_id, "submit-review")),
            ),
            ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
        ]);
        let patch = Patch {
            operations: vec![RowMutation {
                table: "reviews".to_string(),
                pk: single_pk("id", PatchValue::from(local_review_id.clone())),
                before: None,
                after: Some(row),
            }],
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(
            patch,
            ServerCallShape::Opaque(serde_json::json!({
                "local_review_id": local_review_id,
                "event": review_state,
            })),
            Vec::new(),
        ))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct SubmitData {
                #[serde(rename = "submitPullRequestReview")]
                submit_pull_request_review: Option<SubmitPayload>,
            }
            #[derive(serde::Deserialize)]
            struct SubmitPayload {
                #[serde(rename = "pullRequestReview")]
                pull_request_review: Option<ReviewNode>,
            }
            #[derive(serde::Deserialize)]
            struct ReviewNode {
                id: String,
                state: String,
                body: Option<String>,
                #[serde(rename = "createdAt")]
                created_at: String,
                #[serde(rename = "updatedAt")]
                updated_at: String,
                #[serde(rename = "submittedAt")]
                submitted_at: Option<String>,
                author: Option<AuthorNode>,
                comments: Option<CommentConnection>,
            }
            #[derive(serde::Deserialize)]
            struct AuthorNode {
                id: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct CommentConnection {
                nodes: Option<Vec<Option<CommentNode>>>,
            }
            #[derive(serde::Deserialize)]
            struct CommentNode {
                id: String,
                body: String,
                #[serde(rename = "createdAt")]
                created_at: String,
                #[serde(rename = "updatedAt")]
                updated_at: String,
            }

            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let pull_request_id = required_str(ctx.input_json, "pull_request_id")?;
            let event = optional_str(ctx.input_json, "event").unwrap_or("COMMENT");
            let body = optional_str(ctx.input_json, "body").unwrap_or_default();
            let local_review_id = match ctx.server_call {
                ServerCallShape::Opaque(value) => value
                    .get("local_review_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                _ => ctx
                    .input_json
                    .get("local_review_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            };

            let (payload, _rate_limit) = ctx
                .github
                .graphql_mutation::<SubmitData>(
                    ctx.account_id,
                    SUBMIT_PULL_REQUEST_REVIEW_MUTATION,
                    serde_json::json!({
                        "pullRequestId": pull_request_id,
                        "event": event,
                        "body": body,
                    }),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let review = payload
                .submit_pull_request_review
                .and_then(|value| value.pull_request_review)
                .context("submit review mutation missing pullRequestReview")?;
            let author_id = review
                .author
                .and_then(|author| author.id)
                .unwrap_or_else(|| {
                    required_str(ctx.input_json, "author_id")
                        .unwrap_or("unknown")
                        .to_string()
                });
            let review_record = ReviewRecord {
                id: review.id.clone(),
                account_id: ctx.account_id.to_string(),
                pr_id: pr_id.to_string(),
                author_id,
                state: review.state.clone(),
                body: review.body.clone().unwrap_or_default(),
                commit_sha: optional_str(ctx.input_json, "commit_sha").map(ToString::to_string),
                submitted_at: review
                    .submitted_at
                    .as_deref()
                    .map(crate::sync::reconcile::parse_timestamp),
                created_at: crate::sync::reconcile::parse_timestamp(&review.created_at),
                updated_at: crate::sync::reconcile::parse_timestamp(&review.updated_at),
            };

            let mut upserts = vec![ServerNode::Review(review_record)];
            for node in review
                .comments
                .and_then(|comments| comments.nodes)
                .unwrap_or_default()
                .into_iter()
                .flatten()
            {
                upserts.push(ServerNode::Comment(CommentRecord {
                    id: node.id,
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    kind: "review".to_string(),
                    author_id: required_str(ctx.input_json, "author_id")
                        .unwrap_or("unknown")
                        .to_string(),
                    body: node.body,
                    created_at: crate::sync::reconcile::parse_timestamp(&node.created_at),
                    updated_at: crate::sync::reconcile::parse_timestamp(&node.updated_at),
                    deleted_at: None,
                    in_reply_to_id: None,
                    review_id: Some(review.id.clone()),
                    thread_id: None,
                    path: None,
                    line: None,
                    side: None,
                    start_line: None,
                    start_side: None,
                    original_commit_sha: None,
                }));
            }

            Ok(ServerResponse {
                upserts,
                markdown_overlays: vec![MarkdownOverlay {
                    target: MarkdownOverlayTarget::Review {
                        review_id: review.id.clone(),
                    },
                    predicted_body: body.to_string(),
                    server_body: review.body.unwrap_or_default(),
                }],
                id_mappings: vec![IdMappingDraft {
                    kind: "review".to_string(),
                    local_id: local_review_id,
                    server_id: review.id,
                }],
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

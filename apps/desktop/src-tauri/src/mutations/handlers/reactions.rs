use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, now_epoch_seconds, optional_i64, optional_str,
    predicted_effect, required_str, single_pk,
};

#[derive(Debug)]
pub struct AddReaction;

#[derive(Debug)]
pub struct RemoveReaction;

impl Mutation for AddReaction {
    fn kind(&self) -> MutationKind {
        MutationKind::AddReaction
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        touch_comment_prediction(ctx, "add-reaction")
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct ReactionResponse {
                id: i64,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let target_id = required_str(ctx.input_json, "target_id")
                .or_else(|_| required_str(ctx.input_json, "comment_id"))?;
            let content = required_str(ctx.input_json, "content")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = reaction_path(ctx.input_json, owner, repo, target_id)?;
            let (payload, _rate_limit) = ctx
                .github
                .rest_mutation_json::<ReactionResponse>(
                    ctx.account_id,
                    reqwest::Method::POST,
                    &path,
                    Some(serde_json::json!({ "content": content })),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let payload = payload.context("github add reaction response missing body")?;
            let _ = payload.id;
            Ok(ServerResponse {
                upserts: Vec::new(),
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

impl Mutation for RemoveReaction {
    fn kind(&self) -> MutationKind {
        MutationKind::RemoveReaction
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        touch_comment_prediction(ctx, "remove-reaction")
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let target_id = required_str(ctx.input_json, "target_id")
                .or_else(|_| required_str(ctx.input_json, "comment_id"))?;
            let reaction_id = required_str(ctx.input_json, "reaction_id")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = format!(
                "{}/{reaction_id}",
                reaction_path(ctx.input_json, owner, repo, target_id)?
            );
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
}

fn touch_comment_prediction(ctx: &PredictCtx<'_>, pending_kind: &str) -> Result<PredictedEffect> {
    let comment_id = required_str(ctx.input_json, "comment_id")
        .or_else(|_| required_str(ctx.input_json, "target_id"))?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let author_id = required_str(ctx.input_json, "author_id")?;
    let body = optional_str(ctx.input_json, "body").unwrap_or_default();
    let kind = optional_str(ctx.input_json, "kind").unwrap_or("issue");
    let created_at = optional_i64(ctx.input_json, "created_at").unwrap_or(0);
    let previous_updated_at = optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(0);
    let now = now_epoch_seconds()?;

    let patch = Patch {
        operations: vec![RowMutation {
            table: "comments".to_string(),
            pk: single_pk("id", PatchValue::from(comment_id.to_string())),
            before: Some(BTreeMap::from([
                ("id".to_string(), PatchValue::from(comment_id.to_string())),
                ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                ("pr_id".to_string(), PatchValue::from(pr_id)),
                ("kind".to_string(), PatchValue::from(kind)),
                ("author_id".to_string(), PatchValue::from(author_id)),
                ("body".to_string(), PatchValue::from(body)),
                ("created_at".to_string(), PatchValue::from(created_at)),
                (
                    "updated_at".to_string(),
                    PatchValue::from(previous_updated_at),
                ),
                ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
            ])),
            after: Some(BTreeMap::from([
                ("id".to_string(), PatchValue::from(comment_id.to_string())),
                ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                ("pr_id".to_string(), PatchValue::from(pr_id)),
                ("kind".to_string(), PatchValue::from(kind)),
                ("author_id".to_string(), PatchValue::from(author_id)),
                ("body".to_string(), PatchValue::from(body)),
                ("created_at".to_string(), PatchValue::from(created_at)),
                ("updated_at".to_string(), PatchValue::from(now)),
                (
                    "pending_state".to_string(),
                    PatchValue::Json(mutation_state_json(ctx.mutation_id, pending_kind)),
                ),
                ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
            ])),
        }],
        pending_overlay_kind: None,
    };

    Ok(predicted_effect(patch, ServerCallShape::None, Vec::new()))
}

fn reaction_path(
    input: &serde_json::Value,
    owner: &str,
    repo: &str,
    target_id: &str,
) -> Result<String> {
    let target_type = optional_str(input, "target_type").unwrap_or("issue_comment");
    let path = match target_type {
        "issue_comment" => format!("/repos/{owner}/{repo}/issues/comments/{target_id}/reactions"),
        "review_comment" => format!("/repos/{owner}/{repo}/pulls/comments/{target_id}/reactions"),
        "pull_request_review" => {
            let pr_number = required_str(input, "pr_number")?;
            format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews/{target_id}/reactions")
        }
        _ => format!("/repos/{owner}/{repo}/issues/comments/{target_id}/reactions"),
    };
    Ok(path)
}

use anyhow::Result;

use crate::api::{DISABLE_PULL_REQUEST_AUTOMERGE_MUTATION, ENABLE_PULL_REQUEST_AUTOMERGE_MUTATION};
use crate::db::PullRequestRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, no_op_effect, now_epoch_seconds, optional_i64,
    optional_str, predicted_effect, required_i64, required_str, single_pk,
};

#[derive(Debug)]
pub struct EnableAutoMerge;
#[derive(Debug)]
pub struct DisableAutoMerge;
#[derive(Debug)]
pub struct UpdateBranch;
#[derive(Debug)]
pub struct Merge;
#[derive(Debug)]
pub struct ClosePr;
#[derive(Debug)]
pub struct ReopenPr;

impl Mutation for EnableAutoMerge {
    fn kind(&self) -> MutationKind {
        MutationKind::EnableAutoMerge
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::None
    }
    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let pull_request_id = required_str(ctx.input_json, "pull_request_id")?;
            let merge_method = optional_str(ctx.input_json, "merge_method").unwrap_or("SQUASH");
            let _ = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    ENABLE_PULL_REQUEST_AUTOMERGE_MUTATION,
                    serde_json::json!({
                        "pullRequestId": pull_request_id,
                        "mergeMethod": merge_method,
                    }),
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

impl Mutation for DisableAutoMerge {
    fn kind(&self) -> MutationKind {
        MutationKind::DisableAutoMerge
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::None
    }
    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let pull_request_id = required_str(ctx.input_json, "pull_request_id")?;
            let _ = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    DISABLE_PULL_REQUEST_AUTOMERGE_MUTATION,
                    serde_json::json!({
                        "pullRequestId": pull_request_id,
                    }),
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

impl Mutation for UpdateBranch {
    fn kind(&self) -> MutationKind {
        MutationKind::UpdateBranch
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let now = now_epoch_seconds()?;
        let current_state = optional_str(ctx.input_json, "state").unwrap_or("open");
        let current_merge_status =
            optional_str(ctx.input_json, "merge_state_status").unwrap_or("CLEAN");
        let mut before = base_pr_row(ctx, pr_id, now, current_state);
        before.insert(
            "updated_at".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(now)),
        );
        before.insert(
            "merge_state_status".to_string(),
            PatchValue::from(current_merge_status.to_string()),
        );
        if let Some(mergeable_state) = optional_str(ctx.input_json, "mergeable_state") {
            before.insert(
                "mergeable_state".to_string(),
                PatchValue::from(mergeable_state.to_string()),
            );
        }
        let mut after = before.clone();
        after.insert(
            "merge_state_status".to_string(),
            PatchValue::from("UPDATING".to_string()),
        );
        after.insert("updated_at".to_string(), PatchValue::from(now));
        after.insert(
            "pending_state".to_string(),
            PatchValue::Json(mutation_state_json(ctx.mutation_id, "update-branch")),
        );
        Ok(predicted_effect(
            Patch {
                operations: vec![RowMutation {
                    table: "pull_requests".to_string(),
                    pk: single_pk("id", PatchValue::from(pr_id.to_string())),
                    before: Some(before),
                    after: Some(after),
                }],
                pending_overlay_kind: None,
            },
            ServerCallShape::None,
            Vec::new(),
        ))
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = format!("/repos/{owner}/{repo}/pulls/{number}/update-branch");
            let _ = ctx
                .github
                .rest_mutation_json::<serde_json::Value>(
                    ctx.account_id,
                    reqwest::Method::PUT,
                    &path,
                    Some(serde_json::json!({})),
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

impl Mutation for Merge {
    fn kind(&self) -> MutationKind {
        MutationKind::Merge
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::None
    }
    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct MergePayload {
                merged: Option<bool>,
                #[serde(rename = "sha")]
                merge_sha: Option<String>,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = format!("/repos/{owner}/{repo}/pulls/{number}/merge");
            let (payload, _rate_limit) = ctx
                .github
                .rest_mutation_json::<MergePayload>(
                    ctx.account_id,
                    reqwest::Method::PUT,
                    &path,
                    Some(serde_json::json!({
                        "merge_method": optional_str(ctx.input_json, "merge_method").unwrap_or("merge"),
                        "sha": optional_str(ctx.input_json, "expected_head_sha"),
                    })),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let _merged = payload
                .as_ref()
                .and_then(|value| value.merged)
                .unwrap_or(false);
            let now = now_epoch_seconds()?;
            let record = PullRequestRecord {
                id: pr_id.to_string(),
                account_id: ctx.account_id.to_string(),
                repo_id: required_str(ctx.input_json, "repo_id")
                    .unwrap_or_default()
                    .to_string(),
                number,
                state: "closed".to_string(),
                draft: false,
                title: optional_str(ctx.input_json, "title")
                    .unwrap_or_default()
                    .to_string(),
                body: optional_str(ctx.input_json, "body")
                    .unwrap_or_default()
                    .to_string(),
                author_id: optional_str(ctx.input_json, "author_id").map(ToString::to_string),
                base_ref: optional_str(ctx.input_json, "base_ref")
                    .unwrap_or("main")
                    .to_string(),
                base_sha: optional_str(ctx.input_json, "base_sha")
                    .unwrap_or_default()
                    .to_string(),
                head_ref: optional_str(ctx.input_json, "head_ref")
                    .unwrap_or_default()
                    .to_string(),
                head_sha: payload
                    .and_then(|value| value.merge_sha)
                    .or_else(|| optional_str(ctx.input_json, "head_sha").map(ToString::to_string))
                    .unwrap_or_default(),
                head_repo_id: optional_str(ctx.input_json, "head_repo_id").map(ToString::to_string),
                mergeable_state: Some("MERGED".to_string()),
                merge_state_status: Some("MERGED".to_string()),
                additions: optional_i64(ctx.input_json, "additions").unwrap_or(0),
                deletions: optional_i64(ctx.input_json, "deletions").unwrap_or(0),
                changed_files: optional_i64(ctx.input_json, "changed_files").unwrap_or(0),
                comments_count: optional_i64(ctx.input_json, "comments_count").unwrap_or(0),
                reviews_count: optional_i64(ctx.input_json, "reviews_count").unwrap_or(0),
                commits_count: optional_i64(ctx.input_json, "commits_count").unwrap_or(0),
                is_read: optional_i64(ctx.input_json, "is_read").unwrap_or(1) == 1,
                html_url: optional_str(ctx.input_json, "html_url").map(ToString::to_string),
                created_at: optional_i64(ctx.input_json, "created_at").unwrap_or(now),
                updated_at: now,
                closed_at: Some(now),
                merged_at: Some(now),
            };
            Ok(ServerResponse {
                upserts: vec![ServerNode::PullRequest(record)],
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

impl Mutation for ClosePr {
    fn kind(&self) -> MutationKind {
        MutationKind::ClosePr
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        close_state_patch(ctx, "closed", "close-pr")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_state_patch(ctx, "closed").await })
    }
}

impl Mutation for ReopenPr {
    fn kind(&self) -> MutationKind {
        MutationKind::ReopenPr
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        close_state_patch(ctx, "open", "reopen-pr")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_state_patch(ctx, "open").await })
    }
}

fn close_state_patch(
    ctx: &PredictCtx<'_>,
    state: &str,
    pending_kind: &str,
) -> Result<PredictedEffect> {
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let now = now_epoch_seconds()?;
    let previous_state = optional_str(ctx.input_json, "previous_state").unwrap_or("open");
    let mut before = base_pr_row(ctx, pr_id, now, previous_state);
    before.insert(
        "updated_at".to_string(),
        PatchValue::from(optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(now)),
    );
    let mut after = before.clone();
    after.insert("state".to_string(), PatchValue::from(state.to_string()));
    after.insert("updated_at".to_string(), PatchValue::from(now));
    after.insert(
        "pending_state".to_string(),
        PatchValue::Json(mutation_state_json(ctx.mutation_id, pending_kind)),
    );
    Ok(predicted_effect(
        Patch {
            operations: vec![RowMutation {
                table: "pull_requests".to_string(),
                pk: single_pk("id", PatchValue::from(pr_id.to_string())),
                before: Some(before),
                after: Some(after),
            }],
            pending_overlay_kind: None,
        },
        ServerCallShape::None,
        Vec::new(),
    ))
}

fn base_pr_row(
    ctx: &PredictCtx<'_>,
    pr_id: &str,
    now: i64,
    state: &str,
) -> std::collections::BTreeMap<String, PatchValue> {
    std::collections::BTreeMap::from([
        ("id".to_string(), PatchValue::from(pr_id.to_string())),
        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
        (
            "repo_id".to_string(),
            PatchValue::from(required_str(ctx.input_json, "repo_id").unwrap_or_default()),
        ),
        (
            "number".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "pr_number").unwrap_or(0)),
        ),
        ("state".to_string(), PatchValue::from(state.to_string())),
        (
            "draft".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "draft").unwrap_or(0)),
        ),
        (
            "title".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "title").unwrap_or_default()),
        ),
        (
            "body".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "body").unwrap_or_default()),
        ),
        (
            "base_ref".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "base_ref").unwrap_or("main")),
        ),
        (
            "base_sha".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "base_sha").unwrap_or_default()),
        ),
        (
            "head_ref".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "head_ref").unwrap_or_default()),
        ),
        (
            "head_sha".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "head_sha").unwrap_or_default()),
        ),
        (
            "is_read".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "is_read").unwrap_or(1)),
        ),
        ("created_at".to_string(), PatchValue::from(now)),
        ("updated_at".to_string(), PatchValue::from(now)),
        ("body_server_adjusted".to_string(), PatchValue::from(0_i64)),
    ])
}

async fn apply_state_patch(ctx: &ApplyCtx<'_>, state: &str) -> Result<ServerResponse> {
    #[derive(serde::Deserialize)]
    struct PullPayload {
        state: Option<String>,
    }
    let owner = required_str(ctx.input_json, "owner")?;
    let repo = required_str(ctx.input_json, "repo")?;
    let number = required_i64(ctx.input_json, "pr_number")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let path = format!("/repos/{owner}/{repo}/pulls/{number}");
    let (payload, _rate_limit) = ctx
        .github
        .rest_mutation_json::<PullPayload>(
            ctx.account_id,
            reqwest::Method::PATCH,
            &path,
            Some(serde_json::json!({ "state": state })),
            Some(ctx.idempotency_key),
        )
        .await
        .map_err(map_apply_error)?;
    let _ = payload
        .and_then(|value| value.state)
        .unwrap_or_else(|| state.to_string());
    Ok(ServerResponse {
        upserts: Vec::new(),
        markdown_overlays: Vec::new(),
        id_mappings: Vec::new(),
        refetch_pr_ids: vec![pr_id.to_string()],
        hard_conflict: None,
    })
}

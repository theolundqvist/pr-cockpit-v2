use anyhow::Result;
use sqlx::Row;

use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, optional_i64, optional_str, predicted_effect,
    required_str,
};

#[derive(Debug)]
pub struct MarkFileViewed;

#[derive(Debug)]
pub struct UnmarkFileViewed;

impl Mutation for MarkFileViewed {
    fn kind(&self) -> MutationKind {
        MutationKind::MarkFileViewed
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        viewed_patch(ctx, true)
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_viewed(ctx, true).await })
    }
}

impl Mutation for UnmarkFileViewed {
    fn kind(&self) -> MutationKind {
        MutationKind::UnmarkFileViewed
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        viewed_patch(ctx, false)
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_viewed(ctx, false).await })
    }
}

fn viewed_patch(ctx: &PredictCtx<'_>, viewed: bool) -> Result<PredictedEffect> {
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let head_sha = required_str(ctx.input_json, "head_sha")?;
    let path = required_str(ctx.input_json, "path")?;
    let status = optional_str(ctx.input_json, "status").unwrap_or("modified");
    let before_viewer = optional_str(ctx.input_json, "previous_viewed_by_account_id");
    let before_sha = optional_str(ctx.input_json, "previous_viewed_at_head_sha");
    let mut before = std::collections::BTreeMap::from([
        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
        ("pr_id".to_string(), PatchValue::from(pr_id)),
        ("head_sha".to_string(), PatchValue::from(head_sha)),
        ("path".to_string(), PatchValue::from(path)),
        ("status".to_string(), PatchValue::from(status)),
        (
            "is_binary".to_string(),
            PatchValue::from(
                ctx.input_json
                    .get("is_binary")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0),
            ),
        ),
        (
            "additions".to_string(),
            PatchValue::from(
                ctx.input_json
                    .get("additions")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0),
            ),
        ),
        (
            "deletions".to_string(),
            PatchValue::from(
                ctx.input_json
                    .get("deletions")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0),
            ),
        ),
    ]);
    if let Some(value) = before_viewer {
        before.insert(
            "viewed_by_account_id".to_string(),
            PatchValue::from(value.to_string()),
        );
    }
    if let Some(value) = before_sha {
        before.insert(
            "viewed_at_head_sha".to_string(),
            PatchValue::from(value.to_string()),
        );
    }
    let mut after = before.clone();
    if viewed {
        after.insert(
            "viewed_by_account_id".to_string(),
            PatchValue::from(ctx.account_id),
        );
        after.insert(
            "viewed_at_head_sha".to_string(),
            PatchValue::from(head_sha.to_string()),
        );
    } else {
        after.insert("viewed_by_account_id".to_string(), PatchValue::Null);
        after.insert("viewed_at_head_sha".to_string(), PatchValue::Null);
    }
    after.insert(
        "pending_state".to_string(),
        PatchValue::Json(mutation_state_json(
            ctx.mutation_id,
            if viewed {
                "mark-file-viewed"
            } else {
                "unmark-file-viewed"
            },
        )),
    );
    Ok(predicted_effect(
        Patch {
            operations: vec![RowMutation {
                table: "pr_files".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    ("head_sha".to_string(), PatchValue::from(head_sha)),
                    ("path".to_string(), PatchValue::from(path)),
                ]),
                before: Some(before),
                after: Some(after),
            }],
            pending_overlay_kind: None,
        },
        ServerCallShape::None,
        Vec::new(),
    ))
}

async fn apply_viewed(ctx: &ApplyCtx<'_>, viewed: bool) -> Result<ServerResponse> {
    let path = required_str(ctx.input_json, "path")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let locator = sqlx::query(
        "SELECT r.owner AS owner, r.name AS repo, pr.number AS pr_number
         FROM pull_requests pr
         JOIN repos r ON r.id = pr.repo_id
         WHERE pr.account_id = ?1 AND pr.id = ?2
         LIMIT 1",
    )
    .bind(ctx.account_id)
    .bind(pr_id)
    .fetch_optional(ctx.db.pool())
    .await?;
    let owner = optional_str(ctx.input_json, "owner")
        .map(ToString::to_string)
        .or_else(|| {
            locator
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("owner").ok())
        })
        .unwrap_or_else(|| "unknown-owner".to_string());
    let repo = optional_str(ctx.input_json, "repo")
        .map(ToString::to_string)
        .or_else(|| {
            locator
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("repo").ok())
        })
        .unwrap_or_else(|| "unknown-repo".to_string());
    let number = optional_i64(ctx.input_json, "pr_number")
        .or_else(|| {
            locator
                .as_ref()
                .and_then(|row| row.try_get::<i64, _>("pr_number").ok())
        })
        .unwrap_or(0);
    let encoded_path = path
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('/', "%2F");
    let endpoint = format!("/repos/{owner}/{repo}/pulls/{number}/files/{encoded_path}/viewed");
    let method = if viewed {
        reqwest::Method::PUT
    } else {
        reqwest::Method::DELETE
    };
    ctx.github
        .rest_mutation_no_response(
            ctx.account_id,
            method,
            &endpoint,
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
}

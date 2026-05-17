use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::api::{RESOLVE_REVIEW_THREAD_MUTATION, UNRESOLVE_REVIEW_THREAD_MUTATION};
use crate::db::ReviewThreadRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, now_epoch_seconds, optional_i64, optional_str,
    predicted_effect, required_str, single_pk,
};

#[derive(Debug)]
pub struct ResolveThread;

#[derive(Debug)]
pub struct UnresolveThread;

impl Mutation for ResolveThread {
    fn kind(&self) -> MutationKind {
        MutationKind::ResolveThread
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        thread_prediction(ctx, true, "resolve-thread")
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_thread(ctx, true).await })
    }
}

impl Mutation for UnresolveThread {
    fn kind(&self) -> MutationKind {
        MutationKind::UnresolveThread
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        thread_prediction(ctx, false, "unresolve-thread")
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_thread(ctx, false).await })
    }
}

fn thread_prediction(
    ctx: &PredictCtx<'_>,
    is_resolved: bool,
    pending_kind: &str,
) -> Result<PredictedEffect> {
    let thread_id = required_str(ctx.input_json, "thread_id")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let path = required_str(ctx.input_json, "path")?;
    let now = now_epoch_seconds()?;
    let previous_is_resolved = optional_i64(ctx.input_json, "previous_is_resolved").unwrap_or(0);
    let previous_updated_at = optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(now);
    let resolved_by_id = optional_str(ctx.input_json, "resolved_by_id");

    let mut before = BTreeMap::from([
        ("id".to_string(), PatchValue::from(thread_id.to_string())),
        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
        ("pr_id".to_string(), PatchValue::from(pr_id)),
        ("path".to_string(), PatchValue::from(path)),
        (
            "is_outdated".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "is_outdated").unwrap_or(0)),
        ),
        (
            "is_resolved".to_string(),
            PatchValue::from(previous_is_resolved),
        ),
        ("created_at".to_string(), PatchValue::from(now)),
        (
            "updated_at".to_string(),
            PatchValue::from(previous_updated_at),
        ),
    ]);
    if let Some(resolved_by_id) = resolved_by_id {
        before.insert(
            "resolved_by_id".to_string(),
            PatchValue::from(resolved_by_id.to_string()),
        );
    }
    let mut after = before.clone();
    after.insert(
        "is_resolved".to_string(),
        PatchValue::from(if is_resolved { 1_i64 } else { 0_i64 }),
    );
    after.insert("updated_at".to_string(), PatchValue::from(now));
    if is_resolved {
        if let Some(actor_id) = optional_str(ctx.input_json, "actor_id") {
            after.insert(
                "resolved_by_id".to_string(),
                PatchValue::from(actor_id.to_string()),
            );
        }
    } else {
        after.remove("resolved_by_id");
    }
    after.insert(
        "pending_state".to_string(),
        PatchValue::Json(mutation_state_json(ctx.mutation_id, pending_kind)),
    );

    Ok(predicted_effect(
        Patch {
            operations: vec![RowMutation {
                table: "review_threads".to_string(),
                pk: single_pk("id", PatchValue::from(thread_id.to_string())),
                before: Some(before),
                after: Some(after),
            }],
            pending_overlay_kind: None,
        },
        ServerCallShape::None,
        Vec::new(),
    ))
}

async fn apply_thread(ctx: &ApplyCtx<'_>, resolve: bool) -> Result<ServerResponse> {
    #[derive(serde::Deserialize)]
    struct ThreadData {
        #[serde(rename = "resolveReviewThread")]
        resolve_review_thread: Option<ThreadPayload>,
        #[serde(rename = "unresolveReviewThread")]
        unresolve_review_thread: Option<ThreadPayload>,
    }
    #[derive(serde::Deserialize)]
    struct ThreadPayload {
        thread: Option<ThreadNode>,
    }
    #[derive(serde::Deserialize)]
    struct ThreadNode {
        id: String,
        path: String,
        line: Option<i64>,
        side: Option<String>,
        #[serde(rename = "startLine")]
        start_line: Option<i64>,
        #[serde(rename = "startSide")]
        start_side: Option<String>,
        #[serde(rename = "isOutdated")]
        is_outdated: bool,
        #[serde(rename = "isResolved")]
        is_resolved: bool,
        #[serde(rename = "updatedAt")]
        updated_at: Option<String>,
        #[serde(rename = "resolvedBy")]
        resolved_by: Option<ResolvedBy>,
    }
    #[derive(serde::Deserialize)]
    struct ResolvedBy {
        id: Option<String>,
    }

    let thread_id = required_str(ctx.input_json, "thread_id")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let mutation = if resolve {
        RESOLVE_REVIEW_THREAD_MUTATION
    } else {
        UNRESOLVE_REVIEW_THREAD_MUTATION
    };
    let (payload, _rate_limit) = ctx
        .github
        .graphql_mutation::<ThreadData>(
            ctx.account_id,
            mutation,
            serde_json::json!({
                "threadId": thread_id,
            }),
            Some(ctx.idempotency_key),
        )
        .await
        .map_err(map_apply_error)?;
    let thread = if resolve {
        payload.resolve_review_thread.and_then(|value| value.thread)
    } else {
        payload
            .unresolve_review_thread
            .and_then(|value| value.thread)
    }
    .context("thread mutation missing thread payload")?;

    let updated_at = thread
        .updated_at
        .as_deref()
        .map(crate::sync::reconcile::parse_timestamp)
        .unwrap_or_else(|| now_epoch_seconds().unwrap_or_default());
    let created_at = optional_i64(ctx.input_json, "created_at").unwrap_or(updated_at);
    Ok(ServerResponse {
        upserts: vec![ServerNode::ReviewThread(ReviewThreadRecord {
            id: thread.id,
            account_id: ctx.account_id.to_string(),
            pr_id: pr_id.to_string(),
            path: thread.path,
            line: thread.line,
            side: thread.side,
            start_line: thread.start_line,
            start_side: thread.start_side,
            original_commit_sha: optional_str(ctx.input_json, "original_commit_sha")
                .map(ToString::to_string),
            original_path: optional_str(ctx.input_json, "original_path").map(ToString::to_string),
            original_position: optional_i64(ctx.input_json, "original_position"),
            original_line: optional_i64(ctx.input_json, "original_line"),
            is_outdated: thread.is_outdated,
            is_resolved: thread.is_resolved,
            resolved_by_id: thread.resolved_by.and_then(|value| value.id),
            created_at,
            updated_at,
        })],
        markdown_overlays: Vec::new(),
        id_mappings: Vec::new(),
        refetch_pr_ids: vec![pr_id.to_string()],
        hard_conflict: None,
    })
}

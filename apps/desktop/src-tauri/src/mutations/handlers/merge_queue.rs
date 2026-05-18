use anyhow::Result;

use crate::api::{
    DEQUEUE_PULL_REQUEST_MUTATION, ENQUEUE_PULL_REQUEST_MUTATION,
    REORDER_MERGE_QUEUE_ENTRY_MUTATION,
};
use crate::mutations::engine::MutationApplyError;
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, HardConflictDiff, Mutation, MutationKind, OptimismLevel,
    PredictCtx, PredictedEffect, ServerCallShape, ServerResponse,
};

use super::common::{map_apply_error, no_op_effect, optional_str, required_str};

#[derive(Debug)]
pub struct EnqueueMergeQueue;
#[derive(Debug)]
pub struct DequeueMergeQueue;
#[derive(Debug)]
pub struct ReorderMergeQueue;

impl Mutation for EnqueueMergeQueue {
    fn kind(&self) -> MutationKind {
        MutationKind::EnqueueMergeQueue
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
            let jump = ctx
                .input_json
                .get("jump")
                .and_then(serde_json::Value::as_bool);
            let expected_head_oid = ctx
                .input_json
                .get("expected_head_oid")
                .and_then(serde_json::Value::as_str);

            let result = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    ENQUEUE_PULL_REQUEST_MUTATION,
                    serde_json::json!({
                        "pullRequestId": pull_request_id,
                        "expectedHeadOid": expected_head_oid,
                        "jump": jump
                    }),
                    Some(ctx.idempotency_key),
                )
                .await;
            match result {
                Ok((_payload, _rate_limit)) => Ok(ServerResponse {
                    upserts: Vec::new(),
                    markdown_overlays: Vec::new(),
                    id_mappings: Vec::new(),
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                }),
                Err(error) => Err(map_merge_queue_error(error)),
            }
        })
    }
}

impl Mutation for DequeueMergeQueue {
    fn kind(&self) -> MutationKind {
        MutationKind::DequeueMergeQueue
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
            let entry_id = optional_str(ctx.input_json, "merge_queue_entry_id")
                .or_else(|| optional_str(ctx.input_json, "id"))
                .ok_or_else(|| {
                    anyhow::anyhow!("mutation input missing string `merge_queue_entry_id`")
                })?;

            let result = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    DEQUEUE_PULL_REQUEST_MUTATION,
                    serde_json::json!({
                        "id": entry_id
                    }),
                    Some(ctx.idempotency_key),
                )
                .await;
            match result {
                Ok((_payload, _rate_limit)) => Ok(ServerResponse {
                    upserts: Vec::new(),
                    markdown_overlays: Vec::new(),
                    id_mappings: Vec::new(),
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                }),
                Err(error) => Err(map_merge_queue_error(error)),
            }
        })
    }
}

impl Mutation for ReorderMergeQueue {
    fn kind(&self) -> MutationKind {
        MutationKind::ReorderMergeQueue
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
            let entry_id = optional_str(ctx.input_json, "merge_queue_entry_id")
                .or_else(|| optional_str(ctx.input_json, "id"))
                .ok_or_else(|| {
                    anyhow::anyhow!("mutation input missing string `merge_queue_entry_id`")
                })?;
            let mode = optional_str(ctx.input_json, "mode")
                .or_else(|| optional_str(ctx.input_json, "move_to_position"))
                .unwrap_or("TOP");

            let result = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    REORDER_MERGE_QUEUE_ENTRY_MUTATION,
                    serde_json::json!({
                        "id": entry_id,
                        "mode": mode
                    }),
                    Some(ctx.idempotency_key),
                )
                .await;
            match result {
                Ok((_payload, _rate_limit)) => Ok(ServerResponse {
                    upserts: Vec::new(),
                    markdown_overlays: Vec::new(),
                    id_mappings: Vec::new(),
                    refetch_pr_ids: vec![pr_id.to_string()],
                    hard_conflict: None,
                }),
                Err(error) => Err(map_merge_queue_error(error)),
            }
        })
    }
}

fn map_merge_queue_error(error: anyhow::Error) -> anyhow::Error {
    let mapped = map_apply_error(error);
    if mapped.downcast_ref::<MutationApplyError>().is_some() {
        return mapped;
    }

    let message = mapped.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("graphql mutation errors")
        || lower.contains("merge queue")
        || lower.contains("already in queue")
    {
        return MutationApplyError::conflict(
            message,
            Some(HardConflictDiff {
                summary: "Merge queue rejected this request.".to_string(),
                local_body: None,
                server_body: None,
                changed_fields: vec!["merge_queue".to_string()],
            }),
        )
        .into();
    }
    mapped
}

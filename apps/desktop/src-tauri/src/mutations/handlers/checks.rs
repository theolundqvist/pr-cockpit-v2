use anyhow::Result;

use crate::api::RERUN_CHECK_SUITE_MUTATION;
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerResponse,
};

use super::common::{map_apply_error, no_op_effect, optional_i64, required_i64, required_str};

#[derive(Debug)]
pub struct RerunCheckRun;

#[derive(Debug)]
pub struct RerunCheckSuite;

impl Mutation for RerunCheckRun {
    fn kind(&self) -> MutationKind {
        MutationKind::RerunCheckRun
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }

    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let check_run_rest_id = required_i64(ctx.input_json, "check_run_rest_id")?;
            let path = format!("/repos/{owner}/{repo}/check-runs/{check_run_rest_id}/rerequest");
            let _ = ctx
                .github
                .rest_mutation_no_response(
                    ctx.account_id,
                    reqwest::Method::POST,
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

impl Mutation for RerunCheckSuite {
    fn kind(&self) -> MutationKind {
        MutationKind::RerunCheckSuite
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }

    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let check_suite_id = required_str(ctx.input_json, "check_suite_id")?;
            let rerun_result = ctx
                .github
                .graphql_mutation::<serde_json::Value>(
                    ctx.account_id,
                    RERUN_CHECK_SUITE_MUTATION,
                    serde_json::json!({
                        "checkSuiteId": check_suite_id,
                    }),
                    Some(ctx.idempotency_key),
                )
                .await;

            if rerun_result.is_err() {
                let owner = required_str(ctx.input_json, "owner")?;
                let repo = required_str(ctx.input_json, "repo")?;
                if let Some(rest_suite_id) = optional_i64(ctx.input_json, "check_suite_rest_id") {
                    let path =
                        format!("/repos/{owner}/{repo}/check-suites/{rest_suite_id}/rerequest");
                    let _ = ctx
                        .github
                        .rest_mutation_no_response(
                            ctx.account_id,
                            reqwest::Method::POST,
                            &path,
                            Some(serde_json::json!({})),
                            Some(ctx.idempotency_key),
                        )
                        .await
                        .map_err(map_apply_error)?;
                } else {
                    rerun_result.map_err(map_apply_error)?;
                }
            }

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

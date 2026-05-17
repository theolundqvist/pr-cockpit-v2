use anyhow::{Context, Result};

use crate::db::PrLabelRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, predicted_effect, required_i64, required_str,
};

#[derive(Debug)]
pub struct AddLabel;

#[derive(Debug)]
pub struct RemoveLabel;

impl Mutation for AddLabel {
    fn kind(&self) -> MutationKind {
        MutationKind::AddLabel
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let label_name = required_str(ctx.input_json, "label_name")?;
        let label_color = ctx
            .input_json
            .get("label_color")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ffffff");
        let description = ctx
            .input_json
            .get("description")
            .and_then(serde_json::Value::as_str);
        let patch = Patch {
            operations: vec![RowMutation {
                table: "pr_labels".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    ("label_name".to_string(), PatchValue::from(label_name)),
                ]),
                before: None,
                after: Some({
                    let mut row = std::collections::BTreeMap::from([
                        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                        ("pr_id".to_string(), PatchValue::from(pr_id)),
                        ("label_name".to_string(), PatchValue::from(label_name)),
                        ("label_color".to_string(), PatchValue::from(label_color)),
                        (
                            "pending_state".to_string(),
                            PatchValue::Json(mutation_state_json(ctx.mutation_id, "add-label")),
                        ),
                    ]);
                    if let Some(description) = description {
                        row.insert(
                            "description".to_string(),
                            PatchValue::from(description.to_string()),
                        );
                    }
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
            struct LabelPayload {
                name: String,
                color: String,
                description: Option<String>,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let label_name = required_str(ctx.input_json, "label_name")?;
            let path = format!("/repos/{owner}/{repo}/issues/{number}/labels");
            let (payload, _rate_limit) = ctx
                .github
                .rest_mutation_json::<Vec<LabelPayload>>(
                    ctx.account_id,
                    reqwest::Method::POST,
                    &path,
                    Some(serde_json::json!({
                        "labels": [label_name],
                    })),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let labels = payload.context("github add label response missing body")?;
            Ok(ServerResponse {
                upserts: vec![ServerNode::PrLabels {
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    labels: labels
                        .into_iter()
                        .map(|label| PrLabelRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            label_name: label.name,
                            label_color: label.color,
                            description: label.description,
                        })
                        .collect(),
                }],
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

impl Mutation for RemoveLabel {
    fn kind(&self) -> MutationKind {
        MutationKind::RemoveLabel
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let label_name = required_str(ctx.input_json, "label_name")?;
        let label_color = ctx
            .input_json
            .get("label_color")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("ffffff");
        let patch = Patch {
            operations: vec![RowMutation {
                table: "pr_labels".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    ("label_name".to_string(), PatchValue::from(label_name)),
                ]),
                before: Some(std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    ("label_name".to_string(), PatchValue::from(label_name)),
                    ("label_color".to_string(), PatchValue::from(label_color)),
                ])),
                after: None,
            }],
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(patch, ServerCallShape::None, Vec::new()))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct LabelPayload {
                name: String,
                color: String,
                description: Option<String>,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let label_name = required_str(ctx.input_json, "label_name")?;
            let encoded = url_encode_path_component(label_name);
            let path = format!("/repos/{owner}/{repo}/issues/{number}/labels/{encoded}");
            let (payload, _rate_limit) = ctx
                .github
                .rest_mutation_json::<Vec<LabelPayload>>(
                    ctx.account_id,
                    reqwest::Method::DELETE,
                    &path,
                    None,
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            let labels = payload.unwrap_or_default();
            Ok(ServerResponse {
                upserts: vec![ServerNode::PrLabels {
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    labels: labels
                        .into_iter()
                        .map(|label| PrLabelRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            label_name: label.name,
                            label_color: label.color,
                            description: label.description,
                        })
                        .collect(),
                }],
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

fn url_encode_path_component(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('/', "%2F")
        .replace('#', "%23")
}

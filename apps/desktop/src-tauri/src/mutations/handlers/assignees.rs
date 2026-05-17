use anyhow::Result;

use crate::db::PrAssigneeRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    deduped_string_set, map_apply_error, mutation_state_json, now_epoch_seconds, optional_i64,
    predicted_effect, required_i64, required_str, string_vec,
};

#[derive(Debug)]
pub struct SetAssignees;

impl Mutation for SetAssignees {
    fn kind(&self) -> MutationKind {
        MutationKind::SetAssignees
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let now = now_epoch_seconds()?;
        let next = deduped_string_set(
            &ctx.input_json
                .get("next_user_ids")
                .map(|_| string_vec(ctx.input_json, "next_user_ids"))
                .transpose()?
                .unwrap_or_else(|| {
                    vec![required_str(ctx.input_json, "user_id")
                        .unwrap_or_default()
                        .to_string()]
                }),
        );
        let previous = deduped_string_set(
            &ctx.input_json
                .get("previous_user_ids")
                .map(|_| string_vec(ctx.input_json, "previous_user_ids"))
                .transpose()?
                .unwrap_or_default(),
        );

        let previous_set = previous.iter().collect::<std::collections::BTreeSet<_>>();
        let next_set = next.iter().collect::<std::collections::BTreeSet<_>>();
        let previous_assigned_at =
            optional_i64(ctx.input_json, "previous_assigned_at").unwrap_or(now);

        let mut operations = Vec::new();
        for user_id in previous_set.difference(&next_set) {
            operations.push(RowMutation {
                table: "pr_assignees".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                ]),
                before: Some(std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    (
                        "assigned_at".to_string(),
                        PatchValue::from(previous_assigned_at),
                    ),
                ])),
                after: None,
            });
        }
        for user_id in next_set.difference(&previous_set) {
            operations.push(RowMutation {
                table: "pr_assignees".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                ]),
                before: None,
                after: Some(std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    ("assigned_at".to_string(), PatchValue::from(now)),
                    (
                        "pending_state".to_string(),
                        PatchValue::Json(mutation_state_json(ctx.mutation_id, "set-assignees")),
                    ),
                ])),
            });
        }
        let patch = Patch {
            operations,
            pending_overlay_kind: None,
        };
        Ok(predicted_effect(patch, ServerCallShape::None, Vec::new()))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct User {
                id: serde_json::Value,
                login: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct IssueResponse {
                assignees: Option<Vec<User>>,
            }
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let path = format!("/repos/{owner}/{repo}/issues/{number}/assignees");

            let previous = ctx
                .input_json
                .get("previous_logins")
                .map(|_| string_vec(ctx.input_json, "previous_logins"))
                .transpose()?
                .unwrap_or_default();
            let next = ctx
                .input_json
                .get("next_logins")
                .map(|_| string_vec(ctx.input_json, "next_logins"))
                .transpose()?
                .unwrap_or_else(|| {
                    ctx.input_json
                        .get("user_login")
                        .and_then(serde_json::Value::as_str)
                        .map(|value| vec![value.to_string()])
                        .unwrap_or_default()
                });
            let previous_set = previous.iter().collect::<std::collections::BTreeSet<_>>();
            let next_set = next.iter().collect::<std::collections::BTreeSet<_>>();
            let add = next_set
                .difference(&previous_set)
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>();
            let remove = previous_set
                .difference(&next_set)
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>();

            if !add.is_empty() {
                ctx.github
                    .rest_mutation_json::<IssueResponse>(
                        ctx.account_id,
                        reqwest::Method::POST,
                        &path,
                        Some(serde_json::json!({ "assignees": add })),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
            }
            let final_issue = if !remove.is_empty() {
                let (payload, _rate_limit) = ctx
                    .github
                    .rest_mutation_json::<IssueResponse>(
                        ctx.account_id,
                        reqwest::Method::DELETE,
                        &path,
                        Some(serde_json::json!({ "assignees": remove })),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
                payload
            } else if !add.is_empty() {
                let (payload, _rate_limit) = ctx
                    .github
                    .rest_mutation_json::<IssueResponse>(
                        ctx.account_id,
                        reqwest::Method::GET,
                        &path,
                        None,
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
                payload
            } else {
                None
            };
            let now = now_epoch_seconds()?;
            let assignees = if let Some(issue) = final_issue {
                issue
                    .assignees
                    .unwrap_or_default()
                    .into_iter()
                    .map(|user| PrAssigneeRecord {
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        user_id: user
                            .id
                            .as_str()
                            .map(ToString::to_string)
                            .or(user.login)
                            .unwrap_or_else(|| "unknown".to_string()),
                        assigned_at: now,
                    })
                    .collect()
            } else {
                next_set
                    .into_iter()
                    .map(|login| PrAssigneeRecord {
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        user_id: login.to_string(),
                        assigned_at: now,
                    })
                    .collect()
            };

            Ok(ServerResponse {
                upserts: vec![ServerNode::PrAssignees {
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    assignees,
                }],
                markdown_overlays: Vec::new(),
                id_mappings: Vec::new(),
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

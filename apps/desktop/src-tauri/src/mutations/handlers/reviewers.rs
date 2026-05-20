use anyhow::Result;

use crate::db::PrReviewerRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    deduped_string_set, map_apply_error, mutation_state_json, now_epoch_seconds, optional_str,
    predicted_effect, required_i64, required_str, string_vec,
};

#[derive(Debug)]
pub struct RequestReview;

#[derive(Debug)]
pub struct RemoveReviewRequest;

impl Mutation for RequestReview {
    fn kind(&self) -> MutationKind {
        MutationKind::RequestReview
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        reviewer_patch(ctx, true)
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_review_request(ctx, reqwest::Method::POST).await })
    }
}

impl Mutation for RemoveReviewRequest {
    fn kind(&self) -> MutationKind {
        MutationKind::RemoveReviewRequest
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        reviewer_patch(ctx, false)
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_review_request(ctx, reqwest::Method::DELETE).await })
    }
}

fn reviewer_patch(ctx: &PredictCtx<'_>, adding: bool) -> Result<PredictedEffect> {
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let now = now_epoch_seconds()?;
    let next = deduped_string_set(
        &ctx.input_json
            .get("next_reviewers")
            .map(|_| string_vec(ctx.input_json, "next_reviewers"))
            .transpose()?
            .unwrap_or_else(|| {
                optional_str(ctx.input_json, "reviewer_id")
                    .map(|value| vec![value.to_string()])
                    .unwrap_or_default()
            }),
    );
    let previous = deduped_string_set(
        &ctx.input_json
            .get("previous_reviewers")
            .map(|_| string_vec(ctx.input_json, "previous_reviewers"))
            .transpose()?
            .unwrap_or_default(),
    );

    let previous_set = previous.iter().collect::<std::collections::BTreeSet<_>>();
    let next_set = next.iter().collect::<std::collections::BTreeSet<_>>();

    let mut operations = Vec::new();
    if adding {
        for user_id in next_set.difference(&previous_set) {
            operations.push(RowMutation {
                table: "pr_reviewers".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    ("reviewer_type".to_string(), PatchValue::from("user")),
                ]),
                before: None,
                after: Some(std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    ("reviewer_type".to_string(), PatchValue::from("user")),
                    ("reviewer_state".to_string(), PatchValue::from("requested")),
                    ("requested_at".to_string(), PatchValue::from(now)),
                    (
                        "pending_state".to_string(),
                        PatchValue::Json(mutation_state_json(ctx.mutation_id, "request-review")),
                    ),
                ])),
            });
        }
    } else {
        for user_id in previous_set.difference(&next_set) {
            operations.push(RowMutation {
                table: "pr_reviewers".to_string(),
                pk: std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    ("reviewer_type".to_string(), PatchValue::from("user")),
                ]),
                before: Some(std::collections::BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "user_id".to_string(),
                        PatchValue::from((*user_id).to_string()),
                    ),
                    ("reviewer_type".to_string(), PatchValue::from("user")),
                    ("reviewer_state".to_string(), PatchValue::from("requested")),
                    ("requested_at".to_string(), PatchValue::from(now)),
                ])),
                after: None,
            });
        }
    }

    Ok(predicted_effect(
        Patch {
            operations,
            pending_overlay_kind: None,
        },
        ServerCallShape::None,
        Vec::new(),
    ))
}

async fn apply_review_request(
    ctx: &ApplyCtx<'_>,
    method: reqwest::Method,
) -> Result<ServerResponse> {
    #[derive(serde::Deserialize)]
    struct User {
        id: serde_json::Value,
    }
    #[derive(serde::Deserialize)]
    struct Team {
        id: Option<String>,
        slug: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct PullPayload {
        #[serde(rename = "requested_reviewers")]
        requested_reviewers: Option<Vec<User>>,
        #[serde(rename = "requested_teams")]
        requested_teams: Option<Vec<Team>>,
    }

    let owner = required_str(ctx.input_json, "owner")?;
    let repo = required_str(ctx.input_json, "repo")?;
    let number = required_i64(ctx.input_json, "pr_number")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let user_logins = ctx
        .input_json
        .get("reviewer_logins")
        .map(|_| string_vec(ctx.input_json, "reviewer_logins"))
        .transpose()?
        .unwrap_or_default();
    let team_slugs = ctx
        .input_json
        .get("team_slugs")
        .map(|_| string_vec(ctx.input_json, "team_slugs"))
        .transpose()?
        .unwrap_or_default();
    let path = format!("/repos/{owner}/{repo}/pulls/{number}/requested_reviewers");
    let (payload, _rate_limit) = ctx
        .github
        .rest_mutation_json::<PullPayload>(
            ctx.account_id,
            method,
            &path,
            Some(serde_json::json!({
                "reviewers": user_logins,
                "team_reviewers": team_slugs,
            })),
            Some(ctx.idempotency_key),
        )
        .await
        .map_err(map_apply_error)?;
    let payload = payload.unwrap_or(PullPayload {
        requested_reviewers: Some(Vec::new()),
        requested_teams: Some(Vec::new()),
    });
    let now = now_epoch_seconds()?;
    let mut reviewers = Vec::new();
    for user in payload.requested_reviewers.unwrap_or_default() {
        let user_id = user
            .id
            .as_str()
            .map(ToString::to_string)
            .or_else(|| user.id.as_i64().map(|id| id.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        reviewers.push(PrReviewerRecord {
            account_id: ctx.account_id.to_string(),
            pr_id: pr_id.to_string(),
            user_id,
            reviewer_type: "user".to_string(),
            reviewer_state: "requested".to_string(),
            requested_at: now,
        });
    }
    for team in payload.requested_teams.unwrap_or_default() {
        let user_id = team
            .id
            .or(team.slug)
            .unwrap_or_else(|| "unknown".to_string());
        reviewers.push(PrReviewerRecord {
            account_id: ctx.account_id.to_string(),
            pr_id: pr_id.to_string(),
            user_id,
            reviewer_type: "team".to_string(),
            reviewer_state: "requested".to_string(),
            requested_at: now,
        });
    }
    Ok(ServerResponse {
        upserts: vec![ServerNode::PrReviewers {
            account_id: ctx.account_id.to_string(),
            pr_id: pr_id.to_string(),
            reviewers,
        }],
        markdown_overlays: Vec::new(),
        id_mappings: Vec::new(),
        refetch_pr_ids: vec![pr_id.to_string()],
        hard_conflict: None,
    })
}

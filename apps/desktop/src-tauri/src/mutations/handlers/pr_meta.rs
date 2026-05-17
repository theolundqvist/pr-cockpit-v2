use std::collections::BTreeMap;

use anyhow::Result;

use crate::api::{
    ADD_PROJECT_V2_ITEM_MUTATION, CONVERT_PULL_REQUEST_TO_DRAFT_MUTATION,
    MARK_PULL_REQUEST_READY_FOR_REVIEW_MUTATION, UPDATE_PROJECT_V2_ITEM_FIELD_VALUE_MUTATION,
};
use crate::db::PullRequestRecord;
use crate::mutations::patch::{Patch, PatchValue, RowMutation};
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, Mutation, MutationKind, OptimismLevel, PredictCtx,
    PredictedEffect, ServerCallShape, ServerNode, ServerResponse,
};

use super::common::{
    map_apply_error, mutation_state_json, now_epoch_seconds, optional_i64, optional_str,
    predicted_effect, required_i64, required_str, single_pk,
};

#[derive(Debug)]
pub struct UpdatePrTitle;
#[derive(Debug)]
pub struct UpdatePrDescription;
#[derive(Debug)]
pub struct SetMilestone;
#[derive(Debug)]
pub struct SetProject;
#[derive(Debug)]
pub struct ConvertToDraft;
#[derive(Debug)]
pub struct MarkReadyForReview;

impl Mutation for UpdatePrTitle {
    fn kind(&self) -> MutationKind {
        MutationKind::UpdatePrTitle
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        update_pr_patch(ctx, "update-pr-title")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_pr_patch(ctx, true).await })
    }
}

impl Mutation for UpdatePrDescription {
    fn kind(&self) -> MutationKind {
        MutationKind::UpdatePrDescription
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        update_pr_patch(ctx, "update-pr-description")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_pr_patch(ctx, false).await })
    }
}

impl Mutation for SetMilestone {
    fn kind(&self) -> MutationKind {
        MutationKind::SetMilestone
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let milestone_id = required_str(ctx.input_json, "milestone_id")?;
        let title = optional_str(ctx.input_json, "milestone_title").unwrap_or_default();
        let state = optional_str(ctx.input_json, "milestone_state").unwrap_or("open");
        let mut ops = Vec::new();
        if let Some(previous_id) = optional_str(ctx.input_json, "previous_milestone_id") {
            ops.push(RowMutation {
                table: "pr_milestones".to_string(),
                pk: BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "milestone_id".to_string(),
                        PatchValue::from(previous_id.to_string()),
                    ),
                ]),
                before: Some(BTreeMap::from([
                    ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                    ("pr_id".to_string(), PatchValue::from(pr_id)),
                    (
                        "milestone_id".to_string(),
                        PatchValue::from(previous_id.to_string()),
                    ),
                    (
                        "title".to_string(),
                        PatchValue::from(
                            optional_str(ctx.input_json, "previous_milestone_title")
                                .unwrap_or_default(),
                        ),
                    ),
                    (
                        "state".to_string(),
                        PatchValue::from(
                            optional_str(ctx.input_json, "previous_milestone_state")
                                .unwrap_or("open"),
                        ),
                    ),
                ])),
                after: None,
            });
        }
        let now = now_epoch_seconds()?;
        ops.push(RowMutation {
            table: "pr_milestones".to_string(),
            pk: BTreeMap::from([
                ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                ("pr_id".to_string(), PatchValue::from(pr_id)),
                ("milestone_id".to_string(), PatchValue::from(milestone_id)),
            ]),
            before: None,
            after: Some(BTreeMap::from([
                ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                ("pr_id".to_string(), PatchValue::from(pr_id)),
                ("milestone_id".to_string(), PatchValue::from(milestone_id)),
                ("title".to_string(), PatchValue::from(title)),
                ("state".to_string(), PatchValue::from(state)),
                (
                    "pending_state".to_string(),
                    PatchValue::Json(mutation_state_json(ctx.mutation_id, "set-milestone")),
                ),
                (
                    "due_on".to_string(),
                    PatchValue::from(optional_i64(ctx.input_json, "due_on").unwrap_or(now)),
                ),
            ])),
        });
        Ok(predicted_effect(
            Patch {
                operations: ops,
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
            let milestone_number = required_i64(ctx.input_json, "milestone_number")?;
            let path = format!("/repos/{owner}/{repo}/issues/{number}");
            ctx.github
                .rest_mutation_json::<serde_json::Value>(
                    ctx.account_id,
                    reqwest::Method::PATCH,
                    &path,
                    Some(serde_json::json!({
                        "milestone": milestone_number,
                    })),
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

impl Mutation for SetProject {
    fn kind(&self) -> MutationKind {
        MutationKind::SetProject
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let project_id = required_str(ctx.input_json, "project_id")?;
        let project_title = optional_str(ctx.input_json, "project_title").unwrap_or("Project");
        let now = now_epoch_seconds()?;
        Ok(predicted_effect(
            Patch {
                operations: vec![RowMutation {
                    table: "pr_projects".to_string(),
                    pk: BTreeMap::from([
                        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                        ("pr_id".to_string(), PatchValue::from(pr_id)),
                        ("project_id".to_string(), PatchValue::from(project_id)),
                    ]),
                    before: None,
                    after: Some(BTreeMap::from([
                        ("account_id".to_string(), PatchValue::from(ctx.account_id)),
                        ("pr_id".to_string(), PatchValue::from(pr_id)),
                        ("project_id".to_string(), PatchValue::from(project_id)),
                        ("project_title".to_string(), PatchValue::from(project_title)),
                        ("updated_at".to_string(), PatchValue::from(now)),
                        (
                            "pending_state".to_string(),
                            PatchValue::Json(mutation_state_json(ctx.mutation_id, "set-project")),
                        ),
                    ])),
                }],
                pending_overlay_kind: None,
            },
            ServerCallShape::None,
            Vec::new(),
        ))
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            #[derive(serde::Deserialize)]
            struct AddItemData {
                #[serde(rename = "addProjectV2ItemById")]
                add_item: Option<AddItemPayload>,
            }
            #[derive(serde::Deserialize)]
            struct AddItemPayload {
                item: Option<ProjectItem>,
            }
            #[derive(serde::Deserialize)]
            struct ProjectItem {
                id: String,
            }

            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let project_id = required_str(ctx.input_json, "project_id")?;
            let content_id = required_str(ctx.input_json, "pull_request_id")?;
            let (added, _rate_limit) = ctx
                .github
                .graphql_mutation::<AddItemData>(
                    ctx.account_id,
                    ADD_PROJECT_V2_ITEM_MUTATION,
                    serde_json::json!({
                        "projectId": project_id,
                        "contentId": content_id,
                    }),
                    Some(ctx.idempotency_key),
                )
                .await
                .map_err(map_apply_error)?;
            if let (Some(item), Some(field_id), Some(option_id)) = (
                added.add_item.and_then(|value| value.item),
                optional_str(ctx.input_json, "project_field_id"),
                optional_str(ctx.input_json, "project_field_option_id"),
            ) {
                let _ = ctx
                    .github
                    .graphql_mutation::<serde_json::Value>(
                        ctx.account_id,
                        UPDATE_PROJECT_V2_ITEM_FIELD_VALUE_MUTATION,
                        serde_json::json!({
                            "projectId": project_id,
                            "itemId": item.id,
                            "fieldId": field_id,
                            "optionId": option_id,
                        }),
                        Some(ctx.idempotency_key),
                    )
                    .await
                    .map_err(map_apply_error)?;
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

impl Mutation for ConvertToDraft {
    fn kind(&self) -> MutationKind {
        MutationKind::ConvertToDraft
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        draft_patch(ctx, true, "convert-to-draft")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_draft_toggle(ctx, true).await })
    }
}

impl Mutation for MarkReadyForReview {
    fn kind(&self) -> MutationKind {
        MutationKind::MarkReadyForReview
    }
    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Cautious
    }
    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        draft_patch(ctx, false, "mark-ready-for-review")
    }
    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move { apply_draft_toggle(ctx, false).await })
    }
}

fn update_pr_patch(ctx: &PredictCtx<'_>, pending_kind: &str) -> Result<PredictedEffect> {
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let now = now_epoch_seconds()?;
    let previous_title = optional_str(ctx.input_json, "previous_title").unwrap_or_default();
    let previous_body = optional_str(ctx.input_json, "previous_body").unwrap_or_default();
    let next_title = optional_str(ctx.input_json, "title").unwrap_or(previous_title);
    let next_body = optional_str(ctx.input_json, "body").unwrap_or(previous_body);
    let mut before = base_pr_row(ctx, pr_id, previous_title, previous_body, now);
    let mut after = base_pr_row(ctx, pr_id, next_title, next_body, now);
    before.insert(
        "updated_at".to_string(),
        PatchValue::from(optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(now)),
    );
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

fn draft_patch(ctx: &PredictCtx<'_>, draft: bool, pending_kind: &str) -> Result<PredictedEffect> {
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let now = now_epoch_seconds()?;
    let previous_title = optional_str(ctx.input_json, "previous_title").unwrap_or_default();
    let previous_body = optional_str(ctx.input_json, "previous_body").unwrap_or_default();
    let mut before = base_pr_row(ctx, pr_id, previous_title, previous_body, now);
    before.insert(
        "draft".to_string(),
        PatchValue::from(optional_i64(ctx.input_json, "previous_draft").unwrap_or(0)),
    );
    before.insert(
        "updated_at".to_string(),
        PatchValue::from(optional_i64(ctx.input_json, "previous_updated_at").unwrap_or(now)),
    );
    let mut after = before.clone();
    after.insert(
        "draft".to_string(),
        PatchValue::from(if draft { 1_i64 } else { 0_i64 }),
    );
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
    title: &str,
    body: &str,
    now: i64,
) -> BTreeMap<String, PatchValue> {
    BTreeMap::from([
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
        (
            "state".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "state").unwrap_or("open")),
        ),
        (
            "draft".to_string(),
            PatchValue::from(optional_i64(ctx.input_json, "draft").unwrap_or(0)),
        ),
        ("title".to_string(), PatchValue::from(title.to_string())),
        ("body".to_string(), PatchValue::from(body.to_string())),
        (
            "base_ref".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "base_ref").unwrap_or("main")),
        ),
        (
            "base_sha".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "base_sha").unwrap_or("")),
        ),
        (
            "head_ref".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "head_ref").unwrap_or("")),
        ),
        (
            "head_sha".to_string(),
            PatchValue::from(optional_str(ctx.input_json, "head_sha").unwrap_or("")),
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

async fn apply_pr_patch(ctx: &ApplyCtx<'_>, title_only: bool) -> Result<ServerResponse> {
    #[derive(serde::Deserialize)]
    struct PullPayload {
        title: Option<String>,
        body: Option<String>,
        state: Option<String>,
        draft: Option<bool>,
        #[serde(rename = "updated_at")]
        updated_at: Option<String>,
        #[serde(rename = "merged_at")]
        merged_at: Option<String>,
        #[serde(rename = "closed_at")]
        closed_at: Option<String>,
    }
    let owner = required_str(ctx.input_json, "owner")?;
    let repo = required_str(ctx.input_json, "repo")?;
    let number = required_i64(ctx.input_json, "pr_number")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let path = format!("/repos/{owner}/{repo}/pulls/{number}");
    let payload_body = if title_only {
        serde_json::json!({ "title": required_str(ctx.input_json, "title")? })
    } else {
        serde_json::json!({ "body": required_str(ctx.input_json, "body")? })
    };
    let (payload, _rate_limit) = ctx
        .github
        .rest_mutation_json::<PullPayload>(
            ctx.account_id,
            reqwest::Method::PATCH,
            &path,
            Some(payload_body),
            Some(ctx.idempotency_key),
        )
        .await
        .map_err(map_apply_error)?;
    let payload = payload.unwrap_or(PullPayload {
        title: optional_str(ctx.input_json, "title").map(ToString::to_string),
        body: optional_str(ctx.input_json, "body").map(ToString::to_string),
        state: optional_str(ctx.input_json, "state").map(ToString::to_string),
        draft: ctx
            .input_json
            .get("draft")
            .and_then(serde_json::Value::as_bool),
        updated_at: None,
        merged_at: None,
        closed_at: None,
    });
    let now = payload
        .updated_at
        .as_deref()
        .map(crate::sync::reconcile::parse_timestamp)
        .unwrap_or_else(|| now_epoch_seconds().unwrap_or_default());
    let record = PullRequestRecord {
        id: pr_id.to_string(),
        account_id: ctx.account_id.to_string(),
        repo_id: required_str(ctx.input_json, "repo_id")
            .unwrap_or_default()
            .to_string(),
        number,
        state: payload.state.unwrap_or_else(|| {
            optional_str(ctx.input_json, "state")
                .unwrap_or("open")
                .to_string()
        }),
        draft: payload
            .draft
            .unwrap_or(optional_str(ctx.input_json, "draft").is_some_and(|value| value == "true")),
        title: payload.title.unwrap_or_else(|| {
            optional_str(ctx.input_json, "title")
                .unwrap_or_default()
                .to_string()
        }),
        body: payload.body.unwrap_or_else(|| {
            optional_str(ctx.input_json, "body")
                .unwrap_or_default()
                .to_string()
        }),
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
        head_sha: optional_str(ctx.input_json, "head_sha")
            .unwrap_or_default()
            .to_string(),
        head_repo_id: optional_str(ctx.input_json, "head_repo_id").map(ToString::to_string),
        mergeable_state: optional_str(ctx.input_json, "mergeable_state").map(ToString::to_string),
        merge_state_status: optional_str(ctx.input_json, "merge_state_status")
            .map(ToString::to_string),
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
        closed_at: payload
            .closed_at
            .as_deref()
            .map(crate::sync::reconcile::parse_timestamp),
        merged_at: payload
            .merged_at
            .as_deref()
            .map(crate::sync::reconcile::parse_timestamp),
    };
    Ok(ServerResponse {
        upserts: vec![ServerNode::PullRequest(record)],
        markdown_overlays: Vec::new(),
        id_mappings: Vec::new(),
        refetch_pr_ids: vec![pr_id.to_string()],
        hard_conflict: None,
    })
}

async fn apply_draft_toggle(ctx: &ApplyCtx<'_>, draft: bool) -> Result<ServerResponse> {
    #[derive(serde::Deserialize)]
    struct MutationData {
        #[serde(rename = "convertPullRequestToDraft")]
        convert_pull_request_to_draft: Option<TogglePayload>,
        #[serde(rename = "markPullRequestReadyForReview")]
        mark_pull_request_ready_for_review: Option<TogglePayload>,
    }
    #[derive(serde::Deserialize)]
    struct TogglePayload {
        #[serde(rename = "pullRequest")]
        pull_request: Option<ToggleNode>,
    }
    #[derive(serde::Deserialize)]
    struct ToggleNode {
        title: Option<String>,
        body: Option<String>,
        state: Option<String>,
        #[serde(rename = "isDraft")]
        is_draft: Option<bool>,
        #[serde(rename = "updatedAt")]
        updated_at: Option<String>,
    }

    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let pull_request_id = required_str(ctx.input_json, "pull_request_id")?;
    let query = if draft {
        CONVERT_PULL_REQUEST_TO_DRAFT_MUTATION
    } else {
        MARK_PULL_REQUEST_READY_FOR_REVIEW_MUTATION
    };
    let (payload, _rate_limit) = ctx
        .github
        .graphql_mutation::<MutationData>(
            ctx.account_id,
            query,
            serde_json::json!({
                "pullRequestId": pull_request_id,
            }),
            Some(ctx.idempotency_key),
        )
        .await
        .map_err(map_apply_error)?;
    let node = if draft {
        payload
            .convert_pull_request_to_draft
            .and_then(|value| value.pull_request)
    } else {
        payload
            .mark_pull_request_ready_for_review
            .and_then(|value| value.pull_request)
    };
    let now = node
        .as_ref()
        .and_then(|value| value.updated_at.as_deref())
        .map(crate::sync::reconcile::parse_timestamp)
        .unwrap_or_else(|| now_epoch_seconds().unwrap_or_default());
    let record = PullRequestRecord {
        id: pr_id.to_string(),
        account_id: ctx.account_id.to_string(),
        repo_id: required_str(ctx.input_json, "repo_id")
            .unwrap_or_default()
            .to_string(),
        number: required_i64(ctx.input_json, "pr_number")?,
        state: node
            .as_ref()
            .and_then(|value| value.state.clone())
            .unwrap_or_else(|| {
                optional_str(ctx.input_json, "state")
                    .unwrap_or("open")
                    .to_string()
            }),
        draft: node
            .as_ref()
            .and_then(|value| value.is_draft)
            .unwrap_or(draft),
        title: node
            .as_ref()
            .and_then(|value| value.title.clone())
            .unwrap_or_else(|| {
                optional_str(ctx.input_json, "title")
                    .unwrap_or_default()
                    .to_string()
            }),
        body: node
            .as_ref()
            .and_then(|value| value.body.clone())
            .unwrap_or_else(|| {
                optional_str(ctx.input_json, "body")
                    .unwrap_or_default()
                    .to_string()
            }),
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
        head_sha: optional_str(ctx.input_json, "head_sha")
            .unwrap_or_default()
            .to_string(),
        head_repo_id: optional_str(ctx.input_json, "head_repo_id").map(ToString::to_string),
        mergeable_state: optional_str(ctx.input_json, "mergeable_state").map(ToString::to_string),
        merge_state_status: optional_str(ctx.input_json, "merge_state_status")
            .map(ToString::to_string),
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
        closed_at: optional_i64(ctx.input_json, "closed_at"),
        merged_at: optional_i64(ctx.input_json, "merged_at"),
    };
    Ok(ServerResponse {
        upserts: vec![ServerNode::PullRequest(record)],
        markdown_overlays: Vec::new(),
        id_mappings: Vec::new(),
        refetch_pr_ids: vec![pr_id.to_string()],
        hard_conflict: None,
    })
}

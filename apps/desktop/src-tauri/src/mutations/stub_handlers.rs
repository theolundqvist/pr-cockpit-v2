use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{Context, Result};

use super::engine::MutationApplyError;
use super::patch::{Patch, PatchValue, RowMutation};
use super::{
    ApplyCtx, IdMappingDraft, Mutation, MutationKind, OptimismLevel, PredictCtx, PredictedEffect,
    ServerCallShape, ServerNode, ServerResponse,
};
use crate::db::CommentRecord;

// TODO(m2-mutation-handlers): replace these stub handlers with real per-kind bodies.
pub fn stub_handlers() -> Vec<Arc<dyn Mutation>> {
    vec![
        Arc::new(AddCommentStub),
        Arc::new(EditCommentStub),
        Arc::new(DeleteCommentStub),
        Arc::new(AddReactionStub),
        Arc::new(RemoveReactionStub),
        Arc::new(AddLabelStub),
        Arc::new(RemoveLabelStub),
        Arc::new(SetAssigneesStub),
    ]
}

#[derive(Debug)]
pub struct AddCommentStub;

impl Mutation for AddCommentStub {
    fn kind(&self) -> MutationKind {
        MutationKind::AddComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let local_id = ctx
            .input_json
            .get("local_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("local-comment-stub");
        let pr_id = ctx
            .input_json
            .get("pr_id")
            .and_then(serde_json::Value::as_str)
            .context("add comment stub expects input_json.pr_id")?;
        let author_id = ctx
            .input_json
            .get("author_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("user-stub");
        let body = ctx
            .input_json
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let now = super::reconciler::now_epoch_seconds()?;

        let mut row = BTreeMap::new();
        row.insert("id".to_string(), PatchValue::from(local_id));
        row.insert("account_id".to_string(), PatchValue::from(ctx.account_id));
        row.insert("pr_id".to_string(), PatchValue::from(pr_id));
        row.insert("kind".to_string(), PatchValue::from("issue"));
        row.insert("author_id".to_string(), PatchValue::from(author_id));
        row.insert("body".to_string(), PatchValue::from(body));
        row.insert("created_at".to_string(), PatchValue::from(now));
        row.insert("updated_at".to_string(), PatchValue::from(now));
        row.insert("body_server_adjusted".to_string(), PatchValue::from(0_i64));

        let forward = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: BTreeMap::from([(String::from("id"), PatchValue::from(local_id))]),
                before: None,
                after: Some(row),
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: Patch::merge_inverses(&forward),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::Opaque(serde_json::json!({
                "kind": "stub_add_comment",
                "local_id": local_id,
                "pr_id": pr_id,
                "body": body,
            })),
        })
    }

    fn apply<'a>(
        &'a self,
        ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            if let Some(force_error) = ctx
                .input_json
                .get("force_error")
                .and_then(serde_json::Value::as_str)
            {
                let error = match force_error {
                    "transient" => MutationApplyError::network("stub transient network failure"),
                    "hard_conflict" => MutationApplyError::conflict(
                        "stub hard conflict",
                        Some(super::HardConflictDiff {
                            summary: "stub conflict diff".to_string(),
                            local_body: Some("local".to_string()),
                            server_body: Some("server".to_string()),
                            changed_fields: vec!["body".to_string()],
                        }),
                    ),
                    "not_found" => MutationApplyError::http(404, "stub not found"),
                    _ => MutationApplyError::http(500, "stub server error"),
                };
                return Err(error.into());
            }

            if ctx
                .input_json
                .get("transient_once")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                let retries = sqlx::query_scalar::<_, i64>(
                    "SELECT retries FROM pending_mutations WHERE id = ?1",
                )
                .bind(ctx.mutation_id)
                .fetch_one(ctx.db.pool())
                .await?;
                if retries == 0 {
                    return Err(MutationApplyError::network("stub transient-once failure").into());
                }
            }

            let local_id = ctx
                .input_json
                .get("local_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("local-comment-stub");
            let pr_id = ctx
                .input_json
                .get("pr_id")
                .and_then(serde_json::Value::as_str)
                .context("add comment stub expects input_json.pr_id")?;
            let author_id = ctx
                .input_json
                .get("author_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("user-stub");
            let predicted_body = ctx
                .input_json
                .get("body")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let server_body = ctx
                .input_json
                .get("server_body")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(predicted_body);
            let server_id = format!("srv-{local_id}");
            let now = super::reconciler::now_epoch_seconds()?;

            Ok(ServerResponse {
                upserts: vec![ServerNode::Comment(CommentRecord {
                    id: server_id.clone(),
                    account_id: ctx.account_id.to_string(),
                    pr_id: pr_id.to_string(),
                    kind: "issue".to_string(),
                    author_id: author_id.to_string(),
                    body: server_body.to_string(),
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                    in_reply_to_id: None,
                    review_id: None,
                    thread_id: None,
                    path: None,
                    line: None,
                    side: None,
                    start_line: None,
                    start_side: None,
                    original_commit_sha: None,
                })],
                markdown_overlays: vec![super::MarkdownOverlay {
                    target: super::MarkdownOverlayTarget::Comment {
                        comment_id: server_id.clone(),
                    },
                    predicted_body: predicted_body.to_string(),
                    server_body: server_body.to_string(),
                }],
                id_mappings: vec![IdMappingDraft {
                    kind: "comment".to_string(),
                    local_id: local_id.to_string(),
                    server_id,
                }],
                refetch_pr_ids: vec![pr_id.to_string()],
                hard_conflict: None,
            })
        })
    }
}

#[derive(Debug)]
struct EditCommentStub;

impl Mutation for EditCommentStub {
    fn kind(&self) -> MutationKind {
        MutationKind::EditComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let comment_id = required_str(ctx.input_json, "comment_id")?;
        let previous = required_str(ctx.input_json, "previous_body")?;
        let next = required_str(ctx.input_json, "next_body")?;
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let author_id = required_str(ctx.input_json, "author_id")?;
        let kind = ctx
            .input_json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("issue");
        let created_at = ctx
            .input_json
            .get("created_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let previous_updated_at = ctx
            .input_json
            .get("previous_updated_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let now = super::reconciler::now_epoch_seconds()?;
        let base_row = BTreeMap::from([
            (String::from("id"), PatchValue::from(comment_id)),
            (String::from("account_id"), PatchValue::from(ctx.account_id)),
            (String::from("pr_id"), PatchValue::from(pr_id)),
            (String::from("kind"), PatchValue::from(kind)),
            (String::from("author_id"), PatchValue::from(author_id)),
            (String::from("created_at"), PatchValue::from(created_at)),
            (
                String::from("body_server_adjusted"),
                PatchValue::from(0_i64),
            ),
        ]);

        let forward = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: BTreeMap::from([(String::from("id"), PatchValue::from(comment_id))]),
                before: Some({
                    let mut row = base_row.clone();
                    row.insert(String::from("body"), PatchValue::from(previous));
                    row.insert(
                        String::from("updated_at"),
                        PatchValue::from(previous_updated_at),
                    );
                    row
                }),
                after: Some({
                    let mut row = base_row;
                    row.insert(String::from("body"), PatchValue::from(next));
                    row.insert(String::from("updated_at"), PatchValue::from(now));
                    row
                }),
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: forward.inverse(),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::None,
        })
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct DeleteCommentStub;

impl Mutation for DeleteCommentStub {
    fn kind(&self) -> MutationKind {
        MutationKind::DeleteComment
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let comment_id = required_str(ctx.input_json, "comment_id")?;
        let body = ctx
            .input_json
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let author_id = required_str(ctx.input_json, "author_id")?;
        let kind = ctx
            .input_json
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("issue");
        let created_at = ctx
            .input_json
            .get("created_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let previous_updated_at = ctx
            .input_json
            .get("previous_updated_at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(1);
        let forward = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: BTreeMap::from([(String::from("id"), PatchValue::from(comment_id))]),
                before: Some(BTreeMap::from([
                    (String::from("id"), PatchValue::from(comment_id)),
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("kind"), PatchValue::from(kind)),
                    (String::from("author_id"), PatchValue::from(author_id)),
                    (String::from("body"), PatchValue::from(body)),
                    (String::from("created_at"), PatchValue::from(created_at)),
                    (
                        String::from("updated_at"),
                        PatchValue::from(previous_updated_at),
                    ),
                    (
                        String::from("body_server_adjusted"),
                        PatchValue::from(0_i64),
                    ),
                ])),
                after: None,
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: forward.inverse(),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::None,
        })
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct AddReactionStub;

impl Mutation for AddReactionStub {
    fn kind(&self) -> MutationKind {
        MutationKind::AddReaction
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        make_touch_comment_effect(ctx, MutationKind::AddReaction)
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct RemoveReactionStub;

impl Mutation for RemoveReactionStub {
    fn kind(&self) -> MutationKind {
        MutationKind::RemoveReaction
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        make_touch_comment_effect(ctx, MutationKind::RemoveReaction)
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct AddLabelStub;

impl Mutation for AddLabelStub {
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

        let forward = Patch {
            operations: vec![RowMutation {
                table: "pr_labels".to_string(),
                pk: BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("label_name"), PatchValue::from(label_name)),
                ]),
                before: None,
                after: Some(BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("label_name"), PatchValue::from(label_name)),
                    (String::from("label_color"), PatchValue::from(label_color)),
                ])),
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: forward.inverse(),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::None,
        })
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct RemoveLabelStub;

impl Mutation for RemoveLabelStub {
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

        let forward = Patch {
            operations: vec![RowMutation {
                table: "pr_labels".to_string(),
                pk: BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("label_name"), PatchValue::from(label_name)),
                ]),
                before: Some(BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("label_name"), PatchValue::from(label_name)),
                    (String::from("label_color"), PatchValue::from(label_color)),
                ])),
                after: None,
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: forward.inverse(),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::None,
        })
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

#[derive(Debug)]
struct SetAssigneesStub;

impl Mutation for SetAssigneesStub {
    fn kind(&self) -> MutationKind {
        MutationKind::SetAssignees
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::Full
    }

    fn predict(&self, ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        let pr_id = required_str(ctx.input_json, "pr_id")?;
        let user_id = required_str(ctx.input_json, "user_id")?;
        let now = super::reconciler::now_epoch_seconds()?;

        let forward = Patch {
            operations: vec![RowMutation {
                table: "pr_assignees".to_string(),
                pk: BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("user_id"), PatchValue::from(user_id)),
                ]),
                before: None,
                after: Some(BTreeMap::from([
                    (String::from("account_id"), PatchValue::from(ctx.account_id)),
                    (String::from("pr_id"), PatchValue::from(pr_id)),
                    (String::from("user_id"), PatchValue::from(user_id)),
                    (String::from("assigned_at"), PatchValue::from(now)),
                ])),
            }],
            pending_overlay_kind: None,
        };

        Ok(PredictedEffect {
            inverse_patch: forward.inverse(),
            forward_patch: forward,
            id_mappings: Vec::new(),
            server_call: ServerCallShape::None,
        })
    }

    fn apply<'a>(
        &'a self,
        _ctx: &'a ApplyCtx<'_>,
    ) -> super::BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async { Ok(ServerResponse::empty()) })
    }
}

fn make_touch_comment_effect(ctx: &PredictCtx<'_>, kind: MutationKind) -> Result<PredictedEffect> {
    let comment_id = required_str(ctx.input_json, "comment_id")?;
    let pr_id = required_str(ctx.input_json, "pr_id")?;
    let author_id = required_str(ctx.input_json, "author_id")?;
    let comment_kind = ctx
        .input_json
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("issue");
    let body = ctx
        .input_json
        .get("body")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("seed-react");
    let created_at = ctx
        .input_json
        .get("created_at")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(1);
    let previous_updated_at = ctx
        .input_json
        .get("previous_updated_at")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let now = super::reconciler::now_epoch_seconds()?;

    let forward = Patch {
        operations: vec![RowMutation {
            table: "comments".to_string(),
            pk: BTreeMap::from([(String::from("id"), PatchValue::from(comment_id))]),
            before: Some(BTreeMap::from([
                (String::from("id"), PatchValue::from(comment_id)),
                (String::from("account_id"), PatchValue::from(ctx.account_id)),
                (String::from("pr_id"), PatchValue::from(pr_id)),
                (String::from("kind"), PatchValue::from(comment_kind)),
                (String::from("author_id"), PatchValue::from(author_id)),
                (String::from("body"), PatchValue::from(body)),
                (String::from("created_at"), PatchValue::from(created_at)),
                (
                    String::from("updated_at"),
                    PatchValue::from(previous_updated_at),
                ),
                (
                    String::from("body_server_adjusted"),
                    PatchValue::from(0_i64),
                ),
            ])),
            after: Some(BTreeMap::from([
                (String::from("id"), PatchValue::from(comment_id)),
                (String::from("account_id"), PatchValue::from(ctx.account_id)),
                (String::from("pr_id"), PatchValue::from(pr_id)),
                (String::from("kind"), PatchValue::from(comment_kind)),
                (String::from("author_id"), PatchValue::from(author_id)),
                (String::from("body"), PatchValue::from(body)),
                (String::from("created_at"), PatchValue::from(created_at)),
                (String::from("updated_at"), PatchValue::from(now)),
                (
                    String::from("body_server_adjusted"),
                    PatchValue::from(0_i64),
                ),
            ])),
        }],
        pending_overlay_kind: None,
    };

    Ok(PredictedEffect {
        inverse_patch: forward.inverse(),
        forward_patch: forward,
        id_mappings: Vec::new(),
        server_call: ServerCallShape::Opaque(serde_json::json!({
            "kind": kind.as_str(),
            "comment_id": comment_id,
        })),
    })
}

fn required_str<'a>(value: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .with_context(|| format!("stub handler expects input_json.{key}"))
}

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use reqwest::Method;
use serde_json::json;

use crate::db::{SuggestionApplyRecord, SuggestionBlockViewRow};
use crate::mutations::engine::MutationApplyError;
use crate::mutations::{
    ApplyCtx, BoxMutationFuture, ErrorKind, HardConflictDiff, Mutation, MutationKind,
    OptimismLevel, PredictCtx, PredictedEffect, ServerCallShape, ServerResponse,
};
use crate::worktree::write::{
    AuthorIdentity, DiffSide, Git2WorktreeWriter, SuggestionPatch, WorktreeWriteError,
    WorktreeWriteRequest, WorktreeWriter,
};

use super::common::{no_op_effect, now_epoch_seconds, required_i64, required_str, string_vec};

const HEAD_BRANCH_ADVANCED_MESSAGE: &str =
    "Head branch advanced — suggestion no longer applies, please re-comment";

#[derive(Debug)]
pub struct ApplySuggestion;

#[derive(Debug)]
pub struct ApplySuggestionBatch;

impl Mutation for ApplySuggestion {
    fn kind(&self) -> MutationKind {
        MutationKind::ApplySuggestion
    }

    fn optimism(&self) -> OptimismLevel {
        OptimismLevel::None
    }

    fn predict(&self, _ctx: &PredictCtx<'_>) -> Result<PredictedEffect> {
        Ok(no_op_effect(ServerCallShape::None))
    }

    fn apply<'a>(&'a self, ctx: &'a ApplyCtx<'_>) -> BoxMutationFuture<'a, Result<ServerResponse>> {
        Box::pin(async move {
            let owner = required_str(ctx.input_json, "owner")?;
            let repo = required_str(ctx.input_json, "repo")?;
            let pr_number = required_i64(ctx.input_json, "pr_number")?;
            let pr_id = required_str(ctx.input_json, "pr_id")?;
            let review_comment_id = required_str(ctx.input_json, "review_comment_id")?;
            let expected_head_sha = required_str(ctx.input_json, "expected_head_sha")?;

            let apply_outcome = apply_single_suggestion(
                ctx,
                owner,
                repo,
                pr_number,
                review_comment_id,
                expected_head_sha,
            )
            .await;

            match apply_outcome {
                Ok(commit_sha) => {
                    persist_apply_audit(
                        ctx,
                        SuggestionApplyRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            mutation_id: ctx.mutation_id.to_string(),
                            mode: "single".to_string(),
                            commit_sha,
                            head_sha_before: Some(expected_head_sha.to_string()),
                            head_sha_after: None,
                            suggestion_comment_ids: serde_json::to_string(&vec![
                                review_comment_id,
                            ])?,
                            applied_at: now_epoch_seconds()?,
                            outcome: "applied".to_string(),
                            error_kind: None,
                        },
                    )
                    .await?;
                    Ok(ServerResponse {
                        upserts: Vec::new(),
                        markdown_overlays: Vec::new(),
                        id_mappings: Vec::new(),
                        refetch_pr_ids: vec![pr_id.to_string()],
                        hard_conflict: None,
                    })
                }
                Err(error) => {
                    let status = extract_http_status(&error.to_string());
                    let mapped = map_single_apply_error(error);
                    let outcome = if matches!(mapped.kind, ErrorKind::Conflict) {
                        "conflict"
                    } else {
                        "rejected"
                    };
                    persist_apply_audit(
                        ctx,
                        SuggestionApplyRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            mutation_id: ctx.mutation_id.to_string(),
                            mode: "single".to_string(),
                            commit_sha: None,
                            head_sha_before: Some(expected_head_sha.to_string()),
                            head_sha_after: None,
                            suggestion_comment_ids: serde_json::to_string(&vec![
                                review_comment_id,
                            ])?,
                            applied_at: now_epoch_seconds()?,
                            outcome: outcome.to_string(),
                            error_kind: status.map(|code| format!("http_{code}")),
                        },
                    )
                    .await?;
                    Err(mapped.into())
                }
            }
        })
    }
}

impl Mutation for ApplySuggestionBatch {
    fn kind(&self) -> MutationKind {
        MutationKind::ApplySuggestionBatch
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
            let expected_head_sha = required_str(ctx.input_json, "expected_head_sha")?;
            let worktree_path = required_str(ctx.input_json, "worktree_path")?;
            let suggestion_ids = string_vec(ctx.input_json, "suggestion_ids")?;
            let force_with_stash = ctx
                .input_json
                .get("force_with_stash")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let summary = ctx
                .db
                .pr_detail_summary(ctx.account_id, pr_id)
                .await?
                .ok_or_else(|| anyhow!("pull request `{pr_id}` not found"))?;
            if summary.head_sha != expected_head_sha {
                let conflict = MutationApplyError::conflict(
                    HEAD_BRANCH_ADVANCED_MESSAGE,
                    Some(head_advanced_diff()),
                );
                persist_apply_audit(
                    ctx,
                    SuggestionApplyRecord {
                        account_id: ctx.account_id.to_string(),
                        pr_id: pr_id.to_string(),
                        mutation_id: ctx.mutation_id.to_string(),
                        mode: "batch".to_string(),
                        commit_sha: None,
                        head_sha_before: Some(expected_head_sha.to_string()),
                        head_sha_after: Some(summary.head_sha),
                        suggestion_comment_ids: serde_json::to_string(&suggestion_ids)?,
                        applied_at: now_epoch_seconds()?,
                        outcome: "conflict".to_string(),
                        error_kind: Some("head_mismatch".to_string()),
                    },
                )
                .await?;
                return Err(conflict.into());
            }

            let suggestions = resolve_suggestions(ctx, pr_id, &suggestion_ids).await?;
            let endpoints = ctx.github.resolve_account_endpoints(ctx.account_id).await?;
            let author_login = ctx
                .input_json
                .get("author_login")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(endpoints.locator.login.as_str())
                .to_string();
            let author_name = ctx
                .input_json
                .get("author_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(author_login.as_str())
                .to_string();
            let author_email = ctx
                .input_json
                .get("author_email")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("{author_login}@users.noreply.github.com"));
            let commit_message = ctx
                .input_json
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Apply {} suggestion(s)", suggestions.len()));

            let request = WorktreeWriteRequest {
                pr_id: pr_id.to_string(),
                worktree_path: PathBuf::from(worktree_path),
                expected_branch: summary.head_ref,
                expected_head_sha: expected_head_sha.to_string(),
                suggestions,
                author: AuthorIdentity {
                    login: author_login,
                    name: author_name,
                    email: author_email,
                    push_token: if endpoints.token.is_empty() {
                        None
                    } else {
                        Some(endpoints.token)
                    },
                    remote_name: None,
                },
                message: commit_message,
                force_with_stash,
            };

            let writer = Git2WorktreeWriter;
            let write_outcome = writer.run(request);
            match write_outcome {
                Ok(outcome) => {
                    persist_apply_audit(
                        ctx,
                        SuggestionApplyRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            mutation_id: ctx.mutation_id.to_string(),
                            mode: "batch".to_string(),
                            commit_sha: Some(outcome.commit_sha.clone()),
                            head_sha_before: Some(outcome.head_sha_before.clone()),
                            head_sha_after: Some(outcome.head_sha_after.clone()),
                            suggestion_comment_ids: serde_json::to_string(
                                &collect_comment_ids_from_block_ids(&suggestion_ids),
                            )?,
                            applied_at: now_epoch_seconds()?,
                            outcome: "applied".to_string(),
                            error_kind: None,
                        },
                    )
                    .await?;
                    Ok(ServerResponse {
                        upserts: Vec::new(),
                        markdown_overlays: Vec::new(),
                        id_mappings: Vec::new(),
                        refetch_pr_ids: vec![pr_id.to_string()],
                        hard_conflict: None,
                    })
                }
                Err(error) => {
                    let mapped = map_batch_apply_error(&error);
                    let outcome = match error {
                        WorktreeWriteError::WorktreeDirty { .. } => "aborted",
                        WorktreeWriteError::SuggestionConflict { .. }
                        | WorktreeWriteError::HeadMismatch { .. }
                        | WorktreeWriteError::BranchMismatch { .. } => "conflict",
                        WorktreeWriteError::PushRejected { .. } => "rejected",
                        _ => "rejected",
                    };
                    persist_apply_audit(
                        ctx,
                        SuggestionApplyRecord {
                            account_id: ctx.account_id.to_string(),
                            pr_id: pr_id.to_string(),
                            mutation_id: ctx.mutation_id.to_string(),
                            mode: "batch".to_string(),
                            commit_sha: None,
                            head_sha_before: Some(expected_head_sha.to_string()),
                            head_sha_after: None,
                            suggestion_comment_ids: serde_json::to_string(
                                &collect_comment_ids_from_block_ids(&suggestion_ids),
                            )?,
                            applied_at: now_epoch_seconds()?,
                            outcome: outcome.to_string(),
                            error_kind: Some(worktree_error_kind(&error)),
                        },
                    )
                    .await?;
                    Err(mapped.into())
                }
            }
        })
    }
}

fn collect_comment_ids_from_block_ids(suggestion_ids: &[String]) -> Vec<String> {
    suggestion_ids
        .iter()
        .map(|id| id.split(':').next().unwrap_or(id.as_str()).to_string())
        .collect()
}

async fn apply_single_suggestion(
    ctx: &ApplyCtx<'_>,
    owner: &str,
    repo: &str,
    pr_number: i64,
    review_comment_id: &str,
    expected_head_sha: &str,
) -> Result<Option<String>> {
    let primary_path = format!("/repos/{owner}/{repo}/pulls/comments/{review_comment_id}");
    let primary_result = ctx
        .github
        .rest_mutation_json::<serde_json::Value>(
            ctx.account_id,
            Method::PUT,
            &primary_path,
            Some(json!({
                "operation": "apply_suggestion",
                "expected_head_sha": expected_head_sha,
            })),
            Some(ctx.idempotency_key),
        )
        .await;

    match primary_result {
        Ok((payload, _rate_limit)) => {
            return Ok(payload
                .as_ref()
                .and_then(|value| value.get("commit_sha"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string));
        }
        Err(error) => {
            let message = error.to_string().to_ascii_lowercase();
            let status = extract_http_status(&error.to_string());
            let should_fallback = status == Some(404) || message.contains("not implemented");
            if !should_fallback {
                return Err(error);
            }
        }
    }

    let fallback_path = format!("/repos/{owner}/{repo}/pulls/{pr_number}/reviews");
    let (payload, _rate_limit) = ctx
        .github
        .rest_mutation_json::<serde_json::Value>(
            ctx.account_id,
            Method::POST,
            &fallback_path,
            Some(json!({
                "event": "COMMENT",
                "commit_id": expected_head_sha,
                "comments": [{
                    "in_reply_to": review_comment_id,
                    "body": "Applied suggested change from PR Cockpit"
                }]
            })),
            Some(ctx.idempotency_key),
        )
        .await?;
    Ok(payload
        .as_ref()
        .and_then(|value| value.get("commit_id"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string))
}

fn worktree_error_kind(error: &WorktreeWriteError) -> String {
    match error {
        WorktreeWriteError::OpenRepo { .. } => "open_repo",
        WorktreeWriteError::BranchMismatch { .. } => "branch_mismatch",
        WorktreeWriteError::HeadMismatch { .. } => "head_mismatch",
        WorktreeWriteError::WorktreeDirty { .. } => "worktree_dirty",
        WorktreeWriteError::StashFailed { .. } => "stash_failed",
        WorktreeWriteError::StashPopFailed { .. } => "stash_pop_failed",
        WorktreeWriteError::InvalidRange { .. } => "invalid_range",
        WorktreeWriteError::ReadFailed { .. } => "read_failed",
        WorktreeWriteError::WriteFailed { .. } => "write_failed",
        WorktreeWriteError::SuggestionConflict { .. } => "suggestion_conflict",
        WorktreeWriteError::NoSuggestions => "no_suggestions",
        WorktreeWriteError::StageFailed { .. } => "stage_failed",
        WorktreeWriteError::CommitFailed { .. } => "commit_failed",
        WorktreeWriteError::RemoteNotFound { .. } => "remote_not_found",
        WorktreeWriteError::PushRejected { .. } => "push_rejected",
        WorktreeWriteError::PushFailed { .. } => "push_failed",
        WorktreeWriteError::RollbackFailed { .. } => "rollback_failed",
    }
    .to_string()
}

fn map_single_apply_error(error: anyhow::Error) -> MutationApplyError {
    let message = error.to_string();
    let status = extract_http_status(&message);
    if status == Some(409) {
        return MutationApplyError::conflict(
            HEAD_BRANCH_ADVANCED_MESSAGE,
            Some(head_advanced_diff()),
        );
    }
    if let Some(code) = status {
        return MutationApplyError::http(code, message);
    }
    MutationApplyError {
        kind: ErrorKind::Server,
        retryable: false,
        http_status: None,
        hard_conflict: None,
        message,
    }
}

fn map_batch_apply_error(error: &WorktreeWriteError) -> MutationApplyError {
    match error {
        WorktreeWriteError::HeadMismatch { .. } | WorktreeWriteError::BranchMismatch { .. } => {
            MutationApplyError::conflict(HEAD_BRANCH_ADVANCED_MESSAGE, Some(head_advanced_diff()))
        }
        WorktreeWriteError::SuggestionConflict { message, .. } => {
            MutationApplyError::conflict(message.clone(), None)
        }
        WorktreeWriteError::WorktreeDirty { .. } => MutationApplyError::conflict(
            "Worktree is dirty — commit or stash your changes first, or enable force with stash",
            None,
        ),
        WorktreeWriteError::PushRejected { .. } => MutationApplyError::conflict(
            "Remote branch advanced during push — refresh and try again",
            Some(head_advanced_diff()),
        ),
        _ => MutationApplyError {
            kind: ErrorKind::Server,
            retryable: false,
            http_status: None,
            hard_conflict: None,
            message: error.to_string(),
        },
    }
}

fn head_advanced_diff() -> HardConflictDiff {
    HardConflictDiff {
        summary: HEAD_BRANCH_ADVANCED_MESSAGE.to_string(),
        local_body: None,
        server_body: None,
        changed_fields: vec!["head_sha".to_string()],
    }
}

fn extract_http_status(message: &str) -> Option<i64> {
    let start = message.find('(')?;
    let end = message[start + 1..].find(')')?;
    message[start + 1..start + 1 + end].parse::<i64>().ok()
}

async fn resolve_suggestions(
    ctx: &ApplyCtx<'_>,
    pr_id: &str,
    suggestion_ids: &[String],
) -> Result<Vec<SuggestionPatch>> {
    if suggestion_ids.is_empty() {
        return Err(anyhow!("no suggestion ids supplied"));
    }
    let blocks = ctx.db.list_suggestion_blocks(ctx.account_id, pr_id).await?;
    let lookup: HashMap<String, SuggestionBlockViewRow> = blocks
        .into_iter()
        .map(|block| (block.id.clone(), block))
        .collect();
    let mut out = Vec::with_capacity(suggestion_ids.len());
    for suggestion_id in suggestion_ids {
        let block = lookup.get(suggestion_id).ok_or_else(|| {
            anyhow!("suggestion block `{suggestion_id}` not found for pull request `{pr_id}`")
        })?;
        out.push(SuggestionPatch {
            id: block.id.clone(),
            path: PathBuf::from(&block.path),
            start_line: u32::try_from(block.start_line).unwrap_or(0),
            end_line: u32::try_from(block.end_line).unwrap_or(0),
            side: if block.side.eq_ignore_ascii_case("LEFT") {
                DiffSide::Left
            } else {
                DiffSide::Right
            },
            replacement: block.body.clone(),
            original: None,
            original_commit_sha: block.original_commit_sha.clone(),
            suggestion_author_login: block.suggestion_author_login.clone(),
        });
    }
    Ok(out)
}

async fn persist_apply_audit(ctx: &ApplyCtx<'_>, row: SuggestionApplyRecord) -> Result<()> {
    ctx.db.insert_suggestion_apply(&row).await?;
    Ok(())
}

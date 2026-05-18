use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::Row;
use tokio::process::Command;

use crate::api::{GithubClient, UPDATE_PULL_REQUEST_BASE_MUTATION};
use crate::db::Db;
use crate::mutations::handlers::merge_controls::Merge;
use crate::mutations::{ApplyCtx, Mutation, MutationEngine, ReconcileCtx, ServerCallShape};
use crate::stacks::graphite::{detect_graphite, run_gt_restack, run_gt_submit};
use crate::stacks::now_epoch_seconds;

pub type StackOperationId = String;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StackOperationKind {
    Rebase,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StackOperationStatus {
    Pending,
    Running,
    PausedConflict,
    PausedFailure,
    Succeeded,
    Aborted,
}

impl StackOperationStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::PausedConflict => "paused_conflict",
            Self::PausedFailure => "paused_failure",
            Self::Succeeded => "succeeded",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct StackOperationView {
    pub id: String,
    pub stack_id: String,
    pub account_id: String,
    pub op_kind: StackOperationKind,
    pub status: StackOperationStatus,
    pub current_pr_id: Option<String>,
    pub current_step: Option<i64>,
    pub total_steps: Option<i64>,
    pub worktree_path: Option<String>,
    pub conflict_files: Vec<String>,
    pub last_error: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone)]
struct StackStep {
    pr_id: String,
    _position: i64,
    parent_pr_id: Option<String>,
    pr_number: i64,
    title: String,
    base_ref: String,
    head_ref: String,
    head_sha: String,
    base_sha: String,
    repo_id: String,
    repo_owner: String,
    repo_name: String,
}

#[derive(Debug, Clone)]
struct StackPlan {
    _stack_id: String,
    _account_id: String,
    repo_id: String,
    _repo_owner: String,
    _repo_name: String,
    steps: Vec<StackStep>,
}

#[derive(Debug, Clone)]
pub struct GitCommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait GitOps: Send + Sync {
    async fn fetch(&self, worktree_path: &Path) -> Result<GitCommandResult>;
    async fn checkout(&self, worktree_path: &Path, branch: &str) -> Result<GitCommandResult>;
    async fn rebase(&self, worktree_path: &Path, target: &str) -> Result<GitCommandResult>;
    async fn rebase_continue(&self, worktree_path: &Path) -> Result<GitCommandResult>;
    async fn rebase_abort(&self, worktree_path: &Path) -> Result<GitCommandResult>;
    async fn push_force_with_lease(
        &self,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<GitCommandResult>;
    async fn status_porcelain_v2(&self, worktree_path: &Path) -> Result<GitCommandResult>;
}

#[derive(Debug, Default, Clone)]
pub struct CommandGitOps;

#[async_trait]
impl GitOps for CommandGitOps {
    async fn fetch(&self, worktree_path: &Path) -> Result<GitCommandResult> {
        run_git(worktree_path, &["fetch", "--all", "--prune"]).await
    }

    async fn checkout(&self, worktree_path: &Path, branch: &str) -> Result<GitCommandResult> {
        run_git(worktree_path, &["checkout", branch]).await
    }

    async fn rebase(&self, worktree_path: &Path, target: &str) -> Result<GitCommandResult> {
        run_git(worktree_path, &["rebase", target]).await
    }

    async fn rebase_continue(&self, worktree_path: &Path) -> Result<GitCommandResult> {
        run_git(worktree_path, &["rebase", "--continue"]).await
    }

    async fn rebase_abort(&self, worktree_path: &Path) -> Result<GitCommandResult> {
        run_git(worktree_path, &["rebase", "--abort"]).await
    }

    async fn push_force_with_lease(
        &self,
        worktree_path: &Path,
        branch: &str,
    ) -> Result<GitCommandResult> {
        run_git(
            worktree_path,
            &["push", "--force-with-lease", "origin", branch],
        )
        .await
    }

    async fn status_porcelain_v2(&self, worktree_path: &Path) -> Result<GitCommandResult> {
        run_git(worktree_path, &["status", "--porcelain=v2"]).await
    }
}

pub trait StackOperationEmitter: Send + Sync {
    fn emit_stack_op(&self, op_id: &str, payload: &StackOperationView);
}

static STACK_OP_EMITTER: OnceCell<Arc<dyn StackOperationEmitter>> = OnceCell::new();

pub fn install_stack_op_event_emitter(emitter: Arc<dyn StackOperationEmitter>) {
    let _ = STACK_OP_EMITTER.set(emitter);
}

fn emit_stack_op(payload: &StackOperationView) {
    if let Some(emitter) = STACK_OP_EMITTER.get() {
        emitter.emit_stack_op(&payload.id, payload);
    }
}

pub async fn get_stack_op(db: &Db, op_id: &str) -> Result<Option<StackOperationView>> {
    let row = sqlx::query(
        "SELECT
            id, stack_id, account_id, op_kind, status, current_pr_id, current_step, total_steps,
            worktree_path, conflict_files_json, last_error, started_at, updated_at, finished_at
         FROM stack_operations
         WHERE id = ?1
         LIMIT 1",
    )
    .bind(op_id)
    .fetch_optional(db.pool())
    .await?;

    row.map(map_stack_op_row).transpose()
}

pub async fn rebase_stack(
    db: &Db,
    git: &dyn GitOps,
    account_id: &str,
    stack_id: &str,
) -> Result<StackOperationId> {
    let plan = load_stack_plan(db, account_id, stack_id).await?;
    if plan.steps.is_empty() {
        return Err(anyhow!("stack `{stack_id}` has no pull requests"));
    }
    let worktree_path = pick_worktree_for_repo(db, account_id, &plan.repo_id)
        .await?
        .ok_or_else(|| anyhow!("NoWorktree"))?;
    let op_id = format!(
        "stack-op-{}-{}",
        now_epoch_millis()?,
        stack_id.replace(':', "-")
    );
    let now = now_epoch_seconds()?;
    sqlx::query(
        "INSERT INTO stack_operations(
            id, stack_id, account_id, op_kind, status, current_pr_id, current_step, total_steps,
            worktree_path, conflict_files_json, last_error, started_at, updated_at, finished_at
         ) VALUES (?1, ?2, ?3, 'rebase', 'pending', NULL, NULL, ?4, ?5, NULL, NULL, ?6, ?6, NULL)",
    )
    .bind(&op_id)
    .bind(stack_id)
    .bind(account_id)
    .bind(i64::try_from(plan.steps.len()).unwrap_or(i64::MAX))
    .bind(worktree_path.to_string_lossy().to_string())
    .bind(now)
    .execute(db.pool())
    .await?;

    update_operation_state(
        db,
        &op_id,
        StackOperationStatus::Running,
        None,
        Some(0),
        None,
        None,
        Some(worktree_path.to_string_lossy().to_string()),
        None,
    )
    .await?;

    let graphite_enabled = read_bool_setting(db, "graphite_enabled", false).await?;
    if graphite_enabled && detect_graphite().is_some() {
        let graphite_outcome = run_gt_restack(&worktree_path).await;
        match graphite_outcome {
            Ok(output) if output.status.success() => {
                update_operation_state(
                    db,
                    &op_id,
                    StackOperationStatus::Succeeded,
                    None,
                    i64::try_from(plan.steps.len()).ok(),
                    None,
                    None,
                    Some(worktree_path.to_string_lossy().to_string()),
                    Some(now_epoch_seconds()?),
                )
                .await?;
                return Ok(op_id);
            }
            Ok(output) => {
                let warning = format!(
                    "Graphite failed; using plain git: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                update_operation_state(
                    db,
                    &op_id,
                    StackOperationStatus::Running,
                    None,
                    Some(0),
                    Some(warning),
                    None,
                    Some(worktree_path.to_string_lossy().to_string()),
                    None,
                )
                .await?;
            }
            Err(error) => {
                update_operation_state(
                    db,
                    &op_id,
                    StackOperationStatus::Running,
                    None,
                    Some(0),
                    Some(format!("Graphite failed; using plain git: {error}")),
                    None,
                    Some(worktree_path.to_string_lossy().to_string()),
                    None,
                )
                .await?;
            }
        }
    }

    let force_push_with_lease = read_bool_setting(db, "force_push_with_lease", false).await?;
    let fetch_result = git.fetch(&worktree_path).await?;
    if !fetch_result.success {
        pause_failure(
            db,
            &op_id,
            "git fetch failed".to_string(),
            &worktree_path,
            None,
            Some(0),
        )
        .await?;
        return Ok(op_id);
    }

    let rebase_outcome = execute_rebase_plan(
        db,
        git,
        &op_id,
        &plan,
        &worktree_path,
        force_push_with_lease,
        0,
    )
    .await?;

    if rebase_outcome == RebaseExecutionOutcome::Completed {
        update_operation_state(
            db,
            &op_id,
            StackOperationStatus::Succeeded,
            plan.steps.last().map(|step| step.pr_id.clone()),
            i64::try_from(plan.steps.len()).ok(),
            None,
            None,
            Some(worktree_path.to_string_lossy().to_string()),
            Some(now_epoch_seconds()?),
        )
        .await?;
    }

    Ok(op_id)
}

pub async fn merge_stack(
    db: &Db,
    _git: &dyn GitOps,
    github: &GithubClient,
    _mutations: &MutationEngine,
    account_id: &str,
    stack_id: &str,
    method: MergeMethod,
) -> Result<StackOperationId> {
    let plan = load_stack_plan(db, account_id, stack_id).await?;
    if plan.steps.is_empty() {
        return Err(anyhow!("stack `{stack_id}` has no pull requests"));
    }
    let worktree_path = pick_worktree_for_repo(db, account_id, &plan.repo_id)
        .await?
        .map(|path| path.to_string_lossy().to_string());
    let op_id = format!(
        "stack-op-{}-{}",
        now_epoch_millis()?,
        stack_id.replace(':', "-")
    );
    let now = now_epoch_seconds()?;
    sqlx::query(
        "INSERT INTO stack_operations(
            id, stack_id, account_id, op_kind, status, current_pr_id, current_step, total_steps,
            worktree_path, conflict_files_json, last_error, started_at, updated_at, finished_at
         ) VALUES (?1, ?2, ?3, 'merge', 'pending', NULL, NULL, ?4, ?5, NULL, NULL, ?6, ?6, NULL)",
    )
    .bind(&op_id)
    .bind(stack_id)
    .bind(account_id)
    .bind(i64::try_from(plan.steps.len()).unwrap_or(i64::MAX))
    .bind(worktree_path.clone())
    .bind(now)
    .execute(db.pool())
    .await?;

    update_operation_state(
        db,
        &op_id,
        StackOperationStatus::Running,
        None,
        Some(0),
        None,
        None,
        worktree_path.clone(),
        None,
    )
    .await?;

    if read_bool_setting(db, "graphite_enabled", false).await? && detect_graphite().is_some() {
        if let Some(path) = worktree_path.as_deref() {
            let output = run_gt_submit(Path::new(path)).await;
            match output {
                Ok(value) if value.status.success() => {
                    update_operation_state(
                        db,
                        &op_id,
                        StackOperationStatus::Succeeded,
                        None,
                        i64::try_from(plan.steps.len()).ok(),
                        None,
                        None,
                        worktree_path.clone(),
                        Some(now_epoch_seconds()?),
                    )
                    .await?;
                    return Ok(op_id);
                }
                Ok(value) => {
                    update_operation_state(
                        db,
                        &op_id,
                        StackOperationStatus::Running,
                        None,
                        Some(0),
                        Some(format!(
                            "Graphite failed; using plain git: {}",
                            String::from_utf8_lossy(&value.stderr).trim()
                        )),
                        None,
                        worktree_path.clone(),
                        None,
                    )
                    .await?;
                }
                Err(error) => {
                    update_operation_state(
                        db,
                        &op_id,
                        StackOperationStatus::Running,
                        None,
                        Some(0),
                        Some(format!("Graphite failed; using plain git: {error}")),
                        None,
                        worktree_path.clone(),
                        None,
                    )
                    .await?;
                }
            }
        }
    }

    for (index, step) in plan.steps.iter().enumerate() {
        let current_step = i64::try_from(index + 1).unwrap_or(i64::MAX);
        update_operation_state(
            db,
            &op_id,
            StackOperationStatus::Running,
            Some(step.pr_id.clone()),
            Some(current_step),
            None,
            None,
            worktree_path.clone(),
            None,
        )
        .await?;

        if let Err(error) =
            execute_merge_with_handler(db, github, account_id, stack_id, step, method).await
        {
            pause_failure(
                db,
                &op_id,
                format!("merge failed for #{}: {error}", step.pr_number),
                Path::new(worktree_path.as_deref().unwrap_or_default()),
                Some(step.pr_id.clone()),
                Some(current_step),
            )
            .await?;
            return Ok(op_id);
        }

        if let Some(next) = plan.steps.get(index + 1) {
            let variables = serde_json::json!({
                "pullRequestId": next.pr_id,
                "baseRefName": step.base_ref
            });
            let mutation_result = github
                .graphql_mutation::<serde_json::Value>(
                    account_id,
                    UPDATE_PULL_REQUEST_BASE_MUTATION,
                    variables,
                    Some(&format!("stack-retarget:{stack_id}:{}", next.pr_id)),
                )
                .await;
            if let Err(error) = mutation_result {
                pause_failure(
                    db,
                    &op_id,
                    format!("updatePullRequest failed: {error}"),
                    Path::new(worktree_path.as_deref().unwrap_or_default()),
                    Some(next.pr_id.clone()),
                    Some(current_step),
                )
                .await?;
                return Ok(op_id);
            }
        }
    }

    update_operation_state(
        db,
        &op_id,
        StackOperationStatus::Succeeded,
        plan.steps.last().map(|step| step.pr_id.clone()),
        i64::try_from(plan.steps.len()).ok(),
        None,
        None,
        worktree_path,
        Some(now_epoch_seconds()?),
    )
    .await?;
    Ok(op_id)
}

pub async fn resume_stack_op(db: &Db, op_id: &str) -> Result<()> {
    let Some(operation) = get_stack_op(db, op_id).await? else {
        return Err(anyhow!("stack operation `{op_id}` not found"));
    };
    match operation.op_kind {
        StackOperationKind::Rebase => {
            let path = operation
                .worktree_path
                .as_ref()
                .ok_or_else(|| anyhow!("missing worktree path for rebase resume"))?;
            let git = CommandGitOps;
            let plan = load_stack_plan(db, &operation.account_id, &operation.stack_id).await?;
            if plan.steps.is_empty() {
                return Err(anyhow!(
                    "stack `{}` has no pull requests",
                    operation.stack_id
                ));
            }
            let resumed_step = operation.current_step.unwrap_or(1).max(1);
            let resumed_index = usize::try_from(resumed_step - 1).unwrap_or(0);
            let Some(current) = plan.steps.get(resumed_index) else {
                return Err(anyhow!("stack operation step out of range"));
            };
            let continue_result = git.rebase_continue(Path::new(path)).await?;
            if !continue_result.success {
                if is_conflict_output(&continue_result) {
                    pause_conflict_from_path(
                        db,
                        &git,
                        op_id,
                        Path::new(path),
                        operation.current_pr_id.clone(),
                        operation.current_step,
                    )
                    .await?;
                    return Ok(());
                }
                pause_failure(
                    db,
                    op_id,
                    format!(
                        "git rebase --continue failed: {}",
                        continue_result.stderr.trim()
                    ),
                    Path::new(path),
                    operation.current_pr_id.clone(),
                    operation.current_step,
                )
                .await?;
                return Ok(());
            }
            let force_push_with_lease =
                read_bool_setting(db, "force_push_with_lease", false).await?;
            if force_push_with_lease {
                let push_result = git
                    .push_force_with_lease(Path::new(path), &current.head_ref)
                    .await?;
                if !push_result.success {
                    pause_failure(
                        db,
                        op_id,
                        format!("push failed: {}", push_result.stderr.trim()),
                        Path::new(path),
                        operation.current_pr_id.clone(),
                        operation.current_step,
                    )
                    .await?;
                    return Ok(());
                }
            } else {
                pause_failure(
                    db,
                    op_id,
                    "force_push_with_lease disabled; push manually and resume".to_string(),
                    Path::new(path),
                    operation.current_pr_id.clone(),
                    operation.current_step,
                )
                .await?;
                return Ok(());
            }
            let rebase_outcome = execute_rebase_plan(
                db,
                &git,
                op_id,
                &plan,
                Path::new(path),
                force_push_with_lease,
                resumed_index + 1,
            )
            .await?;
            if rebase_outcome == RebaseExecutionOutcome::Completed {
                update_operation_state(
                    db,
                    op_id,
                    StackOperationStatus::Succeeded,
                    operation.current_pr_id,
                    operation.current_step,
                    None,
                    None,
                    Some(path.clone()),
                    Some(now_epoch_seconds()?),
                )
                .await?;
            }
        }
        StackOperationKind::Merge => {
            update_operation_state(
                db,
                op_id,
                StackOperationStatus::Running,
                operation.current_pr_id,
                operation.current_step,
                None,
                None,
                operation.worktree_path,
                None,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn abort_stack_op(db: &Db, op_id: &str) -> Result<()> {
    let Some(operation) = get_stack_op(db, op_id).await? else {
        return Err(anyhow!("stack operation `{op_id}` not found"));
    };
    if operation.op_kind == StackOperationKind::Rebase {
        if let Some(path) = operation.worktree_path.as_ref() {
            let git = CommandGitOps;
            let _ = git.rebase_abort(Path::new(path)).await?;
        }
    }
    update_operation_state(
        db,
        op_id,
        StackOperationStatus::Aborted,
        operation.current_pr_id,
        operation.current_step,
        operation.last_error,
        None,
        operation.worktree_path,
        Some(now_epoch_seconds()?),
    )
    .await
}

async fn execute_rebase_plan(
    db: &Db,
    git: &dyn GitOps,
    op_id: &str,
    plan: &StackPlan,
    worktree_path: &Path,
    force_push_with_lease: bool,
    start_index: usize,
) -> Result<RebaseExecutionOutcome> {
    let head_by_pr = plan
        .steps
        .iter()
        .map(|step| (step.pr_id.clone(), step.head_ref.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    for (index, step) in plan.steps.iter().enumerate().skip(start_index) {
        let current_step = i64::try_from(index + 1).unwrap_or(i64::MAX);
        update_operation_state(
            db,
            op_id,
            StackOperationStatus::Running,
            Some(step.pr_id.clone()),
            Some(current_step),
            None,
            None,
            Some(worktree_path.to_string_lossy().to_string()),
            None,
        )
        .await?;
        let checkout_result = git.checkout(worktree_path, &step.head_ref).await?;
        if !checkout_result.success {
            pause_failure(
                db,
                op_id,
                format!("git checkout failed: {}", checkout_result.stderr.trim()),
                worktree_path,
                Some(step.pr_id.clone()),
                Some(current_step),
            )
            .await?;
            return Ok(RebaseExecutionOutcome::Paused);
        }
        let target_ref = step
            .parent_pr_id
            .as_ref()
            .and_then(|parent| head_by_pr.get(parent))
            .cloned()
            .unwrap_or_else(|| step.base_ref.clone());
        let rebase_result = git.rebase(worktree_path, &target_ref).await?;
        if !rebase_result.success {
            if is_conflict_output(&rebase_result) {
                pause_conflict(db, git, op_id, worktree_path, step, current_step).await?;
            } else {
                pause_failure(
                    db,
                    op_id,
                    format!("git rebase failed: {}", rebase_result.stderr.trim()),
                    worktree_path,
                    Some(step.pr_id.clone()),
                    Some(current_step),
                )
                .await?;
            }
            return Ok(RebaseExecutionOutcome::Paused);
        }
        if force_push_with_lease {
            let push_result = git
                .push_force_with_lease(worktree_path, &step.head_ref)
                .await?;
            if !push_result.success {
                pause_failure(
                    db,
                    op_id,
                    format!("push failed: {}", push_result.stderr.trim()),
                    worktree_path,
                    Some(step.pr_id.clone()),
                    Some(current_step),
                )
                .await?;
                return Ok(RebaseExecutionOutcome::Paused);
            }
        } else {
            pause_failure(
                db,
                op_id,
                "force_push_with_lease disabled; push manually and resume".to_string(),
                worktree_path,
                Some(step.pr_id.clone()),
                Some(current_step),
            )
            .await?;
            return Ok(RebaseExecutionOutcome::Paused);
        }
    }
    Ok(RebaseExecutionOutcome::Completed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebaseExecutionOutcome {
    Completed,
    Paused,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

fn map_stack_op_row(row: sqlx::sqlite::SqliteRow) -> Result<StackOperationView> {
    let kind_raw: String = row.try_get("op_kind")?;
    let status_raw: String = row.try_get("status")?;
    let conflict_files_json: Option<String> = row.try_get("conflict_files_json")?;
    let conflict_files = conflict_files_json
        .map(|json| serde_json::from_str::<Vec<String>>(&json))
        .transpose()
        .context("parsing stack operation conflict_files_json")?
        .unwrap_or_default();
    Ok(StackOperationView {
        id: row.try_get("id")?,
        stack_id: row.try_get("stack_id")?,
        account_id: row.try_get("account_id")?,
        op_kind: if kind_raw.eq_ignore_ascii_case("merge") {
            StackOperationKind::Merge
        } else {
            StackOperationKind::Rebase
        },
        status: match status_raw.as_str() {
            "pending" => StackOperationStatus::Pending,
            "running" => StackOperationStatus::Running,
            "paused_conflict" => StackOperationStatus::PausedConflict,
            "paused_failure" => StackOperationStatus::PausedFailure,
            "succeeded" => StackOperationStatus::Succeeded,
            _ => StackOperationStatus::Aborted,
        },
        current_pr_id: row.try_get("current_pr_id")?,
        current_step: row.try_get("current_step")?,
        total_steps: row.try_get("total_steps")?,
        worktree_path: row.try_get("worktree_path")?,
        conflict_files,
        last_error: row.try_get("last_error")?,
        started_at: row.try_get("started_at")?,
        updated_at: row.try_get("updated_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn update_operation_state(
    db: &Db,
    op_id: &str,
    status: StackOperationStatus,
    current_pr_id: Option<String>,
    current_step: Option<i64>,
    last_error: Option<String>,
    conflict_files: Option<Vec<String>>,
    worktree_path: Option<String>,
    finished_at: Option<i64>,
) -> Result<()> {
    let now = now_epoch_seconds()?;
    let conflict_files_json = conflict_files
        .map(|files| serde_json::to_string(&files))
        .transpose()
        .context("serializing conflict files")?;
    sqlx::query(
        "UPDATE stack_operations
         SET status = ?2,
             current_pr_id = ?3,
             current_step = ?4,
             last_error = ?5,
             conflict_files_json = COALESCE(?6, conflict_files_json),
             worktree_path = COALESCE(?7, worktree_path),
             updated_at = ?8,
             finished_at = COALESCE(?9, finished_at)
         WHERE id = ?1",
    )
    .bind(op_id)
    .bind(status.as_str())
    .bind(current_pr_id)
    .bind(current_step)
    .bind(last_error)
    .bind(conflict_files_json)
    .bind(worktree_path)
    .bind(now)
    .bind(finished_at)
    .execute(db.pool())
    .await?;
    if let Some(view) = get_stack_op(db, op_id).await? {
        emit_stack_op(&view);
    }
    Ok(())
}

async fn pause_conflict(
    db: &Db,
    git: &dyn GitOps,
    op_id: &str,
    worktree_path: &Path,
    step: &StackStep,
    current_step: i64,
) -> Result<()> {
    pause_conflict_from_path(
        db,
        git,
        op_id,
        worktree_path,
        Some(step.pr_id.clone()),
        Some(current_step),
    )
    .await
}

async fn pause_conflict_from_path(
    db: &Db,
    git: &dyn GitOps,
    op_id: &str,
    worktree_path: &Path,
    current_pr_id: Option<String>,
    current_step: Option<i64>,
) -> Result<()> {
    let status = git.status_porcelain_v2(worktree_path).await?;
    let conflict_files = parse_conflict_files(&status.stdout);
    update_operation_state(
        db,
        op_id,
        StackOperationStatus::PausedConflict,
        current_pr_id,
        current_step,
        Some("rebase conflict; resolve then resume".to_string()),
        Some(conflict_files),
        Some(worktree_path.to_string_lossy().to_string()),
        None,
    )
    .await
}

async fn pause_failure(
    db: &Db,
    op_id: &str,
    message: String,
    worktree_path: &Path,
    current_pr_id: Option<String>,
    current_step: Option<i64>,
) -> Result<()> {
    update_operation_state(
        db,
        op_id,
        StackOperationStatus::PausedFailure,
        current_pr_id,
        current_step,
        Some(message),
        Some(Vec::new()),
        Some(worktree_path.to_string_lossy().to_string()),
        None,
    )
    .await
}

async fn read_bool_setting(db: &Db, key: &str, default: bool) -> Result<bool> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?1 LIMIT 1")
            .bind(key)
            .fetch_optional(db.pool())
            .await?;
    let Some(value) = value else {
        return Ok(default);
    };
    let normalized = value.trim().to_ascii_lowercase();
    Ok(matches!(normalized.as_str(), "1" | "true" | "yes" | "on"))
}

pub async fn set_bool_setting(db: &Db, key: &str, value: bool) -> Result<()> {
    let now = now_epoch_seconds()?;
    sqlx::query(
        "INSERT INTO app_settings(key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET
           value = excluded.value,
           updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(if value { "1" } else { "0" })
    .bind(now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn get_graphite_enabled(db: &Db) -> Result<bool> {
    read_bool_setting(db, "graphite_enabled", false).await
}

pub async fn set_graphite_enabled(db: &Db, enabled: bool) -> Result<()> {
    set_bool_setting(db, "graphite_enabled", enabled).await
}

pub async fn get_force_push_with_lease(db: &Db) -> Result<bool> {
    read_bool_setting(db, "force_push_with_lease", false).await
}

async fn load_stack_plan(db: &Db, account_id: &str, stack_id: &str) -> Result<StackPlan> {
    let rows = sqlx::query(
        "SELECT
            s.id AS stack_id,
            s.account_id,
            s.repo_id,
            r.owner AS repo_owner,
            r.name AS repo_name,
            p.pr_id,
            p.position,
            p.parent_pr_id,
            pr.number,
            pr.title,
            pr.base_ref,
            pr.base_sha,
            pr.head_ref,
            pr.head_sha
         FROM stacks s
         JOIN pr_stack_position p ON p.stack_id = s.id
         JOIN pull_requests pr ON pr.id = p.pr_id
         JOIN repos r ON r.id = s.repo_id
         WHERE s.id = ?1
           AND s.account_id = ?2
         ORDER BY p.position ASC, pr.number ASC",
    )
    .bind(stack_id)
    .bind(account_id)
    .fetch_all(db.pool())
    .await?;
    if rows.is_empty() {
        return Err(anyhow!("stack `{stack_id}` not found"));
    }

    let first = rows.first().expect("rows not empty");
    let stack_id: String = first.try_get("stack_id")?;
    let account_id_value: String = first.try_get("account_id")?;
    let repo_id: String = first.try_get("repo_id")?;
    let repo_owner: String = first.try_get("repo_owner")?;
    let repo_name: String = first.try_get("repo_name")?;
    let mut steps = Vec::with_capacity(rows.len());
    for row in rows {
        steps.push(StackStep {
            pr_id: row.try_get("pr_id")?,
            _position: row.try_get("position")?,
            parent_pr_id: row.try_get("parent_pr_id")?,
            pr_number: row.try_get("number")?,
            title: row.try_get("title")?,
            base_ref: row.try_get("base_ref")?,
            head_ref: row.try_get("head_ref")?,
            head_sha: row.try_get("head_sha")?,
            base_sha: row.try_get("base_sha")?,
            repo_id: row.try_get("repo_id")?,
            repo_owner: row.try_get("repo_owner")?,
            repo_name: row.try_get("repo_name")?,
        });
    }

    Ok(StackPlan {
        _stack_id: stack_id,
        _account_id: account_id_value,
        repo_id,
        _repo_owner: repo_owner,
        _repo_name: repo_name,
        steps,
    })
}

async fn pick_worktree_for_repo(
    db: &Db,
    account_id: &str,
    repo_id: &str,
) -> Result<Option<PathBuf>> {
    let path: Option<String> = sqlx::query_scalar(
        "SELECT path
         FROM worktrees
         WHERE account_id = ?1
           AND repo_id = ?2
         ORDER BY
           CASE WHEN manual_override_pr_id IS NOT NULL THEN 1 ELSE 0 END DESC,
           updated_at DESC
         LIMIT 1",
    )
    .bind(account_id)
    .bind(repo_id)
    .fetch_optional(db.pool())
    .await?;
    Ok(path.map(PathBuf::from))
}

async fn run_git(path: &Path, args: &[&str]) -> Result<GitCommandResult> {
    let output = Command::new("git")
        .args(args)
        .env("GIT_EDITOR", "true")
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("running git {:?} in {}", args, path.display()))?;
    Ok(GitCommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn now_epoch_millis() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_millis()).context("unix timestamp millis exceeds i64")
}

async fn execute_merge_with_handler(
    db: &Db,
    github: &GithubClient,
    account_id: &str,
    stack_id: &str,
    step: &StackStep,
    method: MergeMethod,
) -> Result<()> {
    let merge = Merge;
    let payload = serde_json::json!({
        "pr_id": step.pr_id,
        "target_id": step.pr_id,
        "repo_id": step.repo_id,
        "owner": step.repo_owner,
        "repo": step.repo_name,
        "pr_number": step.pr_number,
        "title": step.title,
        "body": "",
        "base_ref": step.base_ref,
        "base_sha": step.base_sha,
        "head_ref": step.head_ref,
        "head_sha": step.head_sha,
        "merge_method": method.as_api_value(),
        "expected_head_sha": step.head_sha
    });
    let server_call = ServerCallShape::None;
    let mutation_id = format!("stack-merge:{stack_id}:{}", step.pr_id);
    let idempotency_key = mutation_id.clone();
    let apply_ctx = ApplyCtx {
        db,
        github,
        blob_store: db.blob_store(),
        account_id,
        mutation_id: &mutation_id,
        idempotency_key: &idempotency_key,
        input_json: &payload,
        server_call: &server_call,
    };
    let response = merge.apply(&apply_ctx).await?;
    let reconcile_ctx = ReconcileCtx {
        db,
        github,
        blob_store: db.blob_store(),
        account_id,
        mutation_id: &mutation_id,
        idempotency_key: &idempotency_key,
        input_json: &payload,
        sync_handle: None,
        cache_invalidation_emitter: None,
    };
    merge.reconcile(&reconcile_ctx, response).await?;
    Ok(())
}

fn is_conflict_output(result: &GitCommandResult) -> bool {
    let output = format!("{}\n{}", result.stdout, result.stderr).to_ascii_uppercase();
    output.contains("CONFLICT")
}

fn parse_conflict_files(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            if line.starts_with("u ") {
                return line.split_whitespace().last().map(ToString::to_string);
            }
            None
        })
        .collect()
}

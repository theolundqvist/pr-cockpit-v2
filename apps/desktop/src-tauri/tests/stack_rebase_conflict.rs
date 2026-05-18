use anyhow::Result;
use async_trait::async_trait;
use desktop_lib::db::Db;
use desktop_lib::stacks::ops::{
    abort_stack_op, get_stack_op, rebase_stack, set_bool_setting, GitCommandResult, GitOps,
    StackOperationStatus,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[path = "stack_test_support.rs"]
mod stack_test_support;

#[derive(Clone, Default)]
struct ConflictGitOps {
    rebase_calls: Arc<Mutex<usize>>,
}

fn ok_result() -> GitCommandResult {
    GitCommandResult {
        success: true,
        stdout: String::new(),
        stderr: String::new(),
    }
}

#[async_trait]
impl GitOps for ConflictGitOps {
    async fn fetch(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        Ok(ok_result())
    }

    async fn checkout(&self, _worktree_path: &Path, _branch: &str) -> Result<GitCommandResult> {
        Ok(ok_result())
    }

    async fn rebase(&self, _worktree_path: &Path, _target: &str) -> Result<GitCommandResult> {
        let mut calls = self.rebase_calls.lock().expect("lock");
        *calls += 1;
        if *calls == 2 {
            return Ok(GitCommandResult {
                success: false,
                stdout: String::new(),
                stderr: "CONFLICT (content): merge conflict in conflict.txt".to_string(),
            });
        }
        Ok(ok_result())
    }

    async fn rebase_continue(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        Ok(ok_result())
    }

    async fn rebase_abort(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        Ok(ok_result())
    }

    async fn push_force_with_lease(
        &self,
        _worktree_path: &Path,
        _branch: &str,
    ) -> Result<GitCommandResult> {
        Ok(ok_result())
    }

    async fn status_porcelain_v2(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        Ok(GitCommandResult {
            success: true,
            stdout: "u 100644 100644 100644 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 0000000000000000000000000000000000000000 conflict.txt\n".to_string(),
            stderr: String::new(),
        })
    }
}

#[tokio::test]
async fn stack_rebase_conflict_pauses_and_abort_recovers_worktree() -> Result<()> {
    let worktree_dir = TempDir::new()?;
    let worktree_path = worktree_dir.path();
    stack_test_support::run_git(worktree_path, &["init"])?;

    let db_dir = TempDir::new()?;
    let db = Db::open(db_dir.path()).await?;
    let repo_id = "repo-stack";
    let account_id = stack_test_support::seed_account_repo(
        &db,
        "stack-rebase-conflict",
        repo_id,
        "octo",
        "hello-world",
    )
    .await?;

    let prs = vec![
        stack_test_support::pr_row(&account_id, repo_id, "pr_c", 3, "main", "feature/c"),
        stack_test_support::pr_row(&account_id, repo_id, "pr_b", 2, "feature/c", "feature/b"),
        stack_test_support::pr_row(&account_id, repo_id, "pr_a", 1, "feature/b", "feature/a"),
    ];
    stack_test_support::seed_prs(&db, &prs).await?;
    let stack_id = stack_test_support::seed_stack(&db, &account_id, repo_id, &prs).await?;
    stack_test_support::seed_worktree(
        &db,
        &account_id,
        repo_id,
        worktree_path,
        "main",
        Some("pr_a"),
    )
    .await?;
    set_bool_setting(&db, "force_push_with_lease", true).await?;

    let git = ConflictGitOps::default();
    let op_id = rebase_stack(&db, &git, &account_id, &stack_id).await?;
    let paused = get_stack_op(&db, &op_id)
        .await?
        .expect("stack operation to exist");
    assert_eq!(paused.status, StackOperationStatus::PausedConflict);
    assert!(
        paused
            .conflict_files
            .iter()
            .any(|path| path.ends_with("conflict.txt")),
        "conflicted files should include conflict.txt, got {:?}",
        paused.conflict_files
    );

    abort_stack_op(&db, &op_id).await?;
    let aborted = get_stack_op(&db, &op_id)
        .await?
        .expect("stack operation to exist");
    assert_eq!(aborted.status, StackOperationStatus::Aborted);

    let porcelain = stack_test_support::run_git(worktree_path, &["status", "--porcelain"])?;
    assert!(
        porcelain.trim().is_empty(),
        "worktree should be clean after abort, got: {porcelain}"
    );
    Ok(())
}

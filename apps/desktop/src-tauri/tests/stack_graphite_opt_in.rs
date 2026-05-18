use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use desktop_lib::db::Db;
use desktop_lib::stacks::ops::{
    get_stack_op, rebase_stack, set_bool_setting, set_graphite_enabled, GitCommandResult, GitOps,
    StackOperationStatus,
};
use tempfile::TempDir;

#[path = "stack_test_support.rs"]
mod stack_test_support;

#[derive(Default, Clone)]
struct MockGitOps {
    calls: Arc<Mutex<Vec<String>>>,
}

impl MockGitOps {
    fn recorded_calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock").clone()
    }

    fn push_call(&self, value: &str) {
        self.calls.lock().expect("lock").push(value.to_string());
    }
}

fn ok_result() -> GitCommandResult {
    GitCommandResult {
        success: true,
        stdout: String::new(),
        stderr: String::new(),
    }
}

#[async_trait]
impl GitOps for MockGitOps {
    async fn fetch(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        self.push_call("fetch");
        Ok(ok_result())
    }

    async fn checkout(&self, _worktree_path: &Path, _branch: &str) -> Result<GitCommandResult> {
        self.push_call("checkout");
        Ok(ok_result())
    }

    async fn rebase(&self, _worktree_path: &Path, _target: &str) -> Result<GitCommandResult> {
        self.push_call("rebase");
        Ok(ok_result())
    }

    async fn rebase_continue(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        self.push_call("rebase_continue");
        Ok(ok_result())
    }

    async fn rebase_abort(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        self.push_call("rebase_abort");
        Ok(ok_result())
    }

    async fn push_force_with_lease(
        &self,
        _worktree_path: &Path,
        _branch: &str,
    ) -> Result<GitCommandResult> {
        self.push_call("push");
        Ok(ok_result())
    }

    async fn status_porcelain_v2(&self, _worktree_path: &Path) -> Result<GitCommandResult> {
        self.push_call("status");
        Ok(ok_result())
    }
}

#[tokio::test]
async fn stack_graphite_opt_in_prefers_gt_then_falls_back_to_git_when_disabled() -> Result<()> {
    let db_dir = TempDir::new()?;
    let db = Db::open(db_dir.path()).await?;
    let worktree_dir = TempDir::new()?;
    let repo_id = "repo-stack";
    let account_id = stack_test_support::seed_account_repo(
        &db,
        "stack-graphite",
        repo_id,
        "octo",
        "hello-world",
    )
    .await?;
    let prs = vec![stack_test_support::pr_row(
        &account_id,
        repo_id,
        "pr_root",
        1,
        "main",
        "feature/root",
    )];
    stack_test_support::seed_prs(&db, &prs).await?;
    let stack_id = stack_test_support::seed_stack(&db, &account_id, repo_id, &prs).await?;
    stack_test_support::seed_worktree(
        &db,
        &account_id,
        repo_id,
        worktree_dir.path(),
        "main",
        Some("pr_root"),
    )
    .await?;
    set_bool_setting(&db, "force_push_with_lease", true).await?;

    let bin_dir = TempDir::new()?;
    let log_path = bin_dir.path().join("gt.log");
    let gt_path = bin_dir.path().join("gt");
    fs::write(
        &gt_path,
        r#"#!/usr/bin/env bash
if [ "$1" = "--version" ]; then
  echo "gt 1.0.0"
  exit 0
fi
echo "$@" >> "$GT_LOG_FILE"
exit 0
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&gt_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&gt_path, perms)?;
    }
    let previous_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin_dir.path().to_string_lossy(), previous_path),
    );
    std::env::set_var("GT_LOG_FILE", log_path.to_string_lossy().to_string());

    let git = MockGitOps::default();
    set_graphite_enabled(&db, true).await?;
    let first_op_id = rebase_stack(&db, &git, &account_id, &stack_id).await?;
    let first_op = get_stack_op(&db, &first_op_id)
        .await?
        .expect("operation exists");
    assert_eq!(first_op.status, StackOperationStatus::Succeeded);
    assert!(
        git.recorded_calls().is_empty(),
        "gt path should bypass plain git calls when enabled"
    );
    let gt_log = fs::read_to_string(&log_path)?;
    assert!(gt_log.contains("restack"), "expected gt restack invocation");

    set_graphite_enabled(&db, false).await?;
    let second_op_id = rebase_stack(&db, &git, &account_id, &stack_id).await?;
    let second_op = get_stack_op(&db, &second_op_id)
        .await?
        .expect("operation exists");
    assert_eq!(second_op.status, StackOperationStatus::Succeeded);
    let calls = git.recorded_calls();
    assert!(
        calls.iter().any(|entry| entry == "fetch"),
        "plain git flow should fetch when graphite disabled"
    );
    assert!(
        calls.iter().any(|entry| entry == "rebase"),
        "plain git flow should rebase when graphite disabled"
    );

    std::env::set_var("PATH", previous_path);
    std::env::remove_var("GT_LOG_FILE");
    Ok(())
}

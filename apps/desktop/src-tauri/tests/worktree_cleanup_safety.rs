use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result};
use desktop_lib::db::{Db, RepoRecord, WorktreeRecord};
use desktop_lib::ipc::worktree::{
    watcher::WatchBackend, CleanupError, NoopWorktreeEventEmitter, WorktreeService,
};

#[tokio::test]
async fn cleanup_safety_gates_fail_closed() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    db.upsert_auth_account("github.com", "fixture", "pat", "repo", 1)
        .await?;
    db.upsert_repo(&RepoRecord {
        id: "repo_1".to_string(),
        account_id: "github.com:fixture".to_string(),
        owner: "fixture".to_string(),
        name: "repo-1".to_string(),
        default_branch: Some("main".to_string()),
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;

    let user_clean = temp.path().join("user-clean");
    let app_dirty = temp.path().join("app-dirty");
    let app_clean = temp.path().join("app-clean");
    init_repo(&user_clean, false)?;
    init_repo(&app_dirty, true)?;
    init_repo(&app_clean, false)?;

    seed_worktree(
        db.as_ref(),
        "wt-user-clean",
        &user_clean,
        false,
        false,
        "main",
        "sha-user",
    )
    .await?;
    seed_worktree(
        db.as_ref(),
        "wt-app-dirty",
        &app_dirty,
        true,
        true,
        "main",
        "sha-dirty",
    )
    .await?;
    seed_worktree(
        db.as_ref(),
        "wt-app-clean",
        &app_clean,
        false,
        true,
        "main",
        "sha-clean",
    )
    .await?;

    let service = WorktreeService::new(
        Arc::clone(&db),
        Arc::new(NoopWorktreeEventEmitter),
        WatchBackend::default(),
    );

    let blocked = service.cleanup_worktree("wt-user-clean", true).await;
    assert!(matches!(blocked, Err(CleanupError::UserManaged)));

    let dirty_without_force = service.cleanup_worktree("wt-app-dirty", false).await;
    assert!(matches!(dirty_without_force, Err(CleanupError::DirtyState)));

    let dirty_forced = service
        .cleanup_worktree("wt-app-dirty", true)
        .await
        .expect("forced dirty cleanup should snapshot only");
    assert_eq!(dirty_forced.blocked_reason.as_deref(), Some("dirty_state"));
    assert!(!dirty_forced.would_remove);
    assert!(dirty_forced.snapshot_id.is_some());
    assert!(
        app_dirty.exists(),
        "dirty worktree must never be removed in M3 cleanup"
    );
    let dirty_row = db
        .worktree_by_id("wt-app-dirty")
        .await?
        .expect("dirty row should exist");
    assert_eq!(
        dirty_row.last_cleanup_snapshot_id, dirty_forced.snapshot_id,
        "dirty forced cleanup should persist snapshot id"
    );

    let clean_forced = service
        .cleanup_worktree("wt-app-clean", true)
        .await
        .expect("forced clean cleanup should return would-remove");
    assert!(clean_forced.would_remove);
    assert!(!clean_forced.removed);
    assert!(clean_forced.snapshot_id.is_some());
    assert!(
        app_clean.exists(),
        "M3 cleanup should not remove clean worktrees yet"
    );
    let clean_row = db
        .worktree_by_id("wt-app-clean")
        .await?
        .expect("clean row should exist");
    assert_eq!(clean_row.last_cleanup_snapshot_id, clean_forced.snapshot_id);

    Ok(())
}

async fn seed_worktree(
    db: &Db,
    id: &str,
    path: &Path,
    dirty: bool,
    is_app_managed: bool,
    branch: &str,
    head_sha: &str,
) -> Result<()> {
    db.upsert_worktree(&WorktreeRecord {
        id: id.to_string(),
        account_id: "github.com:fixture".to_string(),
        repo_id: "repo_1".to_string(),
        path: path.to_string_lossy().to_string(),
        head_sha: head_sha.to_string(),
        branch: branch.to_string(),
        dirty,
        ahead: 0,
        behind: 0,
        mapped_pr_id: None,
        mapping_confidence: None,
        mapping_source: None,
        is_app_managed,
        manual_override_pr_id: None,
        manual_override_at: None,
        last_cleanup_snapshot_id: None,
        untracked_count: if dirty { 1 } else { 0 },
        staged_count: 0,
        modified_count: if dirty { 1 } else { 0 },
        created_at: 1,
        updated_at: 1,
    })
    .await?;
    Ok(())
}

fn init_repo(path: &Path, dirty: bool) -> Result<()> {
    std::fs::create_dir_all(path)?;
    run_git(path, &["init"])?;
    run_git(path, &["config", "user.name", "Fixture Bot"])?;
    run_git(path, &["config", "user.email", "fixture@example.test"])?;
    std::fs::write(path.join("README.md"), "seed\n")?;
    run_git(path, &["add", "."])?;
    run_git(path, &["commit", "-m", "seed"])?;
    run_git(path, &["branch", "-M", "main"])?;
    if dirty {
        std::fs::write(path.join("dirty.txt"), "dirty\n")?;
    }
    Ok(())
}

fn run_git(path: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .status()
        .with_context(|| format!("running git {:?} in {}", args, path.display()))?;
    if !status.success() {
        anyhow::bail!("git {:?} failed in {}", args, path.display());
    }
    Ok(())
}

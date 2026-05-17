use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use desktop_lib::db::{Db, RepoRecord, WorktreeRecord};
use desktop_lib::ipc::worktree::discovery::discover;
use desktop_lib::ipc::worktree::watcher::{WatchBackend, WorktreeWatchTarget, WorktreeWatcher};

#[tokio::test]
async fn discovery_and_watcher_refresh_state_is_deterministic() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let root = temp.path().join("dev");
    std::fs::create_dir_all(&root)?;

    let repo_a = root.join("repo-a");
    let repo_b = root.join("repo-b");
    init_repo_with_linked_worktree(&repo_a, &root.join("repo-a-wt"))?;
    init_repo_with_linked_worktree(&repo_b, &root.join("repo-b-wt"))?;

    std::fs::write(
        root.join("repo-a-wt").join(".github-pr-cockpit.toml"),
        "[worktree]\nrepo = \"fixture/repo-a\"\nmapped_pr = 7\nignore = false\n",
    )?;

    let discovered = discover(std::slice::from_ref(&root)).await;
    assert!(
        discovered
            .iter()
            .any(|entry| entry.path.ends_with("repo-a-wt")),
        "expected linked worktree discovery for repo-a-wt"
    );
    let override_entry = discovered
        .iter()
        .find(|entry| entry.path.ends_with("repo-a-wt"))
        .expect("override entry should exist");
    assert_eq!(
        override_entry.override_config.mapped_pr,
        Some(7),
        "override mapped_pr should be parsed"
    );

    let db_temp = tempfile::TempDir::new()?;
    let db = Db::open(db_temp.path()).await?;
    db.upsert_auth_account("github.com", "fixture", "pat", "repo", 1)
        .await?;
    db.upsert_repo(&RepoRecord {
        id: "repo_a".to_string(),
        account_id: "github.com:fixture".to_string(),
        owner: "fixture".to_string(),
        name: "repo-a".to_string(),
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

    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let _watcher = WorktreeWatcher::spawn(
        vec![WorktreeWatchTarget {
            worktree_id: "wt-test".to_string(),
            path: root.join("repo-a-wt"),
        }],
        WatchBackend::Poll {
            interval: Duration::from_millis(100),
        },
        tx,
    )?;

    std::fs::write(root.join("repo-a-wt").join("scratch.txt"), "dirty\n")?;

    let event = tokio::time::timeout(Duration::from_secs(8), rx.recv())
        .await
        .context("watcher event timeout")?
        .context("watcher channel closed unexpectedly")?;
    assert!(
        event.state.dirty,
        "file mutation should mark worktree dirty"
    );
    assert!(
        event.state.untracked_count > 0 || event.state.modified_count > 0,
        "dirty event should surface modified/untracked counts"
    );

    db.upsert_worktree(&WorktreeRecord {
        id: "wt-test".to_string(),
        account_id: "github.com:fixture".to_string(),
        repo_id: "repo_a".to_string(),
        path: event.path.to_string_lossy().to_string(),
        head_sha: event.state.head_sha.clone(),
        branch: event.state.branch.clone(),
        dirty: event.state.dirty,
        ahead: event.state.ahead,
        behind: event.state.behind,
        mapped_pr_id: None,
        mapping_confidence: None,
        mapping_source: None,
        is_app_managed: false,
        manual_override_pr_id: None,
        manual_override_at: None,
        last_cleanup_snapshot_id: None,
        untracked_count: event.state.untracked_count,
        staged_count: event.state.staged_count,
        modified_count: event.state.modified_count,
        created_at: 1,
        updated_at: 2,
    })
    .await?;
    let rows = db.list_worktrees("github.com:fixture").await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].dirty, 1);

    Ok(())
}

fn init_repo_with_linked_worktree(repo: &Path, linked_worktree: &Path) -> Result<()> {
    std::fs::create_dir_all(repo)?;
    run_git(repo, &["init"])?;
    run_git(repo, &["config", "user.name", "Fixture Bot"])?;
    run_git(repo, &["config", "user.email", "fixture@example.test"])?;
    std::fs::write(repo.join("README.md"), "seed\n")?;
    run_git(repo, &["add", "."])?;
    run_git(repo, &["commit", "-m", "seed"])?;
    run_git(repo, &["branch", "-M", "main"])?;
    run_git(
        repo,
        &[
            "worktree",
            "add",
            linked_worktree.to_string_lossy().as_ref(),
        ],
    )?;
    Ok(())
}

fn run_git(repo: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .with_context(|| format!("running git {:?} in {}", args, repo.display()))?;
    if !status.success() {
        anyhow::bail!("git {:?} failed in {}", args, repo.display());
    }
    Ok(())
}

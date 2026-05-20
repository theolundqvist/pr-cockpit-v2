#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use desktop_lib::db::{Db, PullRequestRecord, RepoRecord, WorktreeRecord};
use desktop_lib::stacks::{
    detect_stacks, now_epoch_seconds, upsert_stack_state_for_scope, PullRequestRow,
};
use tempfile::TempDir;

pub struct GitFixture {
    pub _root: TempDir,
    pub repo_path: PathBuf,
}

pub fn run_git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .with_context(|| format!("running git {:?} in {}", args, path.display()))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    Err(anyhow!(
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    ))
}

pub fn init_git_fixture() -> Result<GitFixture> {
    let root = TempDir::new()?;
    let remote_path = root.path().join("remote.git");
    let repo_path = root.path().join("repo");
    let remote_path_str = remote_path
        .to_str()
        .ok_or_else(|| anyhow!("remote path contains invalid UTF-8"))?
        .to_string();

    run_git(root.path(), &["init", "--bare", &remote_path_str])?;
    run_git(root.path(), &["init", "repo"])?;
    run_git(&repo_path, &["config", "user.name", "Stack Tester"])?;
    run_git(&repo_path, &["config", "user.email", "stack@test.local"])?;
    fs::write(repo_path.join("shared.txt"), "root\n")?;
    run_git(&repo_path, &["add", "shared.txt"])?;
    run_git(&repo_path, &["commit", "-m", "initial"])?;
    run_git(&repo_path, &["branch", "-M", "main"])?;
    run_git(&repo_path, &["remote", "add", "origin", &remote_path_str])?;
    run_git(&repo_path, &["push", "-u", "origin", "main"])?;

    Ok(GitFixture {
        _root: root,
        repo_path,
    })
}

pub fn seed_linear_git_stack(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["checkout", "-b", "feature/c", "main"])?;
    fs::write(repo_path.join("c.txt"), "c-commit\n")?;
    run_git(repo_path, &["add", "c.txt"])?;
    run_git(repo_path, &["commit", "-m", "C"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/c"])?;

    run_git(repo_path, &["checkout", "-b", "feature/b", "feature/c"])?;
    fs::write(repo_path.join("b.txt"), "b-commit\n")?;
    run_git(repo_path, &["add", "b.txt"])?;
    run_git(repo_path, &["commit", "-m", "B"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/b"])?;

    run_git(repo_path, &["checkout", "-b", "feature/a", "feature/b"])?;
    fs::write(repo_path.join("a.txt"), "a-commit\n")?;
    run_git(repo_path, &["add", "a.txt"])?;
    run_git(repo_path, &["commit", "-m", "A"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/a"])?;
    run_git(repo_path, &["checkout", "main"])?;
    Ok(())
}

pub fn seed_conflicting_git_stack(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["checkout", "-b", "feature/c", "main"])?;
    fs::write(repo_path.join("conflict.txt"), "from-c\n")?;
    run_git(repo_path, &["add", "conflict.txt"])?;
    run_git(repo_path, &["commit", "-m", "C adds conflict file"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/c"])?;

    run_git(repo_path, &["checkout", "-b", "feature/b", "main"])?;
    fs::write(repo_path.join("conflict.txt"), "from-b\n")?;
    run_git(repo_path, &["add", "conflict.txt"])?;
    run_git(repo_path, &["commit", "-m", "B adds conflicting file"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/b"])?;

    run_git(repo_path, &["checkout", "-b", "feature/a", "feature/b"])?;
    fs::write(repo_path.join("a.txt"), "a-commit\n")?;
    run_git(repo_path, &["add", "a.txt"])?;
    run_git(repo_path, &["commit", "-m", "A change"])?;
    run_git(repo_path, &["push", "-u", "origin", "feature/a"])?;
    run_git(repo_path, &["checkout", "main"])?;
    Ok(())
}

pub async fn seed_account_repo(
    db: &Db,
    login: &str,
    repo_id: &str,
    owner: &str,
    name: &str,
) -> Result<String> {
    let now = now_epoch_seconds().unwrap_or(1);
    let account = db
        .upsert_auth_account("github.com", login, "pat", "repo", now)
        .await?;
    db.upsert_repo(&RepoRecord {
        id: repo_id.to_string(),
        account_id: account.id.clone(),
        owner: owner.to_string(),
        name: name.to_string(),
        default_branch: Some("main".to_string()),
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: now,
        updated_at: now,
    })
    .await?;
    Ok(account.id)
}

pub fn pr_row(
    account_id: &str,
    repo_id: &str,
    id: &str,
    number: i64,
    base_ref: &str,
    head_ref: &str,
) -> PullRequestRow {
    let now = now_epoch_seconds().unwrap_or(1);
    PullRequestRow {
        id: id.to_string(),
        account_id: account_id.to_string(),
        repo_id: repo_id.to_string(),
        number,
        title: format!("PR {number}"),
        state: "open".to_string(),
        base_ref: base_ref.to_string(),
        base_sha: format!("base-{id}"),
        head_ref: head_ref.to_string(),
        head_sha: format!("head-{id}"),
        merge_state_status: Some("CLEAN".to_string()),
        review_decision: Some("APPROVED".to_string()),
        check_rollup_state: Some("SUCCESS".to_string()),
        updated_at: now,
    }
}

fn pr_record(pr: &PullRequestRow) -> PullRequestRecord {
    PullRequestRecord {
        id: pr.id.clone(),
        account_id: pr.account_id.clone(),
        repo_id: pr.repo_id.clone(),
        number: pr.number,
        state: pr.state.clone(),
        draft: false,
        title: pr.title.clone(),
        body: String::new(),
        author_id: None,
        base_ref: pr.base_ref.clone(),
        base_sha: pr.base_sha.clone(),
        head_ref: pr.head_ref.clone(),
        head_sha: pr.head_sha.clone(),
        head_repo_id: Some(pr.repo_id.clone()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: pr.merge_state_status.clone(),
        merge_commit_allowed: Some(true),
        squash_merge_allowed: Some(true),
        rebase_merge_allowed: Some(true),
        delete_branch_on_merge_default: Some(true),
        viewer_can_merge: Some(true),
        viewer_can_enable_auto_merge: Some(true),
        viewer_can_disable_auto_merge: Some(true),
        viewer_can_update_branch: Some(true),
        viewer_can_delete_head_ref: Some(true),
        auto_merge_enabled: Some(false),
        auto_merge_method: None,
        auto_merge_commit_headline: None,
        auto_merge_commit_body: None,
        auto_merge_enabled_by_login: None,
        auto_merge_enabled_at: None,
        merge_queue_entry_id: None,
        merge_queue_entry_position: None,
        merge_queue_entry_state: None,
        merge_queue_entry_estimated_ms: None,
        branch_protection_summary_json: None,
        repo_has_merge_queue: Some(false),
        head_ref_state: Some("ACTIVE".to_string()),
        additions: 1,
        deletions: 1,
        changed_files: 1,
        comments_count: 0,
        reviews_count: 0,
        commits_count: 0,
        is_read: true,
        html_url: None,
        created_at: now_epoch_seconds().unwrap_or(1),
        updated_at: now_epoch_seconds().unwrap_or(1),
        closed_at: None,
        merged_at: None,
    }
}

pub async fn seed_prs(db: &Db, prs: &[PullRequestRow]) -> Result<()> {
    for pr in prs {
        db.upsert_pull_request(&pr_record(pr)).await?;
    }
    Ok(())
}

pub async fn seed_stack(
    db: &Db,
    account_id: &str,
    repo_id: &str,
    prs: &[PullRequestRow],
) -> Result<String> {
    let stacks = detect_stacks(prs, repo_id, account_id);
    upsert_stack_state_for_scope(db, account_id, repo_id, &stacks).await?;
    let stack = stacks
        .first()
        .ok_or_else(|| anyhow!("expected one detected stack"))?;
    Ok(stack.stack_id.clone())
}

pub async fn seed_worktree(
    db: &Db,
    account_id: &str,
    repo_id: &str,
    path: &Path,
    branch: &str,
    mapped_pr_id: Option<&str>,
) -> Result<()> {
    let now = now_epoch_seconds().unwrap_or(1);
    db.upsert_worktree(&WorktreeRecord {
        id: "wt_stack".to_string(),
        account_id: account_id.to_string(),
        repo_id: repo_id.to_string(),
        path: path.to_string_lossy().to_string(),
        head_sha: "head".to_string(),
        branch: branch.to_string(),
        dirty: false,
        ahead: 0,
        behind: 0,
        mapped_pr_id: mapped_pr_id.map(ToString::to_string),
        mapping_confidence: Some(1.0),
        mapping_source: Some("{\"manual\":true}".to_string()),
        is_app_managed: false,
        manual_override_pr_id: mapped_pr_id.map(ToString::to_string),
        manual_override_at: Some(now),
        last_cleanup_snapshot_id: None,
        untracked_count: 0,
        staged_count: 0,
        modified_count: 0,
        created_at: now,
        updated_at: now,
    })
    .await?;
    Ok(())
}

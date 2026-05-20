use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use desktop_lib::worktree::write::{
    AuthorIdentity, DiffSide, Git2WorktreeWriter, SuggestionPatch, WorktreeWriteError,
    WorktreeWriteRequest, WorktreeWriter,
};
use git2::Repository;

#[test]
fn worktree_write_happy_path_pushes_commit_with_coauthors() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let remote = temp.path().join("remote.git");
    let repo_path = temp.path().join("repo");
    init_bare_repo(&remote)?;
    init_repo(&repo_path)?;
    write_file(&repo_path.join("src/lib.rs"), "one\ntwo\nthree\n")?;
    write_file(&repo_path.join("src/main.rs"), "alpha\nbeta\ngamma\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )?;
    git(&repo_path, &["branch", "-M", "main"])?;
    git(&repo_path, &["push", "-u", "origin", "main"])?;

    let expected_head = head_sha(&repo_path)?;
    let writer = Git2WorktreeWriter;
    let outcome = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: expected_head.clone(),
            suggestions: vec![
                suggestion("s1", "src/lib.rs", 2, 2, "TWO", "bot-a", &expected_head),
                suggestion("s2", "src/main.rs", 1, 1, "ALPHA", "bot-b", &expected_head),
                suggestion("s3", "src/main.rs", 3, 3, "GAMMA", "bot-a", &expected_head),
            ],
            author: author(),
            message: "Apply 3 suggestions".to_string(),
            force_with_stash: false,
        })
        .context("running happy-path writer")?;

    assert!(outcome.pushed);
    assert_eq!(outcome.head_sha_before, expected_head);
    assert_eq!(
        outcome.coauthors,
        vec!["bot-a".to_string(), "bot-b".to_string()]
    );
    let commit_message = git_output(&repo_path, &["log", "-1", "--pretty=%B"])?;
    assert!(commit_message.contains("Co-authored-by: bot-a <bot-a@users.noreply.github.com>"));
    assert!(commit_message.contains("Co-authored-by: bot-b <bot-b@users.noreply.github.com>"));
    assert_eq!(
        read_file(&repo_path.join("src/lib.rs"))?,
        "one\nTWO\nthree\n"
    );
    assert_eq!(
        read_file(&repo_path.join("src/main.rs"))?,
        "ALPHA\nbeta\nGAMMA\n"
    );

    let remote_head = git_output(
        &repo_path,
        &["ls-remote", "origin", "refs/heads/main", "--heads"],
    )?;
    assert!(remote_head.contains(&outcome.commit_sha));
    Ok(())
}

#[test]
fn worktree_write_dirty_blocks_without_force() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let remote = temp.path().join("remote.git");
    let repo_path = temp.path().join("repo");
    init_bare_repo(&remote)?;
    init_repo(&repo_path)?;
    write_file(&repo_path.join("file.txt"), "a\nb\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )?;
    git(&repo_path, &["branch", "-M", "main"])?;
    git(&repo_path, &["push", "-u", "origin", "main"])?;
    write_file(&repo_path.join("file.txt"), "a\nb\nlocal-dirty\n")?;

    let expected_head = head_sha(&repo_path)?;
    let writer = Git2WorktreeWriter;
    let err = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: expected_head.clone(),
            suggestions: vec![suggestion(
                "s1",
                "file.txt",
                2,
                2,
                "B",
                "bot-a",
                &expected_head,
            )],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: false,
        })
        .expect_err("dirty worktree should reject");

    assert!(matches!(err, WorktreeWriteError::WorktreeDirty { .. }));
    assert!(read_file(&repo_path.join("file.txt"))?.contains("local-dirty"));
    Ok(())
}

#[test]
fn worktree_write_branch_mismatch() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_path = temp.path().join("repo");
    init_repo(&repo_path)?;
    write_file(&repo_path.join("f.txt"), "a\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(&repo_path, &["branch", "-M", "main"])?;

    let expected_head = head_sha(&repo_path)?;
    let writer = Git2WorktreeWriter;
    let err = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "feature".to_string(),
            expected_head_sha: expected_head.clone(),
            suggestions: vec![suggestion(
                "s1",
                "f.txt",
                1,
                1,
                "b",
                "bot-a",
                &expected_head,
            )],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: false,
        })
        .expect_err("branch mismatch should fail");
    assert!(matches!(err, WorktreeWriteError::BranchMismatch { .. }));
    Ok(())
}

#[test]
fn worktree_write_head_mismatch() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_path = temp.path().join("repo");
    init_repo(&repo_path)?;
    write_file(&repo_path.join("f.txt"), "a\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(&repo_path, &["branch", "-M", "main"])?;
    let wrong_head = "0000000000000000000000000000000000000000".to_string();

    let writer = Git2WorktreeWriter;
    let err = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: wrong_head,
            suggestions: vec![suggestion("s1", "f.txt", 1, 1, "b", "bot-a", "head")],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: false,
        })
        .expect_err("head mismatch should fail");
    assert!(matches!(err, WorktreeWriteError::HeadMismatch { .. }));
    Ok(())
}

#[test]
fn worktree_write_suggestion_conflict() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let repo_path = temp.path().join("repo");
    init_repo(&repo_path)?;
    write_file(&repo_path.join("f.txt"), "a\nb\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(&repo_path, &["branch", "-M", "main"])?;

    let expected_head = head_sha(&repo_path)?;
    let mut patch = suggestion("s1", "f.txt", 1, 1, "A", "bot-a", &expected_head);
    patch.original = Some("not-a".to_string());
    patch.original_commit_sha = expected_head.clone();

    let writer = Git2WorktreeWriter;
    let err = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: expected_head,
            suggestions: vec![patch],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: false,
        })
        .expect_err("conflicting suggestion should fail");
    assert!(matches!(err, WorktreeWriteError::SuggestionConflict { .. }));
    assert_eq!(read_file(&repo_path.join("f.txt"))?, "a\nb\n");
    Ok(())
}

#[test]
fn worktree_write_push_rejected_rolls_back_local_commit() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let remote = temp.path().join("remote.git");
    let repo_path = temp.path().join("repo");
    let peer_path = temp.path().join("peer");
    init_bare_repo(&remote)?;
    init_repo(&repo_path)?;
    write_file(&repo_path.join("f.txt"), "a\nb\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )?;
    git(&repo_path, &["branch", "-M", "main"])?;
    git(&repo_path, &["push", "-u", "origin", "main"])?;
    run_git(
        temp.path(),
        &[
            "--git-dir",
            remote.to_string_lossy().as_ref(),
            "symbolic-ref",
            "HEAD",
            "refs/heads/main",
        ],
    )?;
    let head_before = head_sha(&repo_path)?;

    git_clone(&remote, &peer_path)?;
    git(&peer_path, &["checkout", "main"])?;
    write_file(&peer_path.join("peer.txt"), "remote change\n")?;
    git(&peer_path, &["add", "."])?;
    git(&peer_path, &["commit", "-m", "remote advance"])?;
    git(&peer_path, &["push", "origin", "main"])?;

    let writer = Git2WorktreeWriter;
    let err = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: head_before.clone(),
            suggestions: vec![suggestion("s1", "f.txt", 2, 2, "B", "bot-a", &head_before)],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: false,
        })
        .expect_err("remote head advance should reject push");
    assert!(matches!(err, WorktreeWriteError::PushRejected { .. }));
    assert_eq!(head_sha(&repo_path)?, head_before);
    Ok(())
}

#[test]
fn worktree_write_force_with_stash_restores_local_dirty_state() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let remote = temp.path().join("remote.git");
    let repo_path = temp.path().join("repo");
    init_bare_repo(&remote)?;
    init_repo(&repo_path)?;
    write_file(&repo_path.join("f.txt"), "a\nb\n")?;
    git(&repo_path, &["add", "."])?;
    git(&repo_path, &["commit", "-m", "seed"])?;
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    )?;
    git(&repo_path, &["branch", "-M", "main"])?;
    git(&repo_path, &["push", "-u", "origin", "main"])?;
    write_file(&repo_path.join("scratch.txt"), "dirty-local\n")?;

    let expected_head = head_sha(&repo_path)?;
    let writer = Git2WorktreeWriter;
    let outcome = writer
        .run(WorktreeWriteRequest {
            pr_id: "pr_1".to_string(),
            worktree_path: repo_path.clone(),
            expected_branch: "main".to_string(),
            expected_head_sha: expected_head.clone(),
            suggestions: vec![suggestion(
                "s1",
                "f.txt",
                2,
                2,
                "B",
                "bot-a",
                &expected_head,
            )],
            author: author(),
            message: "Apply".to_string(),
            force_with_stash: true,
        })
        .context("force_with_stash should succeed")?;

    assert!(outcome.dirty_snapshot.is_some());
    assert_eq!(read_file(&repo_path.join("scratch.txt"))?, "dirty-local\n");
    Ok(())
}

fn suggestion(
    id: &str,
    relative_path: &str,
    start_line: u32,
    end_line: u32,
    replacement: &str,
    author: &str,
    original_commit_sha: &str,
) -> SuggestionPatch {
    SuggestionPatch {
        id: id.to_string(),
        path: PathBuf::from(relative_path),
        start_line,
        end_line,
        side: DiffSide::Right,
        replacement: replacement.to_string(),
        original: None,
        original_commit_sha: original_commit_sha.to_string(),
        suggestion_author_login: author.to_string(),
    }
}

fn author() -> AuthorIdentity {
    AuthorIdentity {
        login: "fixture".to_string(),
        name: "Fixture Bot".to_string(),
        email: "fixture@example.test".to_string(),
        push_token: None,
        remote_name: None,
    }
}

fn init_repo(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    git(path, &["init"])?;
    git(path, &["config", "user.name", "Fixture Bot"])?;
    git(path, &["config", "user.email", "fixture@example.test"])?;
    Ok(())
}

fn init_bare_repo(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    run_git(
        path.parent().unwrap_or(path),
        &["init", "--bare", path.to_string_lossy().as_ref()],
    )
}

fn git_clone(remote: &Path, destination: &Path) -> Result<()> {
    run_git(
        destination.parent().unwrap_or(destination),
        &[
            "clone",
            remote.to_string_lossy().as_ref(),
            destination.to_string_lossy().as_ref(),
        ],
    )
}

fn head_sha(path: &Path) -> Result<String> {
    let repo = Repository::open(path)?;
    let head = repo.head()?;
    let oid = head.target().context("head must point to commit")?;
    Ok(oid.to_string())
}

fn git(path: &Path, args: &[&str]) -> Result<()> {
    run_git(path, args)
}

fn git_output(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .with_context(|| format!("running git {:?} in {}", args, path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {:?} failed in {}: {}",
            args,
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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

fn write_file(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    Ok(())
}

fn read_file(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

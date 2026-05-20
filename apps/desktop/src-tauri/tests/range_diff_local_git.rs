use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use desktop_lib::range_diff::{
    CommitPairStatus, HighlightKind, LocalGitRangeDiff, RangeDiffSource, RangeDiffSourceProvider,
    RepoLocator,
};

fn run_git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .with_context(|| format!("running git command: {}", args.join(" ")))?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed (status {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[tokio::test]
async fn local_git_provider_builds_modified_pair_with_highlights() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    run_git(temp.path(), &["init"])?;
    run_git(
        temp.path(),
        &["config", "user.email", "range-diff@test.local"],
    )?;
    run_git(temp.path(), &["config", "user.name", "Range Diff Test"])?;

    fs::write(
        temp.path().join("example.txt"),
        "line one\nline two\nline three\n",
    )?;
    run_git(temp.path(), &["add", "example.txt"])?;
    run_git(temp.path(), &["commit", "-m", "base"])?;
    let base_sha = run_git(temp.path(), &["rev-parse", "HEAD"])?;

    run_git(temp.path(), &["checkout", "-b", "old-head"])?;
    fs::write(
        temp.path().join("example.txt"),
        "line one\nline two old\nline three\n",
    )?;
    run_git(temp.path(), &["commit", "-am", "Adjust greeting wording"])?;
    let old_head_sha = run_git(temp.path(), &["rev-parse", "HEAD"])?;

    run_git(temp.path(), &["checkout", "-B", "new-head", &base_sha])?;
    fs::write(
        temp.path().join("example.txt"),
        "line one\nline two new\nline three\n",
    )?;
    run_git(temp.path(), &["commit", "-am", "Adjust greeting wording"])?;
    let new_head_sha = run_git(temp.path(), &["rev-parse", "HEAD"])?;

    let provider = LocalGitRangeDiff {
        worktree_path: temp.path().to_path_buf(),
    };
    let range_diff = provider
        .compute(
            "github.com:fixture-user",
            &RepoLocator {
                owner: "acme".to_string(),
                name: "rocket".to_string(),
            },
            &base_sha,
            &old_head_sha,
            &new_head_sha,
        )
        .await?;

    assert_eq!(range_diff.mode, RangeDiffSource::LocalGit);
    assert_eq!(range_diff.old_range.head_sha, old_head_sha);
    assert_eq!(range_diff.new_range.head_sha, new_head_sha);

    let modified_pair = range_diff
        .commit_pairs
        .iter()
        .find(|pair| pair.status == CommitPairStatus::Modified)
        .expect("expected a modified pair in range-diff");
    let intra = modified_pair
        .intra_diff
        .as_ref()
        .expect("modified pair should include intra-line highlights");
    let first_hunk = intra.hunks.first().expect("expected at least one hunk");

    assert!(
        first_hunk
            .old_lines
            .iter()
            .flat_map(|line| line.segments.iter())
            .any(|segment| segment.kind == HighlightKind::Removed),
        "old side should contain removed highlights"
    );
    assert!(
        first_hunk
            .new_lines
            .iter()
            .flat_map(|line| line.segments.iter())
            .any(|segment| segment.kind == HighlightKind::Added),
        "new side should contain added highlights"
    );

    Ok(())
}

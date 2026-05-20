mod support;

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use desktop_lib::range_diff::{
    pick_provider, RangeDiffError, RangeDiffSource, RangeDiffSourceProvider, RepoLocator,
    RestCompareRangeDiff, WorktreeMapping,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn run_git(path: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
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
async fn missing_worktree_commits_fall_back_to_rest_compare() -> Result<()> {
    let repo_dir = tempfile::TempDir::new()?;
    run_git(repo_dir.path(), &["init"])?;
    run_git(
        repo_dir.path(),
        &["config", "user.email", "range-diff@test.local"],
    )?;
    run_git(repo_dir.path(), &["config", "user.name", "Range Diff Test"])?;
    std::fs::write(repo_dir.path().join("example.txt"), "line one\nline two\n")?;
    run_git(repo_dir.path(), &["add", "example.txt"])?;
    run_git(repo_dir.path(), &["commit", "-m", "base"])?;
    let base_sha = run_git(repo_dir.path(), &["rev-parse", "HEAD"])?;

    std::fs::write(
        repo_dir.path().join("example.txt"),
        "line one\nline two new\n",
    )?;
    run_git(repo_dir.path(), &["commit", "-am", "new"])?;
    let new_head_sha = run_git(repo_dir.path(), &["rev-parse", "HEAD"])?;
    let old_head_sha = "9999999999999999999999999999999999999999".to_string();

    let server = MockServer::start().await;
    let harness = support::build_harness(&server.uri(), "ghp_fallback", "fallback-user").await?;
    let github = Arc::new(harness.github.clone());
    let repo = RepoLocator {
        owner: "acme".to_string(),
        name: "rocket".to_string(),
    };

    let preferred = pick_provider(
        Some(WorktreeMapping {
            path: repo_dir.path().to_string_lossy().to_string(),
            confidence: 0.99,
            source: Some("test".to_string()),
            manual_override_pr_id: Some("pr_1".to_string()),
        }),
        Arc::clone(&github),
    );
    let preferred_error = preferred
        .compute(
            &harness.account_id,
            &repo,
            &base_sha,
            &old_head_sha,
            &new_head_sha,
        )
        .await
        .expect_err("local provider should fail when commits are missing");
    assert!(preferred_error
        .downcast_ref::<RangeDiffError>()
        .is_some_and(|typed| {
            matches!(
                typed,
                RangeDiffError::WorktreeMissingCommits { missing_sha } if missing_sha == &old_head_sha
            )
        }));

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/rocket/compare/{base_sha}...{old_head_sha}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "commits": [{ "sha": old_head_sha }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/rocket/compare/{base_sha}...{new_head_sha}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "commits": [{ "sha": new_head_sha }]
        })))
        .mount(&server)
        .await;
    for (sha, patch) in [
        (old_head_sha.as_str(), "@@ -1 +1 @@\n-old\n+old"),
        (new_head_sha.as_str(), "@@ -1 +1 @@\n-old\n+new"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/repos/acme/rocket/commits/{sha}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sha": sha,
                "commit": {
                    "message": "fallback",
                    "author": { "name": "Fixture", "date": "2026-05-18T00:00:00Z" }
                },
                "files": [{
                    "filename": "src/example.txt",
                    "patch": patch,
                    "additions": 1,
                    "deletions": 1
                }]
            })))
            .mount(&server)
            .await;
    }

    let fallback = RestCompareRangeDiff { github };
    let result = fallback
        .compute(
            &harness.account_id,
            &repo,
            &base_sha,
            &old_head_sha,
            &new_head_sha,
        )
        .await?;
    assert_eq!(result.mode, RangeDiffSource::RestCompare);

    Ok(())
}

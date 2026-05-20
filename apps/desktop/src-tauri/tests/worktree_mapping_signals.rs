use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Result;
use desktop_lib::ipc::worktree::mapping::{
    best_mapping, MappingCommandRunner, PullRequestCandidate, WorktreeMappingInput,
};

#[derive(Default)]
struct FakeRunner {
    remote_url: Option<String>,
    upstream: Option<String>,
    gh_pr_number: Option<i64>,
    related_pairs: HashSet<(String, String)>,
}

impl MappingCommandRunner for FakeRunner {
    fn origin_remote_url<'a>(
        &'a self,
        _worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>> {
        Box::pin(async move { self.remote_url.clone() })
    }

    fn branch_upstream<'a>(
        &'a self,
        _worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>> {
        Box::pin(async move { self.upstream.clone() })
    }

    fn gh_current_branch_pr_number<'a>(
        &'a self,
        _worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<i64>> + Send + 'a>> {
        Box::pin(async move { self.gh_pr_number })
    }

    fn heads_are_related<'a>(
        &'a self,
        _worktree_path: &'a Path,
        left_head_sha: &'a str,
        right_head_sha: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let key = (left_head_sha.to_string(), right_head_sha.to_string());
        Box::pin(async move { self.related_pairs.contains(&key) })
    }
}

fn candidate(number: i64, id: &str) -> PullRequestCandidate {
    PullRequestCandidate {
        id: id.to_string(),
        number,
        repo_owner: "acme".to_string(),
        repo_name: "rocket".to_string(),
        head_ref: format!("feature-{number}"),
        head_sha: format!("sha-{number}"),
        head_repo_owner: Some("acme".to_string()),
    }
}

#[tokio::test]
async fn high_confidence_uses_remote_upstream_and_head_sha() -> Result<()> {
    let runner = FakeRunner {
        remote_url: Some("git@github.com:acme/rocket.git".to_string()),
        upstream: Some("acme/feature-12".to_string()),
        gh_pr_number: None,
        related_pairs: HashSet::from([(String::from("sha-12"), String::from("sha-12"))]),
    };
    let worktree = WorktreeMappingInput {
        worktree_path: PathBuf::from("/tmp/worktree"),
        branch: "feature-12".to_string(),
        head_sha: "sha-12".to_string(),
        manual_override_pr_id: None,
    };
    let outcome = best_mapping(&runner, &worktree, &[candidate(12, "pr-12")]).await;
    assert_eq!(outcome.mapped_pr_id.as_deref(), Some("pr-12"));
    assert!(
        outcome.mapping_confidence >= 0.8,
        "expected high confidence, got {}",
        outcome.mapping_confidence
    );
    Ok(())
}

#[tokio::test]
async fn low_confidence_branch_convention_only() -> Result<()> {
    let runner = FakeRunner {
        remote_url: Some("git@github.com:wrong/repo.git".to_string()),
        upstream: Some("other/main".to_string()),
        gh_pr_number: None,
        related_pairs: HashSet::new(),
    };
    let worktree = WorktreeMappingInput {
        worktree_path: PathBuf::from("/tmp/worktree"),
        branch: "pr/77".to_string(),
        head_sha: "different".to_string(),
        manual_override_pr_id: None,
    };
    let outcome = best_mapping(&runner, &worktree, &[candidate(77, "pr-77")]).await;
    assert_eq!(outcome.mapped_pr_id.as_deref(), Some("pr-77"));
    assert!(
        outcome.mapping_confidence <= 0.11,
        "expected low confidence, got {}",
        outcome.mapping_confidence
    );
    Ok(())
}

#[tokio::test]
async fn manual_override_wins_and_clearing_restores_auto() -> Result<()> {
    let runner = FakeRunner {
        remote_url: Some("git@github.com:acme/rocket.git".to_string()),
        upstream: Some("acme/feature-9".to_string()),
        gh_pr_number: Some(9),
        related_pairs: HashSet::from([(String::from("sha-9"), String::from("sha-9"))]),
    };
    let candidate = candidate(9, "pr-9");

    let manual = WorktreeMappingInput {
        worktree_path: PathBuf::from("/tmp/worktree"),
        branch: "feature-9".to_string(),
        head_sha: "sha-9".to_string(),
        manual_override_pr_id: Some("pr-manual".to_string()),
    };
    let manual_outcome = best_mapping(&runner, &manual, std::slice::from_ref(&candidate)).await;
    assert_eq!(manual_outcome.mapped_pr_id.as_deref(), Some("pr-manual"));
    assert_eq!(manual_outcome.mapping_confidence, 1.0);

    let cleared = WorktreeMappingInput {
        manual_override_pr_id: None,
        ..manual
    };
    let auto_outcome = best_mapping(&runner, &cleared, &[candidate]).await;
    assert_eq!(auto_outcome.mapped_pr_id.as_deref(), Some("pr-9"));
    assert!(auto_outcome.mapping_confidence > 0.8);
    Ok(())
}

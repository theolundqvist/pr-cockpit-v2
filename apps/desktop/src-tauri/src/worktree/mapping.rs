use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

const REMOTE_URL_WEIGHT: f64 = 0.30;
const UPSTREAM_WEIGHT: f64 = 0.20;
const GH_STATUS_WEIGHT: f64 = 0.20;
const HEAD_SHA_WEIGHT: f64 = 0.15;
const BRANCH_CONVENTION_WEIGHT: f64 = 0.10;
const ANCESTRY_WEIGHT: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSlug {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeMappingInput {
    pub worktree_path: PathBuf,
    pub branch: String,
    pub head_sha: String,
    pub manual_override_pr_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestCandidate {
    pub id: String,
    pub number: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub head_ref: String,
    pub head_sha: String,
    pub head_repo_owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalContribution {
    pub signal: String,
    pub weight: f64,
    pub confidence: f64,
    pub contribution: f64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingOutcome {
    pub mapped_pr_id: Option<String>,
    pub mapping_confidence: f64,
    pub mapping_source: String,
}

pub trait MappingCommandRunner: Send + Sync {
    fn origin_remote_url<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;
    fn branch_upstream<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>>;
    fn gh_current_branch_pr_number<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<i64>> + Send + 'a>>;
    fn heads_are_related<'a>(
        &'a self,
        worktree_path: &'a Path,
        left_head_sha: &'a str,
        right_head_sha: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

#[derive(Debug, Default, Clone)]
pub struct RealMappingCommandRunner;

impl MappingCommandRunner for RealMappingCommandRunner {
    fn origin_remote_url<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>> {
        Box::pin(async move {
            run_git(worktree_path, &["remote", "get-url", "origin"])
                .await
                .and_then(non_empty)
        })
    }

    fn branch_upstream<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'a>> {
        Box::pin(async move {
            run_git(worktree_path, &["rev-parse", "--abbrev-ref", "@{u}"])
                .await
                .and_then(non_empty)
        })
    }

    fn gh_current_branch_pr_number<'a>(
        &'a self,
        worktree_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Option<i64>> + Send + 'a>> {
        Box::pin(async move {
            let output = Command::new("gh")
                .arg("pr")
                .arg("status")
                .arg("--json")
                .arg("number,headRefName")
                .arg("--jq")
                .arg(".currentBranch.number")
                .current_dir(worktree_path)
                .env_remove("GH_TOKEN")
                .output()
                .await
                .ok()?;
            if !output.status.success() {
                return None;
            }
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<i64>()
                .ok()
        })
    }

    fn heads_are_related<'a>(
        &'a self,
        worktree_path: &'a Path,
        left_head_sha: &'a str,
        right_head_sha: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            if left_head_sha.is_empty() || right_head_sha.is_empty() {
                return false;
            }
            let forward = run_git_status(
                worktree_path,
                &["merge-base", "--is-ancestor", left_head_sha, right_head_sha],
            )
            .await;
            if forward == Some(0) {
                return true;
            }
            let reverse = run_git_status(
                worktree_path,
                &["merge-base", "--is-ancestor", right_head_sha, left_head_sha],
            )
            .await;
            reverse == Some(0)
        })
    }
}

pub async fn best_mapping<R: MappingCommandRunner>(
    runner: &R,
    worktree: &WorktreeMappingInput,
    candidates: &[PullRequestCandidate],
) -> MappingOutcome {
    if let Some(manual_override_pr_id) = worktree.manual_override_pr_id.as_ref() {
        return MappingOutcome {
            mapped_pr_id: Some(manual_override_pr_id.clone()),
            mapping_confidence: 1.0,
            mapping_source: r#"{"manual":true}"#.to_string(),
        };
    }

    if candidates.is_empty() {
        return MappingOutcome {
            mapped_pr_id: None,
            mapping_confidence: 0.0,
            mapping_source: r#"{"signals":[],"reason":"no_candidates"}"#.to_string(),
        };
    }

    let remote_url = runner.origin_remote_url(&worktree.worktree_path).await;
    let upstream = runner.branch_upstream(&worktree.worktree_path).await;
    let gh_pr_number = runner
        .gh_current_branch_pr_number(&worktree.worktree_path)
        .await;

    let mut best_candidate = None::<(String, f64, Vec<SignalContribution>)>;
    for candidate in candidates {
        let remote_signal = remote_url_match_signal(remote_url.as_deref(), candidate);
        let upstream_signal = upstream_match_signal(upstream.as_deref(), candidate);
        let gh_signal = gh_status_signal(gh_pr_number, candidate.number);
        let sha_signal = exact_head_sha_signal(&worktree.head_sha, &candidate.head_sha);
        let branch_signal = branch_convention_signal(&worktree.branch, candidate);
        let ancestry_signal = ancestry_signal(
            runner,
            &worktree.worktree_path,
            &worktree.head_sha,
            &candidate.head_sha,
        )
        .await;

        let signals = vec![
            remote_signal,
            upstream_signal,
            gh_signal,
            sha_signal,
            branch_signal,
            ancestry_signal,
        ];
        let confidence = signals
            .iter()
            .fold(0.0_f64, |sum, signal| sum + signal.contribution)
            .clamp(0.0, 1.0);

        match best_candidate {
            Some((_, best_confidence, _)) if best_confidence >= confidence => {}
            _ => {
                best_candidate = Some((candidate.id.clone(), confidence, signals));
            }
        }
    }

    let Some((mapped_pr_id, mapping_confidence, signals)) = best_candidate else {
        return MappingOutcome {
            mapped_pr_id: None,
            mapping_confidence: 0.0,
            mapping_source: r#"{"signals":[],"reason":"mapping_failed"}"#.to_string(),
        };
    };
    let mapping_source = serde_json::json!({
        "manual": false,
        "confidence": mapping_confidence,
        "signals": signals,
    })
    .to_string();

    MappingOutcome {
        mapped_pr_id: Some(mapped_pr_id),
        mapping_confidence,
        mapping_source,
    }
}

fn remote_url_match_signal(
    remote_url: Option<&str>,
    candidate: &PullRequestCandidate,
) -> SignalContribution {
    let remote_slug = remote_url.and_then(parse_repo_slug);
    let confidence = match remote_slug.as_ref() {
        Some(slug) if slug.owner == candidate.repo_owner && slug.name == candidate.repo_name => 1.0,
        Some(_) => 0.0,
        None => 0.0,
    };
    SignalContribution {
        signal: "remote_url_match".to_string(),
        weight: REMOTE_URL_WEIGHT,
        confidence,
        contribution: confidence * REMOTE_URL_WEIGHT,
        detail: remote_slug.map(|slug| format!("{}/{}", slug.owner, slug.name)),
    }
}

fn upstream_match_signal(
    upstream: Option<&str>,
    candidate: &PullRequestCandidate,
) -> SignalContribution {
    let expected_owner = candidate
        .head_repo_owner
        .as_deref()
        .unwrap_or(candidate.repo_owner.as_str());
    let expected = format!("{expected_owner}/{}", candidate.head_ref);
    let confidence = upstream.map_or(0.0, |value| (value == expected) as i32 as f64);
    SignalContribution {
        signal: "branch_upstream_match".to_string(),
        weight: UPSTREAM_WEIGHT,
        confidence,
        contribution: confidence * UPSTREAM_WEIGHT,
        detail: upstream.map(ToString::to_string),
    }
}

fn gh_status_signal(gh_pr_number: Option<i64>, candidate_number: i64) -> SignalContribution {
    let weight = if gh_pr_number.is_some() {
        GH_STATUS_WEIGHT
    } else {
        0.0
    };
    let confidence = gh_pr_number.map_or(0.0, |number| (number == candidate_number) as i32 as f64);
    SignalContribution {
        signal: "gh_pr_status".to_string(),
        weight,
        confidence,
        contribution: confidence * weight,
        detail: gh_pr_number.map(|value| value.to_string()),
    }
}

fn exact_head_sha_signal(worktree_head_sha: &str, candidate_head_sha: &str) -> SignalContribution {
    let confidence = (worktree_head_sha == candidate_head_sha) as i32 as f64;
    SignalContribution {
        signal: "exact_head_sha".to_string(),
        weight: HEAD_SHA_WEIGHT,
        confidence,
        contribution: confidence * HEAD_SHA_WEIGHT,
        detail: Some(worktree_head_sha.to_string()),
    }
}

fn branch_convention_signal(branch: &str, candidate: &PullRequestCandidate) -> SignalContribution {
    let candidate_head_owner = candidate
        .head_repo_owner
        .as_deref()
        .unwrap_or(candidate.repo_owner.as_str());
    let by_pr_number = branch == format!("pr/{}", candidate.number);
    let by_fork_convention = branch == format!("{candidate_head_owner}/{}", candidate.head_ref);
    let by_head_ref = branch == candidate.head_ref;
    let confidence = (by_pr_number || by_fork_convention || by_head_ref) as i32 as f64;
    SignalContribution {
        signal: "branch_convention".to_string(),
        weight: BRANCH_CONVENTION_WEIGHT,
        confidence,
        contribution: confidence * BRANCH_CONVENTION_WEIGHT,
        detail: Some(branch.to_string()),
    }
}

async fn ancestry_signal<R: MappingCommandRunner>(
    runner: &R,
    worktree_path: &Path,
    worktree_head_sha: &str,
    candidate_head_sha: &str,
) -> SignalContribution {
    let confidence = runner
        .heads_are_related(worktree_path, candidate_head_sha, worktree_head_sha)
        .await as i32 as f64;
    SignalContribution {
        signal: "head_sha_ancestry".to_string(),
        weight: ANCESTRY_WEIGHT,
        confidence,
        contribution: confidence * ANCESTRY_WEIGHT,
        detail: Some(format!("{candidate_head_sha}~{worktree_head_sha}")),
    }
}

pub fn parse_repo_slug(remote_url: &str) -> Option<RepoSlug> {
    let normalized = remote_url.trim().trim_end_matches(".git");
    if let Some((_, tail)) = normalized.rsplit_once(':') {
        if tail.contains('/') {
            let (owner, name) = tail.split_once('/')?;
            return Some(RepoSlug {
                owner: owner.to_string(),
                name: name.to_string(),
            });
        }
    }

    let trimmed = normalized.trim_end_matches('/');
    let parts = trimmed.split('/').collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let owner = parts[parts.len() - 2];
    let name = parts[parts.len() - 1];
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(RepoSlug {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

async fn run_git(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

async fn run_git_status(path: &Path, args: &[&str]) -> Option<i32> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .await
        .ok()?;
    output.status.code()
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

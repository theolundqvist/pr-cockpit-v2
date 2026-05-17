use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use tokio::process::Command;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const DISCOVERY_PROCESS_CAP: usize = 8;
const OVERRIDE_FILE_NAME: &str = ".github-pr-cockpit.toml";

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct WorktreeOverride {
    pub repo: Option<String>,
    pub mapped_pr: Option<i64>,
    pub is_app_managed: Option<bool>,
    pub ignore: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredWorktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    pub origin_remote_url: Option<String>,
    pub override_config: WorktreeOverride,
}

#[derive(Debug, Default, Deserialize)]
struct CockpitOverrideToml {
    worktree: Option<WorktreeOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PorcelainWorktreeEntry {
    path: PathBuf,
    branch: Option<String>,
    head_sha: Option<String>,
}

pub async fn discover(roots: &[PathBuf]) -> Vec<DiscoveredWorktree> {
    let repo_roots = collect_repo_roots(roots).await;
    let semaphore = Arc::new(Semaphore::new(DISCOVERY_PROCESS_CAP));
    let mut join_set = tokio::task::JoinSet::new();

    for repo_root in repo_roots {
        let permit = semaphore.clone().acquire_owned().await;
        if let Ok(permit) = permit {
            join_set.spawn(discover_repo_worktrees(repo_root, permit));
        }
    }

    let mut by_path = BTreeMap::<PathBuf, DiscoveredWorktree>::new();
    while let Some(result) = join_set.join_next().await {
        let Ok(entries) = result else {
            continue;
        };
        for entry in entries {
            by_path.insert(entry.path.clone(), entry);
        }
    }
    by_path.into_values().collect()
}

async fn collect_repo_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = BTreeMap::<PathBuf, ()>::new();
    for root in roots {
        let mut entries = match tokio::fs::read_dir(root).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let repo_root = entry.path();
            let git_dir = repo_root.join(".git");
            if tokio::fs::metadata(&git_dir).await.is_err() {
                continue;
            }
            let canonical = canonicalize_or_self(repo_root).await;
            seen.insert(canonical, ());
        }
    }
    seen.into_keys().collect()
}

async fn discover_repo_worktrees(
    repo_root: PathBuf,
    _permit: OwnedSemaphorePermit,
) -> Vec<DiscoveredWorktree> {
    let worktree_entries = list_worktrees(&repo_root).await;
    if worktree_entries.is_empty() {
        return Vec::new();
    }
    let origin_remote_url = read_origin_remote_url(&repo_root).await;
    let mut discovered = Vec::with_capacity(worktree_entries.len());
    for entry in worktree_entries {
        let canonical_path = canonicalize_or_self(entry.path).await;
        let override_config = read_override_file(&canonical_path)
            .await
            .unwrap_or_default();
        if override_config.ignore.unwrap_or(false) {
            continue;
        }
        discovered.push(DiscoveredWorktree {
            path: canonical_path,
            branch: entry.branch,
            head_sha: entry.head_sha,
            origin_remote_url: origin_remote_url.clone(),
            override_config,
        });
    }
    discovered
}

async fn list_worktrees(repo_root: &Path) -> Vec<PorcelainWorktreeEntry> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output()
        .await;
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout))
}

async fn read_origin_remote_url(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_worktree_porcelain(input: &str) -> Vec<PorcelainWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path = None;
    let mut current_branch = None;
    let mut current_head_sha = None;

    let flush = |entries: &mut Vec<PorcelainWorktreeEntry>,
                 path: &mut Option<PathBuf>,
                 branch: &mut Option<String>,
                 head_sha: &mut Option<String>| {
        if let Some(path) = path.take() {
            entries.push(PorcelainWorktreeEntry {
                path,
                branch: branch.take(),
                head_sha: head_sha.take(),
            });
        }
    };

    for raw_line in input.lines() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_branch,
                &mut current_head_sha,
            );
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_branch,
                &mut current_head_sha,
            );
            current_path = Some(PathBuf::from(path));
            continue;
        }
        if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
            continue;
        }
        if let Some(head_sha) = line.strip_prefix("HEAD ") {
            current_head_sha = Some(head_sha.to_string());
        }
    }

    flush(
        &mut entries,
        &mut current_path,
        &mut current_branch,
        &mut current_head_sha,
    );
    entries
}

async fn read_override_file(path: &Path) -> Option<WorktreeOverride> {
    let override_path = path.join(OVERRIDE_FILE_NAME);
    let raw = tokio::fs::read_to_string(override_path).await.ok()?;
    let parsed: CockpitOverrideToml = toml::from_str(&raw).ok()?;
    parsed.worktree
}

async fn canonicalize_or_self(path: PathBuf) -> PathBuf {
    tokio::fs::canonicalize(&path).await.unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::parse_worktree_porcelain;

    #[test]
    fn parses_porcelain_entries() {
        let parsed = parse_worktree_porcelain(
            r#"worktree /tmp/repo
HEAD abc
branch refs/heads/main

worktree /tmp/repo-pr1
HEAD def
branch refs/heads/pr/1
"#,
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert_eq!(parsed[1].branch.as_deref(), Some("pr/1"));
    }
}

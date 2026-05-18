use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use similar::{ChangeTag, TextDiff};
use specta::Type;
use tokio::process::Command;

use crate::api::GithubClient;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RepoLocator {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum RangeDiffSource {
    LocalGit,
    RestCompare,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum CommitPairStatus {
    Unchanged,
    Modified,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum DiffSide {
    Old,
    New,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum HighlightKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct HighlightSegment {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct HighlightedLine {
    pub text: String,
    pub segments: Vec<HighlightSegment>,
    pub side: DiffSide,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct IntraHunk {
    pub old_lines: Vec<HighlightedLine>,
    pub new_lines: Vec<HighlightedLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct IntraDiff {
    pub hunks: Vec<IntraHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CommitMetadata {
    pub sha: String,
    pub title: String,
    pub body: String,
    pub author_name: Option<String>,
    pub committed_at: Option<i64>,
    pub patch_hash: String,
    pub file_paths: Vec<String>,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CommitRange {
    pub base_sha: String,
    pub head_sha: String,
    pub commits: Vec<CommitMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CommitPair {
    pub status: CommitPairStatus,
    pub old: Option<CommitMetadata>,
    pub new: Option<CommitMetadata>,
    pub intra_diff: Option<IntraDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RangeDiff {
    pub old_range: CommitRange,
    pub new_range: CommitRange,
    pub commit_pairs: Vec<CommitPair>,
    pub mode: RangeDiffSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct WorktreeMapping {
    pub path: String,
    pub confidence: f64,
    pub source: Option<String>,
    pub manual_override_pr_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RangeDiffError {
    #[error("worktree missing commit `{missing_sha}`")]
    WorktreeMissingCommits { missing_sha: String },
}

#[async_trait]
pub trait RangeDiffSourceProvider: Send + Sync {
    async fn compute(
        &self,
        account_id: &str,
        repo: &RepoLocator,
        base_sha: &str,
        old_head_sha: &str,
        new_head_sha: &str,
    ) -> Result<RangeDiff>;
}

#[derive(Debug, Clone)]
pub struct LocalGitRangeDiff {
    pub worktree_path: PathBuf,
}

#[derive(Clone)]
pub struct RestCompareRangeDiff {
    pub github: Arc<GithubClient>,
}

pub fn pick_provider(
    worktree: Option<WorktreeMapping>,
    github: Arc<GithubClient>,
) -> Box<dyn RangeDiffSourceProvider> {
    if let Some(mapping) = worktree {
        let manual = mapping.manual_override_pr_id.is_some();
        if manual || mapping.confidence >= 0.75 {
            return Box::new(LocalGitRangeDiff {
                worktree_path: PathBuf::from(mapping.path),
            });
        }
    }
    Box::new(RestCompareRangeDiff { github })
}

#[async_trait]
impl RangeDiffSourceProvider for LocalGitRangeDiff {
    async fn compute(
        &self,
        _account_id: &str,
        _repo: &RepoLocator,
        base_sha: &str,
        old_head_sha: &str,
        new_head_sha: &str,
    ) -> Result<RangeDiff> {
        ensure_commit_available(&self.worktree_path, base_sha).await?;
        ensure_commit_available(&self.worktree_path, old_head_sha).await?;
        ensure_commit_available(&self.worktree_path, new_head_sha).await?;

        let old_entries =
            collect_local_range_commits(&self.worktree_path, base_sha, old_head_sha).await?;
        let new_entries =
            collect_local_range_commits(&self.worktree_path, base_sha, new_head_sha).await?;
        let headers = parse_range_diff_output(
            &run_git_output(
                &self.worktree_path,
                &[
                    "range-diff",
                    "--no-color",
                    "--no-notes",
                    "--creation-factor=999",
                    &format!("{base_sha}...{old_head_sha}"),
                    &format!("{base_sha}...{new_head_sha}"),
                ],
            )
            .await
            .context("executing git range-diff")?,
        );

        let mut commit_pairs = Vec::new();
        for header in headers {
            let old_commit = header
                .old_sha_prefix
                .as_deref()
                .and_then(|prefix| resolve_local_commit(&old_entries, prefix));
            let new_commit = header
                .new_sha_prefix
                .as_deref()
                .and_then(|prefix| resolve_local_commit(&new_entries, prefix));
            let intra_diff = if header.status == CommitPairStatus::Modified {
                match (&old_commit, &new_commit) {
                    (Some(old), Some(new)) => build_intra_diff_from_text(
                        &old.patch_text,
                        &new.patch_text,
                        &old.metadata,
                        &new.metadata,
                    ),
                    _ => None,
                }
            } else {
                None
            };
            commit_pairs.push(CommitPair {
                status: header.status,
                old: old_commit.as_ref().map(|entry| entry.metadata.clone()),
                new: new_commit.as_ref().map(|entry| entry.metadata.clone()),
                intra_diff,
            });
        }

        Ok(RangeDiff {
            old_range: CommitRange {
                base_sha: base_sha.to_string(),
                head_sha: old_head_sha.to_string(),
                commits: old_entries
                    .iter()
                    .map(|entry| entry.metadata.clone())
                    .collect(),
            },
            new_range: CommitRange {
                base_sha: base_sha.to_string(),
                head_sha: new_head_sha.to_string(),
                commits: new_entries
                    .iter()
                    .map(|entry| entry.metadata.clone())
                    .collect(),
            },
            commit_pairs,
            mode: RangeDiffSource::LocalGit,
        })
    }
}

#[async_trait]
impl RangeDiffSourceProvider for RestCompareRangeDiff {
    async fn compute(
        &self,
        account_id: &str,
        repo: &RepoLocator,
        base_sha: &str,
        old_head_sha: &str,
        new_head_sha: &str,
    ) -> Result<RangeDiff> {
        let old_compare = fetch_compare_response(
            self.github.as_ref(),
            account_id,
            repo,
            base_sha,
            old_head_sha,
        )
        .await?;
        let new_compare = fetch_compare_response(
            self.github.as_ref(),
            account_id,
            repo,
            base_sha,
            new_head_sha,
        )
        .await?;

        let old_candidates = fetch_compare_commit_candidates(
            self.github.as_ref(),
            account_id,
            repo,
            old_compare.commits,
        )
        .await?;
        let new_candidates = fetch_compare_commit_candidates(
            self.github.as_ref(),
            account_id,
            repo,
            new_compare.commits,
        )
        .await?;
        let commit_pairs = pair_rest_candidates(&old_candidates, &new_candidates);

        Ok(RangeDiff {
            old_range: CommitRange {
                base_sha: base_sha.to_string(),
                head_sha: old_head_sha.to_string(),
                commits: old_candidates
                    .iter()
                    .map(|candidate| candidate.metadata.clone())
                    .collect(),
            },
            new_range: CommitRange {
                base_sha: base_sha.to_string(),
                head_sha: new_head_sha.to_string(),
                commits: new_candidates
                    .iter()
                    .map(|candidate| candidate.metadata.clone())
                    .collect(),
            },
            commit_pairs,
            mode: RangeDiffSource::RestCompare,
        })
    }
}

#[derive(Debug, Clone)]
struct ParsedRangeDiffHeader {
    status: CommitPairStatus,
    old_sha_prefix: Option<String>,
    new_sha_prefix: Option<String>,
}

#[derive(Debug, Clone)]
struct LocalCommitEntry {
    metadata: CommitMetadata,
    patch_text: String,
}

#[derive(Debug, Clone)]
struct CompareCommitCandidate {
    metadata: CommitMetadata,
    patch_text: String,
    path_set: HashSet<String>,
    touched_line_count: i64,
}

#[derive(Debug, Deserialize)]
struct CompareResponse {
    commits: Vec<CompareCommit>,
}

#[derive(Debug, Deserialize)]
struct CompareCommit {
    sha: String,
}

#[derive(Debug, Deserialize)]
struct CommitDetailsResponse {
    sha: String,
    commit: CommitDetailsCommitNode,
    files: Option<Vec<CommitDetailsFile>>,
}

#[derive(Debug, Deserialize)]
struct CommitDetailsCommitNode {
    message: String,
    author: Option<CommitDetailsAuthorNode>,
}

#[derive(Debug, Deserialize)]
struct CommitDetailsAuthorNode {
    name: Option<String>,
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommitDetailsFile {
    filename: String,
    patch: Option<String>,
    additions: Option<i64>,
    deletions: Option<i64>,
}

async fn ensure_commit_available(worktree_path: &Path, sha: &str) -> Result<()> {
    if has_commit(worktree_path, sha).await? {
        return Ok(());
    }
    let _ = run_git_status(worktree_path, &["fetch", "origin", sha]).await?;
    if has_commit(worktree_path, sha).await? {
        return Ok(());
    }
    Err(RangeDiffError::WorktreeMissingCommits {
        missing_sha: sha.to_string(),
    }
    .into())
}

async fn has_commit(worktree_path: &Path, sha: &str) -> Result<bool> {
    let exit = run_git_status(
        worktree_path,
        &["cat-file", "-e", &format!("{sha}^{{commit}}")],
    )
    .await?;
    Ok(exit == 0)
}

async fn collect_local_range_commits(
    worktree_path: &Path,
    base_sha: &str,
    head_sha: &str,
) -> Result<Vec<LocalCommitEntry>> {
    let rev_list = run_git_output(
        worktree_path,
        &["rev-list", "--reverse", &format!("{base_sha}..{head_sha}")],
    )
    .await?;
    let mut commits = Vec::new();
    for sha in rev_list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        commits.push(read_local_commit_entry(worktree_path, sha).await?);
    }
    Ok(commits)
}

async fn read_local_commit_entry(worktree_path: &Path, sha: &str) -> Result<LocalCommitEntry> {
    let header = run_git_output(
        worktree_path,
        &[
            "show",
            "-s",
            "--format=%H%n%s%n%b%n%an%n%ct",
            "--no-color",
            sha,
        ],
    )
    .await?;
    let mut header_lines = header.lines();
    let commit_sha = header_lines.next().unwrap_or(sha).to_string();
    let title = header_lines.next().unwrap_or_default().to_string();
    let remaining = header_lines.collect::<Vec<_>>();
    let (body, author_name, committed_at) = if remaining.len() >= 2 {
        let committed_at = remaining.last().and_then(|value| value.parse::<i64>().ok());
        let author_name = remaining
            .get(remaining.len().saturating_sub(2))
            .map(|value| value.to_string());
        let body_lines = &remaining[..remaining.len().saturating_sub(2)];
        (body_lines.join("\n"), author_name, committed_at)
    } else {
        (String::new(), None, None)
    };

    let patch_text = read_local_patch_text(worktree_path, sha).await?;
    let numstat = run_git_output(
        worktree_path,
        &["show", "--no-color", "--format=", "--numstat", sha],
    )
    .await?;
    let (file_paths, additions, deletions) = parse_numstat(&numstat);
    Ok(LocalCommitEntry {
        metadata: CommitMetadata {
            sha: commit_sha,
            title,
            body,
            author_name,
            committed_at,
            patch_hash: hash_patch_body(&patch_text),
            file_paths,
            additions,
            deletions,
        },
        patch_text,
    })
}

async fn read_local_patch_text(worktree_path: &Path, sha: &str) -> Result<String> {
    run_git_output(
        worktree_path,
        &["show", "--no-color", "--format=", "--patch", sha],
    )
    .await
}

fn parse_numstat(raw: &str) -> (Vec<String>, i64, i64) {
    let mut file_paths = Vec::new();
    let mut additions = 0_i64;
    let mut deletions = 0_i64;
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split_whitespace();
        let add = parts.next();
        let del = parts.next();
        let file = parts.next();
        let Some(file) = file else {
            continue;
        };
        file_paths.push(file.to_string());
        additions += add
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default();
        deletions += del
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default();
    }
    (file_paths, additions, deletions)
}

fn parse_range_diff_output(raw: &str) -> Vec<ParsedRangeDiffHeader> {
    raw.lines().filter_map(parse_range_diff_header).collect()
}

fn parse_range_diff_header(line: &str) -> Option<ParsedRangeDiffHeader> {
    if line.starts_with(' ') || line.trim().is_empty() {
        return None;
    }
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 5 {
        return None;
    }
    let status_index = tokens
        .iter()
        .position(|token| matches!(*token, "=" | "!" | "<" | ">"))?;
    let status = match tokens[status_index] {
        "=" => CommitPairStatus::Unchanged,
        "!" => CommitPairStatus::Modified,
        "<" => CommitPairStatus::Removed,
        ">" => CommitPairStatus::Added,
        _ => return None,
    };

    let old_sha_prefix = match status {
        CommitPairStatus::Added => None,
        _ => status_index
            .checked_sub(1)
            .and_then(|idx| tokens.get(idx))
            .and_then(|token| normalize_sha_token(token)),
    };
    let new_sha_prefix = match status {
        CommitPairStatus::Removed => None,
        _ => tokens
            .get(status_index + 2)
            .and_then(|token| normalize_sha_token(token)),
    };
    Some(ParsedRangeDiffHeader {
        status,
        old_sha_prefix,
        new_sha_prefix,
    })
}

fn normalize_sha_token(token: &str) -> Option<String> {
    if token == "-------" {
        return None;
    }
    let trimmed = token.trim_end_matches(':');
    if trimmed == "-" {
        return None;
    }
    Some(trimmed.to_string())
}

fn resolve_local_commit(
    entries: &[LocalCommitEntry],
    sha_prefix: &str,
) -> Option<LocalCommitEntry> {
    entries
        .iter()
        .find(|entry| {
            entry.metadata.sha == sha_prefix || entry.metadata.sha.starts_with(sha_prefix)
        })
        .cloned()
}

fn pair_rest_candidates(
    old_candidates: &[CompareCommitCandidate],
    new_candidates: &[CompareCommitCandidate],
) -> Vec<CommitPair> {
    let mut pairs = Vec::new();
    let mut new_by_hash = HashMap::<String, VecDeque<usize>>::new();
    for (index, candidate) in new_candidates.iter().enumerate() {
        new_by_hash
            .entry(candidate.metadata.patch_hash.clone())
            .or_default()
            .push_back(index);
    }

    let mut matched_old = HashSet::<usize>::new();
    let mut matched_new = HashSet::<usize>::new();
    let mut modified_matches = HashMap::<usize, usize>::new();

    for (old_index, old_candidate) in old_candidates.iter().enumerate() {
        let Some(queue) = new_by_hash.get_mut(&old_candidate.metadata.patch_hash) else {
            continue;
        };
        while let Some(new_index) = queue.pop_front() {
            if matched_new.contains(&new_index) {
                continue;
            }
            matched_old.insert(old_index);
            matched_new.insert(new_index);
            pairs.push(CommitPair {
                status: CommitPairStatus::Unchanged,
                old: Some(old_candidate.metadata.clone()),
                new: Some(new_candidates[new_index].metadata.clone()),
                intra_diff: None,
            });
            break;
        }
    }

    loop {
        let mut best: Option<(usize, usize, f64)> = None;
        for (old_index, old_candidate) in old_candidates.iter().enumerate() {
            if matched_old.contains(&old_index) {
                continue;
            }
            for (new_index, new_candidate) in new_candidates.iter().enumerate() {
                if matched_new.contains(&new_index) {
                    continue;
                }
                let score = similarity_score(old_candidate, new_candidate);
                if score < 0.35 {
                    continue;
                }
                match best {
                    Some((_, _, best_score)) if best_score >= score => {}
                    _ => best = Some((old_index, new_index, score)),
                }
            }
        }
        let Some((old_index, new_index, _)) = best else {
            break;
        };
        matched_old.insert(old_index);
        matched_new.insert(new_index);
        modified_matches.insert(old_index, new_index);
    }

    let mut ordered_pairs = Vec::<(usize, CommitPair)>::new();
    let unchanged_pairs = pairs
        .drain(..)
        .map(|pair| {
            let order = pair
                .old
                .as_ref()
                .and_then(|old| {
                    old_candidates
                        .iter()
                        .position(|candidate| candidate.metadata.sha == old.sha)
                })
                .unwrap_or(usize::MAX);
            (order, pair)
        })
        .collect::<Vec<_>>();
    ordered_pairs.extend(unchanged_pairs);

    for (old_index, old_candidate) in old_candidates.iter().enumerate() {
        if let Some(new_index) = modified_matches.get(&old_index).copied() {
            let new_candidate = &new_candidates[new_index];
            ordered_pairs.push((
                old_index,
                CommitPair {
                    status: CommitPairStatus::Modified,
                    old: Some(old_candidate.metadata.clone()),
                    new: Some(new_candidate.metadata.clone()),
                    intra_diff: build_intra_diff_from_text(
                        &old_candidate.patch_text,
                        &new_candidate.patch_text,
                        &old_candidate.metadata,
                        &new_candidate.metadata,
                    ),
                },
            ));
        } else if !matched_old.contains(&old_index) {
            ordered_pairs.push((
                old_index,
                CommitPair {
                    status: CommitPairStatus::Removed,
                    old: Some(old_candidate.metadata.clone()),
                    new: None,
                    intra_diff: None,
                },
            ));
        }
    }

    let mut appended = new_candidates
        .iter()
        .enumerate()
        .filter(|(index, _)| !matched_new.contains(index))
        .map(|(index, candidate)| {
            (
                old_candidates.len() + index,
                CommitPair {
                    status: CommitPairStatus::Added,
                    old: None,
                    new: Some(candidate.metadata.clone()),
                    intra_diff: None,
                },
            )
        })
        .collect::<Vec<_>>();
    ordered_pairs.append(&mut appended);
    ordered_pairs.sort_by_key(|pair| pair.0);
    ordered_pairs.into_iter().map(|(_, pair)| pair).collect()
}

fn similarity_score(left: &CompareCommitCandidate, right: &CompareCommitCandidate) -> f64 {
    let union = left.path_set.union(&right.path_set).count() as f64;
    let intersection = left.path_set.intersection(&right.path_set).count() as f64;
    let jaccard = if union == 0.0 {
        0.0
    } else {
        intersection / union
    };
    let max_lines = left.touched_line_count.max(right.touched_line_count).max(1) as f64;
    let line_similarity =
        1.0 - ((left.touched_line_count - right.touched_line_count).abs() as f64 / max_lines);
    (jaccard + line_similarity) / 2.0
}

async fn fetch_compare_response(
    github: &GithubClient,
    account_id: &str,
    repo: &RepoLocator,
    base_sha: &str,
    head_sha: &str,
) -> Result<CompareResponse> {
    let path = format!(
        "/repos/{}/{}/compare/{}...{}",
        repo.owner, repo.name, base_sha, head_sha
    );
    let (response, _rate_limit) = github
        .rest_get_json::<CompareResponse>(account_id, &path)
        .await
        .with_context(|| format!("fetching compare response for `{path}`"))?;
    Ok(response)
}

async fn fetch_compare_commit_candidates(
    github: &GithubClient,
    account_id: &str,
    repo: &RepoLocator,
    commits: Vec<CompareCommit>,
) -> Result<Vec<CompareCommitCandidate>> {
    let mut candidates = Vec::new();
    for commit in commits {
        let path = format!("/repos/{}/{}/commits/{}", repo.owner, repo.name, commit.sha);
        let (details, _rate_limit) = github
            .rest_get_json::<CommitDetailsResponse>(account_id, &path)
            .await
            .with_context(|| format!("fetching commit details for `{path}`"))?;
        let files = details.files.unwrap_or_default();
        let mut additions = 0_i64;
        let mut deletions = 0_i64;
        let mut file_paths = Vec::new();
        let mut patch_chunks = Vec::new();
        for file in files {
            additions += file.additions.unwrap_or_default();
            deletions += file.deletions.unwrap_or_default();
            file_paths.push(file.filename.clone());
            if let Some(patch) = file.patch {
                patch_chunks.push(format!("diff --git a/{0} b/{0}\n{1}", file.filename, patch));
            }
        }
        let patch_text = patch_chunks.join("\n");
        let (title, body) = split_message(&details.commit.message);
        let committed_at = details
            .commit
            .author
            .as_ref()
            .and_then(|author| author.date.as_deref())
            .and_then(parse_timestamp);
        let author_name = details.commit.author.and_then(|author| author.name);
        let patch_hash = hash_patch_body(&patch_text);
        let path_set = file_paths.iter().cloned().collect::<HashSet<_>>();
        candidates.push(CompareCommitCandidate {
            metadata: CommitMetadata {
                sha: details.sha,
                title,
                body,
                author_name,
                committed_at,
                patch_hash,
                file_paths,
                additions,
                deletions,
            },
            patch_text,
            path_set,
            touched_line_count: additions + deletions,
        });
    }
    Ok(candidates)
}

fn split_message(message: &str) -> (String, String) {
    let mut lines = message.lines();
    let title = lines.next().unwrap_or_default().to_string();
    let body = lines.collect::<Vec<_>>().join("\n");
    (title, body)
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp())
}

fn hash_patch_body(patch: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(patch.as_bytes());
    hex::encode(hasher.finalize())
}

fn build_intra_diff_from_text(
    old_text: &str,
    new_text: &str,
    old_metadata: &CommitMetadata,
    new_metadata: &CommitMetadata,
) -> Option<IntraDiff> {
    if old_text.is_empty() && new_text.is_empty() {
        return None;
    }
    let old_lines = old_text.lines().collect::<Vec<_>>();
    let new_lines = new_text.lines().collect::<Vec<_>>();
    let max_len = old_lines.len().max(new_lines.len());
    let mut old_highlighted = Vec::new();
    let mut new_highlighted = Vec::new();
    for index in 0..max_len {
        let old_line = old_lines.get(index).copied().unwrap_or_default();
        let new_line = new_lines.get(index).copied().unwrap_or_default();
        let (old_segments, new_segments) = diff_line_segments(old_line, new_line);
        old_highlighted.push(HighlightedLine {
            text: old_line.to_string(),
            segments: old_segments,
            side: DiffSide::Old,
        });
        new_highlighted.push(HighlightedLine {
            text: new_line.to_string(),
            segments: new_segments,
            side: DiffSide::New,
        });
    }
    if old_metadata.sha == new_metadata.sha {
        return None;
    }
    Some(IntraDiff {
        hunks: vec![IntraHunk {
            old_lines: old_highlighted,
            new_lines: new_highlighted,
        }],
    })
}

fn diff_line_segments(
    old_line: &str,
    new_line: &str,
) -> (Vec<HighlightSegment>, Vec<HighlightSegment>) {
    let diff = TextDiff::from_chars(old_line, new_line);
    let mut old_cursor = 0_usize;
    let mut new_cursor = 0_usize;
    let mut old_segments = Vec::new();
    let mut new_segments = Vec::new();
    for change in diff.iter_all_changes() {
        let len = change.to_string().chars().count();
        match change.tag() {
            ChangeTag::Equal => {
                if len > 0 {
                    old_segments.push(HighlightSegment {
                        start: old_cursor,
                        end: old_cursor + len,
                        kind: HighlightKind::Unchanged,
                    });
                    new_segments.push(HighlightSegment {
                        start: new_cursor,
                        end: new_cursor + len,
                        kind: HighlightKind::Unchanged,
                    });
                }
                old_cursor += len;
                new_cursor += len;
            }
            ChangeTag::Delete => {
                if len > 0 {
                    old_segments.push(HighlightSegment {
                        start: old_cursor,
                        end: old_cursor + len,
                        kind: HighlightKind::Removed,
                    });
                }
                old_cursor += len;
            }
            ChangeTag::Insert => {
                if len > 0 {
                    new_segments.push(HighlightSegment {
                        start: new_cursor,
                        end: new_cursor + len,
                        kind: HighlightKind::Added,
                    });
                }
                new_cursor += len;
            }
        }
    }
    (old_segments, new_segments)
}

async fn run_git_output(worktree_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .await
        .with_context(|| {
            format!(
                "running git command in `{}`: {}",
                worktree_path.display(),
                args.join(" ")
            )
        })?;
    if !output.status.success() {
        return Err(anyhow!(
            "git command failed (status {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_git_status(worktree_path: &Path, args: &[&str]) -> Result<i32> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .args(args)
        .output()
        .await
        .with_context(|| {
            format!(
                "running git status command in `{}`: {}",
                worktree_path.display(),
                args.join(" ")
            )
        })?;
    Ok(output.status.code().unwrap_or(-1))
}

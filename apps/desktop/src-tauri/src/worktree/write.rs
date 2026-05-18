use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{
    Cred, CredentialType, Direction, Oid, PushOptions, RemoteCallbacks, Repository, Signature,
    StatusOptions,
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum DiffSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AuthorIdentity {
    pub login: String,
    pub name: String,
    pub email: String,
    pub push_token: Option<String>,
    pub remote_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct WorktreeWriteRequest {
    pub pr_id: String,
    pub worktree_path: PathBuf,
    pub expected_branch: String,
    pub expected_head_sha: String,
    pub suggestions: Vec<SuggestionPatch>,
    pub author: AuthorIdentity,
    pub message: String,
    pub force_with_stash: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct SuggestionPatch {
    pub id: String,
    pub path: PathBuf,
    pub start_line: u32,
    pub end_line: u32,
    pub side: DiffSide,
    pub replacement: String,
    pub original: Option<String>,
    pub original_commit_sha: String,
    pub suggestion_author_login: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct WorktreeWriteOutcome {
    pub head_sha_before: String,
    pub head_sha_after: String,
    pub commit_sha: String,
    pub pushed: bool,
    pub dirty_snapshot: Option<PathBuf>,
    pub coauthors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, thiserror::Error, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail")]
pub enum WorktreeWriteError {
    #[error("failed to open worktree repository at `{path}`: {message}")]
    OpenRepo { path: String, message: String },
    #[error("worktree branch mismatch: expected `{expected}`, found `{actual}`")]
    BranchMismatch { expected: String, actual: String },
    #[error("worktree head mismatch: expected `{expected}`, found `{actual}`")]
    HeadMismatch { expected: String, actual: String },
    #[error("worktree is dirty")]
    WorktreeDirty { snapshot_command_hint: String },
    #[error("failed to stash dirty state: {message}")]
    StashFailed { message: String },
    #[error("failed to restore stash: {message}")]
    StashPopFailed { message: String },
    #[error("invalid patch range {start_line}-{end_line} for `{path}`")]
    InvalidRange {
        path: String,
        start_line: u32,
        end_line: u32,
    },
    #[error("failed reading `{path}`: {message}")]
    ReadFailed { path: String, message: String },
    #[error("failed writing `{path}`: {message}")]
    WriteFailed { path: String, message: String },
    #[error("suggestion `{suggestion_id}` conflicts for `{path}`: {message}")]
    SuggestionConflict {
        suggestion_id: String,
        path: String,
        message: String,
    },
    #[error("no suggestions selected")]
    NoSuggestions,
    #[error("failed staging files: {message}")]
    StageFailed { message: String },
    #[error("failed creating commit: {message}")]
    CommitFailed { message: String },
    #[error("remote `{remote}` not found")]
    RemoteNotFound { remote: String },
    #[error("push rejected; remote head is `{remote_head}`")]
    PushRejected { remote_head: String },
    #[error("push failed: {message}")]
    PushFailed { message: String },
    #[error("failed to rollback local commit: {message}")]
    RollbackFailed { message: String },
}

pub trait WorktreeWriter: Send + Sync {
    fn run(&self, req: WorktreeWriteRequest) -> Result<WorktreeWriteOutcome, WorktreeWriteError>;
}

pub trait WorktreeWriteEventEmitter: Send + Sync {
    fn emit_step(&self, pr_id: &str, step: &str);
}

static WORKTREE_WRITE_EVENTS: OnceCell<Arc<dyn WorktreeWriteEventEmitter>> = OnceCell::new();

pub fn install_worktree_write_event_emitter(emitter: Arc<dyn WorktreeWriteEventEmitter>) {
    let _ = WORKTREE_WRITE_EVENTS.set(emitter);
}

fn emit_step(pr_id: &str, step: &str) {
    if let Some(emitter) = WORKTREE_WRITE_EVENTS.get() {
        emitter.emit_step(pr_id, step);
    }
}

#[derive(Debug, Default, Clone)]
pub struct Git2WorktreeWriter;

impl Git2WorktreeWriter {
    fn stage_files(
        repo: &Repository,
        worktree_path: &Path,
        paths: &BTreeSet<PathBuf>,
    ) -> Result<(), WorktreeWriteError> {
        let mut index = repo
            .index()
            .map_err(|error| WorktreeWriteError::StageFailed {
                message: error.to_string(),
            })?;
        for path in paths {
            let relative = relative_path(worktree_path, path).map_err(|error| {
                WorktreeWriteError::StageFailed {
                    message: error.to_string(),
                }
            })?;
            index
                .add_path(&relative)
                .map_err(|error| WorktreeWriteError::StageFailed {
                    message: error.to_string(),
                })?;
        }
        index
            .write()
            .map_err(|error| WorktreeWriteError::StageFailed {
                message: error.to_string(),
            })?;
        Ok(())
    }
}

impl WorktreeWriter for Git2WorktreeWriter {
    fn run(&self, req: WorktreeWriteRequest) -> Result<WorktreeWriteOutcome, WorktreeWriteError> {
        emit_step(&req.pr_id, "opened");
        let repo =
            Repository::open(&req.worktree_path).map_err(|error| WorktreeWriteError::OpenRepo {
                path: req.worktree_path.to_string_lossy().to_string(),
                message: error.to_string(),
            })?;
        let head = repo.head().map_err(|error| WorktreeWriteError::OpenRepo {
            path: req.worktree_path.to_string_lossy().to_string(),
            message: error.to_string(),
        })?;
        let branch = head.shorthand().unwrap_or("detached").to_string();
        if branch != req.expected_branch {
            return Err(WorktreeWriteError::BranchMismatch {
                expected: req.expected_branch,
                actual: branch,
            });
        }
        let head_oid = head.target().ok_or_else(|| WorktreeWriteError::OpenRepo {
            path: req.worktree_path.to_string_lossy().to_string(),
            message: "HEAD is not a direct commit".to_string(),
        })?;
        let head_before = head_oid.to_string();
        if head_before != req.expected_head_sha {
            return Err(WorktreeWriteError::HeadMismatch {
                expected: req.expected_head_sha,
                actual: head_before,
            });
        }

        let mut dirty_snapshot: Option<PathBuf> = None;
        let mut stash_created = false;
        if worktree_is_dirty(&repo)? {
            if !req.force_with_stash {
                return Err(WorktreeWriteError::WorktreeDirty {
                    snapshot_command_hint: format!(
                        "git -C {} stash push --include-untracked",
                        req.worktree_path.to_string_lossy()
                    ),
                });
            }
            let snapshot = stash_worktree(&req.worktree_path)?;
            stash_created = snapshot.is_some();
            dirty_snapshot = snapshot;
        }

        emit_step(&req.pr_id, "assertions_ok");
        let result = self.run_with_clean_state(&repo, &req, &head_before, dirty_snapshot.clone());

        if stash_created {
            stash_pop(&req.worktree_path)?;
        }
        result
    }
}

impl Git2WorktreeWriter {
    fn run_with_clean_state(
        &self,
        repo: &Repository,
        req: &WorktreeWriteRequest,
        head_before: &str,
        dirty_snapshot: Option<PathBuf>,
    ) -> Result<WorktreeWriteOutcome, WorktreeWriteError> {
        if req.suggestions.is_empty() {
            return Err(WorktreeWriteError::NoSuggestions);
        }

        let mut touched_paths = BTreeSet::<PathBuf>::new();
        let mut coauthors = BTreeSet::<String>::new();
        for suggestion in &req.suggestions {
            apply_suggestion_patch(&req.worktree_path, &req.expected_head_sha, suggestion)?;
            touched_paths.insert(suggestion.path.clone());
            coauthors.insert(suggestion.suggestion_author_login.clone());
        }
        emit_step(&req.pr_id, "patched");

        Self::stage_files(repo, &req.worktree_path, &touched_paths)?;
        let commit_id = commit_suggestions(repo, req, &coauthors)?;
        emit_step(&req.pr_id, "committed");

        let remote_name = req
            .author
            .remote_name
            .as_deref()
            .unwrap_or("origin")
            .to_string();
        let remote_head = fetch_remote_head(
            repo,
            &remote_name,
            &req.expected_branch,
            req.author.push_token.clone(),
        )?;
        if let Some(remote_head) = remote_head {
            if remote_head != head_before {
                reset_hard(repo, head_before)?;
                return Err(WorktreeWriteError::PushRejected { remote_head });
            }
        }

        let push_result = push_branch(
            repo,
            &remote_name,
            &req.expected_branch,
            req.author.push_token.clone(),
        );
        if let Err(error) = push_result {
            let fallback_remote_head = fetch_remote_head(
                repo,
                &remote_name,
                &req.expected_branch,
                req.author.push_token.clone(),
            )?
            .unwrap_or_else(|| "unknown".to_string());
            let _ = reset_hard(repo, head_before);
            if error.contains("non-fast-forward")
                || error.contains("failed to push")
                || error.contains("rejected")
            {
                return Err(WorktreeWriteError::PushRejected {
                    remote_head: fallback_remote_head,
                });
            }
            return Err(WorktreeWriteError::PushFailed { message: error });
        }

        emit_step(&req.pr_id, "pushed");
        Ok(WorktreeWriteOutcome {
            head_sha_before: head_before.to_string(),
            head_sha_after: repo
                .head()
                .ok()
                .and_then(|head| head.target())
                .map(|value| value.to_string())
                .unwrap_or_else(|| commit_id.to_string()),
            commit_sha: commit_id.to_string(),
            pushed: true,
            dirty_snapshot,
            coauthors: coauthors.into_iter().collect(),
        })
    }
}

fn worktree_is_dirty(repo: &Repository) -> Result<bool, WorktreeWriteError> {
    let mut status_options = StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .include_ignored(false);
    let statuses =
        repo.statuses(Some(&mut status_options))
            .map_err(|error| WorktreeWriteError::OpenRepo {
                path: repo.path().to_string_lossy().to_string(),
                message: error.to_string(),
            })?;
    Ok(!statuses.is_empty())
}

fn apply_suggestion_patch(
    worktree_path: &Path,
    expected_head_sha: &str,
    patch: &SuggestionPatch,
) -> Result<(), WorktreeWriteError> {
    if patch.start_line == 0 || patch.end_line < patch.start_line {
        return Err(WorktreeWriteError::InvalidRange {
            path: patch.path.to_string_lossy().to_string(),
            start_line: patch.start_line,
            end_line: patch.end_line,
        });
    }
    let absolute = absolute_path(worktree_path, &patch.path)?;
    let body = fs::read_to_string(&absolute).map_err(|error| WorktreeWriteError::ReadFailed {
        path: absolute.to_string_lossy().to_string(),
        message: error.to_string(),
    })?;
    let had_trailing_newline = body.ends_with('\n');
    let mut lines = body
        .split_terminator('\n')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let start = (patch.start_line - 1) as usize;
    let end = (patch.end_line - 1) as usize;
    if start >= lines.len() || end >= lines.len() {
        return Err(WorktreeWriteError::InvalidRange {
            path: patch.path.to_string_lossy().to_string(),
            start_line: patch.start_line,
            end_line: patch.end_line,
        });
    }
    let existing = lines[start..=end].join("\n");
    if patch.original_commit_sha != expected_head_sha {
        let expected_original = patch.original.as_deref().unwrap_or_default();
        if expected_original.trim() != existing.trim() {
            return Err(WorktreeWriteError::SuggestionConflict {
                suggestion_id: patch.id.clone(),
                path: patch.path.to_string_lossy().to_string(),
                message: "original lines no longer match current worktree".to_string(),
            });
        }
    } else if let Some(expected_original) = patch.original.as_deref() {
        if expected_original.trim() != existing.trim() {
            return Err(WorktreeWriteError::SuggestionConflict {
                suggestion_id: patch.id.clone(),
                path: patch.path.to_string_lossy().to_string(),
                message: "suggestion preimage mismatch".to_string(),
            });
        }
    }

    let replacement_lines = if patch.replacement.is_empty() {
        Vec::<String>::new()
    } else {
        patch
            .replacement
            .split('\n')
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    lines.splice(start..=end, replacement_lines);
    let mut updated = lines.join("\n");
    if !updated.is_empty() && (had_trailing_newline || patch.replacement.ends_with('\n')) {
        updated.push('\n');
    }
    fs::write(&absolute, updated).map_err(|error| WorktreeWriteError::WriteFailed {
        path: absolute.to_string_lossy().to_string(),
        message: error.to_string(),
    })?;
    Ok(())
}

fn commit_suggestions(
    repo: &Repository,
    req: &WorktreeWriteRequest,
    coauthors: &BTreeSet<String>,
) -> Result<Oid, WorktreeWriteError> {
    let mut index = repo
        .index()
        .map_err(|error| WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        })?;
    let tree_oid = index
        .write_tree()
        .map_err(|error| WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        })?;
    let tree = repo
        .find_tree(tree_oid)
        .map_err(|error| WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        })?;
    let parent_commit = repo
        .head()
        .map_err(|error| WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        })?
        .peel_to_commit()
        .map_err(|error| WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        })?;
    let author = Signature::now(&req.author.name, &req.author.email).map_err(|error| {
        WorktreeWriteError::CommitFailed {
            message: error.to_string(),
        }
    })?;

    let mut trailers = Vec::new();
    for login in coauthors {
        trailers.push(format!(
            "Co-authored-by: {login} <{login}@users.noreply.github.com>"
        ));
    }
    let message = if trailers.is_empty() {
        req.message.clone()
    } else {
        format!("{}\n\n{}", req.message, trailers.join("\n"))
    };
    repo.commit(
        Some("HEAD"),
        &author,
        &author,
        &message,
        &tree,
        &[&parent_commit],
    )
    .map_err(|error| WorktreeWriteError::CommitFailed {
        message: error.to_string(),
    })
}

fn fetch_remote_head(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
    push_token: Option<String>,
) -> Result<Option<String>, WorktreeWriteError> {
    let mut remote =
        repo.find_remote(remote_name)
            .map_err(|_| WorktreeWriteError::RemoteNotFound {
                remote: remote_name.to_string(),
            })?;
    let callbacks = remote_callbacks(push_token);
    remote
        .connect_auth(Direction::Fetch, Some(callbacks), None)
        .map_err(|error| WorktreeWriteError::PushFailed {
            message: error.to_string(),
        })?;
    let target = format!("refs/heads/{branch}");
    let mut head = None;
    if let Ok(list) = remote.list() {
        for entry in list {
            if entry.name() == target {
                head = Some(entry.oid().to_string());
                break;
            }
        }
    }
    let _ = remote.disconnect();
    Ok(head)
}

fn push_branch(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
    push_token: Option<String>,
) -> Result<(), String> {
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|error| format!("remote lookup failed: {error}"))?;
    let callbacks = remote_callbacks(push_token);
    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote
        .push(&[refspec.as_str()], Some(&mut push_options))
        .map_err(|error| error.to_string())
}

fn remote_callbacks(push_token: Option<String>) -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, username_from_url, allowed| {
        if let Some(token) = push_token.as_deref() {
            let username = username_from_url.unwrap_or("x-access-token");
            return Cred::userpass_plaintext(username, token);
        }
        if allowed.contains(CredentialType::SSH_KEY) {
            let user = username_from_url.unwrap_or("git");
            return Cred::ssh_key_from_agent(user);
        }
        Cred::default()
    });
    callbacks
}

fn reset_hard(repo: &Repository, sha: &str) -> Result<(), WorktreeWriteError> {
    let oid = Oid::from_str(sha).map_err(|error| WorktreeWriteError::RollbackFailed {
        message: error.to_string(),
    })?;
    let object =
        repo.find_object(oid, None)
            .map_err(|error| WorktreeWriteError::RollbackFailed {
                message: error.to_string(),
            })?;
    repo.reset(&object, git2::ResetType::Hard, None)
        .map_err(|error| WorktreeWriteError::RollbackFailed {
            message: error.to_string(),
        })
}

fn stash_worktree(worktree_path: &Path) -> Result<Option<PathBuf>, WorktreeWriteError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("stash")
        .arg("push")
        .arg("--include-untracked")
        .arg("-m")
        .arg("pr-cockpit:apply-suggestion-batch")
        .output()
        .map_err(|error| WorktreeWriteError::StashFailed {
            message: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(WorktreeWriteError::StashFailed {
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.contains("No local changes to save") {
        return Ok(None);
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let snapshot_path = worktree_path.join(format!(".git/pr-cockpit-stash-{now}.txt"));
    fs::write(&snapshot_path, stdout).map_err(|error| WorktreeWriteError::StashFailed {
        message: error.to_string(),
    })?;
    Ok(Some(snapshot_path))
}

fn stash_pop(worktree_path: &Path) -> Result<(), WorktreeWriteError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("stash")
        .arg("pop")
        .arg("--index")
        .output()
        .map_err(|error| WorktreeWriteError::StashPopFailed {
            message: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(WorktreeWriteError::StashPopFailed {
            message: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }
    Ok(())
}

fn absolute_path(root: &Path, path: &Path) -> Result<PathBuf, WorktreeWriteError> {
    if path.is_absolute() {
        if path.starts_with(root) {
            return Ok(path.to_path_buf());
        }
        return Err(WorktreeWriteError::ReadFailed {
            path: path.to_string_lossy().to_string(),
            message: "path escapes worktree root".to_string(),
        });
    }
    Ok(root.join(path))
}

fn relative_path(root: &Path, path: &Path) -> Result<PathBuf, anyhow::Error> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let relative = absolute.strip_prefix(root)?;
    Ok(relative.to_path_buf())
}

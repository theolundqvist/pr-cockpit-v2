pub mod discovery;
pub mod mapping;
pub mod watcher;

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use self::mapping::{
    best_mapping, parse_repo_slug, PullRequestCandidate, RealMappingCommandRunner,
    WorktreeMappingInput,
};
use self::watcher::{
    refresh_worktree_state, WatchBackend, WorktreeRefreshEvent, WorktreeState, WorktreeWatchTarget,
    WorktreeWatcher,
};
use crate::db::{
    BlobKind, Db, PullRequestMappingCandidateRow, RepoOwnerNameRow, WorktreeRecord, WorktreeViewRow,
};

const DEFAULT_WORKTREE_ROOTS: &[&str] = &["dev", "code", "src", "repos"];

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct WorktreeView {
    pub id: String,
    pub account_id: String,
    pub repo_id: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub path: String,
    pub head_sha: String,
    pub branch: String,
    pub dirty: bool,
    pub ahead: i64,
    pub behind: i64,
    pub untracked_count: i64,
    pub staged_count: i64,
    pub modified_count: i64,
    pub mapped_pr_id: Option<String>,
    pub mapped_pr_number: Option<i64>,
    pub mapping_confidence: Option<f64>,
    pub mapping_source: Option<String>,
    pub is_app_managed: bool,
    pub manual_override_pr_id: Option<String>,
    pub manual_override_at: Option<i64>,
    pub last_cleanup_snapshot_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RediscoverSummary {
    pub roots: Vec<String>,
    pub discovered: i64,
    pub watched: i64,
    pub skipped_unmapped: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CleanupOutcome {
    pub worktree_id: String,
    pub snapshot_id: Option<String>,
    pub blocked_reason: Option<String>,
    pub dirty_detected: bool,
    pub would_remove: bool,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "kind", content = "detail")]
pub enum CleanupError {
    NotFound,
    UserManaged,
    DirtyState,
    ForceRequired,
    SnapshotFailed(String),
}

pub trait WorktreeEventEmitter: Send + Sync {
    fn emit_worktree_changed(&self, worktree_id: &str);
    fn emit_worktree_discovery_completed(&self, summary: &RediscoverSummary);
}

pub struct NoopWorktreeEventEmitter;

impl WorktreeEventEmitter for NoopWorktreeEventEmitter {
    fn emit_worktree_changed(&self, _worktree_id: &str) {}

    fn emit_worktree_discovery_completed(&self, _summary: &RediscoverSummary) {}
}

struct ActiveWatcher {
    _watcher: WorktreeWatcher,
    refresh_task: JoinHandle<()>,
}

struct WorktreeRuntimeState {
    active: Option<ActiveWatcher>,
}

pub struct WorktreeService {
    db: Arc<Db>,
    emitter: Arc<dyn WorktreeEventEmitter>,
    watcher_backend: WatchBackend,
    state: Mutex<WorktreeRuntimeState>,
}

impl WorktreeService {
    pub fn new(
        db: Arc<Db>,
        emitter: Arc<dyn WorktreeEventEmitter>,
        watcher_backend: WatchBackend,
    ) -> Self {
        Self {
            db,
            emitter,
            watcher_backend,
            state: Mutex::new(WorktreeRuntimeState { active: None }),
        }
    }

    pub async fn start(&self) -> Result<RediscoverSummary> {
        self.rediscover_worktrees().await
    }

    pub async fn list_worktrees(&self, account_id: &str) -> Result<Vec<WorktreeView>> {
        let rows = self.db.list_worktrees(account_id).await?;
        Ok(rows.into_iter().map(worktree_view_from_row).collect())
    }

    pub async fn list_worktree_roots(&self) -> Result<Vec<String>> {
        match self.db.list_worktree_roots().await? {
            Some(roots) => Ok(roots),
            None => Ok(default_worktree_roots()),
        }
    }

    pub async fn set_worktree_roots(&self, roots: Vec<String>) -> Result<Vec<String>> {
        let normalized = normalize_root_inputs(roots)?;
        let now = now_epoch_seconds()?;
        self.db.set_worktree_roots(&normalized, now).await?;
        Ok(normalized)
    }

    pub async fn set_worktree_manual_override(
        &self,
        worktree_id: &str,
        pr_id: Option<&str>,
    ) -> Result<()> {
        let now = now_epoch_seconds()?;
        self.db
            .set_worktree_manual_override(worktree_id, pr_id, now)
            .await?;
        if let Some(worktree) = self.db.worktree_by_id(worktree_id).await? {
            apply_mapping_for_worktree_row(&self.db, &worktree).await?;
        }
        self.emitter.emit_worktree_changed(worktree_id);
        Ok(())
    }

    pub async fn rediscover_worktrees(&self) -> Result<RediscoverSummary> {
        let roots = self.list_worktree_roots().await?;
        let root_paths = roots.iter().map(PathBuf::from).collect::<Vec<_>>();
        let discovered = discovery::discover(&root_paths).await;
        let discovered_count = discovered.len();
        let repo_rows = self.db.list_repo_owner_name_rows().await?;

        let mut watch_targets = Vec::<WorktreeWatchTarget>::new();
        let mut skipped_unmapped = 0_i64;
        let now = now_epoch_seconds()?;

        for discovered_worktree in discovered {
            let Some(repo_row) = select_repo_row(&repo_rows, &discovered_worktree) else {
                skipped_unmapped += 1;
                continue;
            };

            let path_str = discovered_worktree.path.to_string_lossy().to_string();
            let existing = self
                .db
                .worktree_by_account_path(&repo_row.account_id, &path_str)
                .await?;
            let state = refresh_worktree_state(&discovered_worktree.path)
                .await
                .unwrap_or_else(|_| fallback_worktree_state(&discovered_worktree));

            let mut manual_override_pr_id = existing
                .as_ref()
                .and_then(|row| row.manual_override_pr_id.clone());
            let mut manual_override_at = existing.as_ref().and_then(|row| row.manual_override_at);

            if let Some(mapped_pr_number) = discovered_worktree.override_config.mapped_pr {
                if let Some(pr_id) = self
                    .db
                    .resolve_pr_id_by_number(
                        &repo_row.account_id,
                        &repo_row.repo_id,
                        mapped_pr_number,
                    )
                    .await?
                {
                    manual_override_pr_id = Some(pr_id);
                    manual_override_at = Some(now);
                }
            }

            let record = WorktreeRecord {
                id: existing
                    .as_ref()
                    .map(|row| row.id.clone())
                    .unwrap_or_else(|| worktree_id(&repo_row.account_id, &path_str)),
                account_id: repo_row.account_id.clone(),
                repo_id: repo_row.repo_id.clone(),
                path: path_str,
                head_sha: state.head_sha.clone(),
                branch: state.branch.clone(),
                dirty: state.dirty,
                ahead: state.ahead,
                behind: state.behind,
                mapped_pr_id: existing.as_ref().and_then(|row| row.mapped_pr_id.clone()),
                mapping_confidence: existing.as_ref().and_then(|row| row.mapping_confidence),
                mapping_source: existing.as_ref().and_then(|row| row.mapping_source.clone()),
                is_app_managed: false,
                manual_override_pr_id,
                manual_override_at,
                last_cleanup_snapshot_id: existing
                    .as_ref()
                    .and_then(|row| row.last_cleanup_snapshot_id.clone()),
                untracked_count: state.untracked_count,
                staged_count: state.staged_count,
                modified_count: state.modified_count,
                created_at: existing.as_ref().map_or(now, |row| row.created_at),
                updated_at: now,
            };
            self.db.upsert_worktree(&record).await?;
            if let Some(worktree_row) = self.db.worktree_by_id(&record.id).await? {
                apply_mapping_for_worktree_row(&self.db, &worktree_row).await?;
            }
            watch_targets.push(WorktreeWatchTarget {
                worktree_id: record.id.clone(),
                path: discovered_worktree.path,
            });
            self.emitter.emit_worktree_changed(&record.id);
        }

        let watched_count = watch_targets.len();
        self.restart_watchers(watch_targets).await?;

        let summary = RediscoverSummary {
            roots,
            discovered: i64::try_from(discovered_count).unwrap_or(i64::MAX),
            watched: i64::try_from(watched_count).unwrap_or(i64::MAX),
            skipped_unmapped,
            updated_at: now,
        };
        self.emitter.emit_worktree_discovery_completed(&summary);
        Ok(summary)
    }

    pub async fn cleanup_worktree(
        &self,
        worktree_id: &str,
        force: bool,
    ) -> std::result::Result<CleanupOutcome, CleanupError> {
        let Some(row) = self
            .db
            .worktree_by_id(worktree_id)
            .await
            .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?
        else {
            return Err(CleanupError::NotFound);
        };

        if row.is_app_managed == 0 {
            return Err(CleanupError::UserManaged);
        }

        let state = refresh_worktree_state(Path::new(&row.path))
            .await
            .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?;
        let dirty_detected =
            state.untracked_count > 0 || state.staged_count > 0 || state.modified_count > 0;

        if dirty_detected && !force {
            return Err(CleanupError::DirtyState);
        }
        if !force {
            return Err(CleanupError::ForceRequired);
        }

        let snapshot_id = write_cleanup_snapshot(&self.db, &row.path).await?;
        let now =
            now_epoch_seconds().map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?;
        self.db
            .set_worktree_cleanup_snapshot(worktree_id, &snapshot_id, now)
            .await
            .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?;

        if dirty_detected {
            return Ok(CleanupOutcome {
                worktree_id: worktree_id.to_string(),
                snapshot_id: Some(snapshot_id),
                blocked_reason: Some("dirty_state".to_string()),
                dirty_detected: true,
                would_remove: false,
                removed: false,
            });
        }

        Ok(CleanupOutcome {
            worktree_id: worktree_id.to_string(),
            snapshot_id: Some(snapshot_id),
            blocked_reason: Some("m3_write_surface_disabled".to_string()),
            dirty_detected: false,
            would_remove: true,
            removed: false,
        })
    }

    async fn restart_watchers(&self, targets: Vec<WorktreeWatchTarget>) -> Result<()> {
        let (refresh_tx, mut refresh_rx) = mpsc::channel::<WorktreeRefreshEvent>(256);
        let watcher = WorktreeWatcher::spawn(targets, self.watcher_backend.clone(), refresh_tx)?;
        let db = Arc::clone(&self.db);
        let emitter = Arc::clone(&self.emitter);
        let refresh_task = tokio::spawn(async move {
            while let Some(event) = refresh_rx.recv().await {
                if let Err(error) = apply_refresh_event(&db, &event).await {
                    tracing::warn!(
                        target: "worktree",
                        worktree_id = %event.worktree_id,
                        error = %error,
                        "failed to apply worktree refresh event"
                    );
                    continue;
                }
                if let Ok(Some(row)) = db.worktree_by_id(&event.worktree_id).await {
                    let _ = apply_mapping_for_worktree_row(&db, &row).await;
                }
                emitter.emit_worktree_changed(&event.worktree_id);
            }
        });

        let mut state = self.state.lock().await;
        if let Some(active) = state.active.take() {
            active.refresh_task.abort();
            drop(active._watcher);
        }
        state.active = Some(ActiveWatcher {
            _watcher: watcher,
            refresh_task,
        });
        Ok(())
    }
}

async fn apply_refresh_event(db: &Db, event: &WorktreeRefreshEvent) -> Result<()> {
    let Some(row) = db.worktree_by_id(&event.worktree_id).await? else {
        return Ok(());
    };
    let now = now_epoch_seconds()?;
    let record = WorktreeRecord {
        id: row.id.clone(),
        account_id: row.account_id.clone(),
        repo_id: row.repo_id.clone(),
        path: row.path.clone(),
        head_sha: event.state.head_sha.clone(),
        branch: event.state.branch.clone(),
        dirty: event.state.dirty,
        ahead: event.state.ahead,
        behind: event.state.behind,
        mapped_pr_id: row.mapped_pr_id.clone(),
        mapping_confidence: row.mapping_confidence,
        mapping_source: row.mapping_source.clone(),
        is_app_managed: row.is_app_managed == 1,
        manual_override_pr_id: row.manual_override_pr_id.clone(),
        manual_override_at: row.manual_override_at,
        last_cleanup_snapshot_id: row.last_cleanup_snapshot_id.clone(),
        untracked_count: event.state.untracked_count,
        staged_count: event.state.staged_count,
        modified_count: event.state.modified_count,
        created_at: row.created_at,
        updated_at: now,
    };
    db.upsert_worktree(&record).await?;
    Ok(())
}

fn worktree_view_from_row(row: WorktreeViewRow) -> WorktreeView {
    WorktreeView {
        id: row.id,
        account_id: row.account_id,
        repo_id: row.repo_id,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        path: row.path,
        head_sha: row.head_sha,
        branch: row.branch,
        dirty: row.dirty == 1,
        ahead: row.ahead,
        behind: row.behind,
        untracked_count: row.untracked_count,
        staged_count: row.staged_count,
        modified_count: row.modified_count,
        mapped_pr_id: row.mapped_pr_id,
        mapped_pr_number: row.mapped_pr_number,
        mapping_confidence: row.mapping_confidence,
        mapping_source: row.mapping_source,
        is_app_managed: row.is_app_managed == 1,
        manual_override_pr_id: row.manual_override_pr_id,
        manual_override_at: row.manual_override_at,
        last_cleanup_snapshot_id: row.last_cleanup_snapshot_id,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn default_worktree_roots() -> Vec<String> {
    let Some(home_dir) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    DEFAULT_WORKTREE_ROOTS
        .iter()
        .map(|segment| home_dir.join(segment).to_string_lossy().to_string())
        .collect()
}

fn normalize_root_inputs(inputs: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = BTreeSet::<String>::new();
    for input in inputs {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }
        let expanded = expand_tilde(trimmed);
        let absolute = if expanded.is_absolute() {
            expanded
        } else {
            std::env::current_dir()
                .context("reading current working directory for root normalization")?
                .join(expanded)
        };
        let clean = normalize_path_components(&absolute);
        let canonical = std::fs::canonicalize(&clean).unwrap_or(clean);
        normalized.insert(canonical.to_string_lossy().to_string());
    }
    Ok(normalized.into_iter().collect())
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(value)
}

fn normalize_path_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn select_repo_row(
    repo_rows: &[RepoOwnerNameRow],
    discovered: &discovery::DiscoveredWorktree,
) -> Option<RepoOwnerNameRow> {
    let override_slug = discovered
        .override_config
        .repo
        .as_deref()
        .and_then(parse_repo_slug);
    let derived_slug = discovered
        .origin_remote_url
        .as_deref()
        .and_then(parse_repo_slug);
    let slug = override_slug.or(derived_slug)?;
    repo_rows
        .iter()
        .find(|row| row.owner == slug.owner && row.name == slug.name)
        .cloned()
}

fn fallback_worktree_state(discovered: &discovery::DiscoveredWorktree) -> WorktreeState {
    WorktreeState {
        branch: discovered
            .branch
            .clone()
            .unwrap_or_else(|| "detached".to_string()),
        head_sha: discovered.head_sha.clone().unwrap_or_default(),
        dirty: false,
        ahead: 0,
        behind: 0,
        untracked_count: 0,
        staged_count: 0,
        modified_count: 0,
    }
}

async fn apply_mapping_for_worktree_row(db: &Db, row: &WorktreeViewRow) -> Result<()> {
    let candidates_rows = db
        .list_pull_request_mapping_candidates(&row.account_id, &row.repo_id)
        .await?;
    let candidates = candidates_rows
        .into_iter()
        .map(candidate_from_row)
        .collect::<Vec<_>>();
    let mapping_input = WorktreeMappingInput {
        worktree_path: PathBuf::from(&row.path),
        branch: row.branch.clone(),
        head_sha: row.head_sha.clone(),
        manual_override_pr_id: row.manual_override_pr_id.clone(),
    };
    let runner = RealMappingCommandRunner;
    let outcome = best_mapping(&runner, &mapping_input, &candidates).await;
    let now = now_epoch_seconds()?;
    db.set_worktree_mapping(
        &row.id,
        outcome.mapped_pr_id.as_deref(),
        Some(outcome.mapping_confidence),
        Some(outcome.mapping_source.as_str()),
        now,
    )
    .await?;
    Ok(())
}

fn candidate_from_row(row: PullRequestMappingCandidateRow) -> PullRequestCandidate {
    PullRequestCandidate {
        id: row.id,
        number: row.number,
        repo_owner: row.repo_owner,
        repo_name: row.repo_name,
        head_ref: row.head_ref,
        head_sha: row.head_sha,
        head_repo_owner: row.head_repo_owner,
    }
}

async fn write_cleanup_snapshot(
    db: &Db,
    worktree_path: &str,
) -> std::result::Result<String, CleanupError> {
    let status_output = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("status")
        .arg("--porcelain=v2")
        .arg("--branch")
        .output()
        .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?;
    let files = list_files_recursive(Path::new(worktree_path));
    let snapshot_payload = serde_json::json!({
        "captured_at": now_epoch_seconds().map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?,
        "worktree_path": worktree_path,
        "status_porcelain_v2_branch": String::from_utf8_lossy(&status_output.stdout),
        "status_stderr": String::from_utf8_lossy(&status_output.stderr),
        "files": files,
    });
    let encoded = serde_json::to_vec_pretty(&snapshot_payload)
        .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))?;
    db.blob_store()
        .put(&encoded, BlobKind::Asset)
        .await
        .map_err(|error| CleanupError::SnapshotFailed(error.to_string()))
}

fn list_files_recursive(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate_path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if candidate_path.file_name().and_then(|name| name.to_str()) == Some(".git") {
                continue;
            }
            if file_type.is_dir() {
                stack.push(candidate_path);
            } else if file_type.is_file() {
                if let Ok(relative) = candidate_path.strip_prefix(root) {
                    files.push(relative.to_string_lossy().to_string());
                }
            }
        }
    }
    files.sort();
    files
}

fn worktree_id(account_id: &str, path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(account_id.as_bytes());
    hasher.update(b":");
    hasher.update(path.as_bytes());
    format!("worktree:{}", hex::encode(hasher.finalize()))
}

fn now_epoch_seconds() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(duration.as_secs()).context("unix timestamp overflow")
}

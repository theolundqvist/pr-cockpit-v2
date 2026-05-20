use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use notify::{Config, PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

#[derive(Debug, Clone)]
pub struct WorktreeWatchTarget {
    pub worktree_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct WorktreeRefreshEvent {
    pub worktree_id: String,
    pub path: PathBuf,
    pub state: WorktreeState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeState {
    pub branch: String,
    pub head_sha: String,
    pub dirty: bool,
    pub ahead: i64,
    pub behind: i64,
    pub untracked_count: i64,
    pub staged_count: i64,
    pub modified_count: i64,
}

#[derive(Debug, Clone, Default)]
pub enum WatchBackend {
    #[default]
    Recommended,
    Poll {
        interval: Duration,
    },
}

enum WatcherHandle {
    Recommended(RecommendedWatcher),
    Poll(PollWatcher),
}

impl WatcherHandle {
    fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Recommended(watcher) => watcher.watch(path, recursive_mode),
            Self::Poll(watcher) => watcher.watch(path, recursive_mode),
        }
    }
}

#[derive(Debug, Clone)]
struct RawWorktreeEvent {
    worktree_id: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct PendingRefresh {
    path: PathBuf,
    due_at: Instant,
}

pub struct WorktreeWatcher {
    _handles: Vec<WatcherHandle>,
    debounce_task: JoinHandle<()>,
}

impl WorktreeWatcher {
    pub fn spawn(
        targets: Vec<WorktreeWatchTarget>,
        backend: WatchBackend,
        refresh_tx: mpsc::Sender<WorktreeRefreshEvent>,
    ) -> Result<Self> {
        let (raw_tx, mut raw_rx) = mpsc::channel::<RawWorktreeEvent>(512);
        let mut handles = Vec::new();

        for target in targets {
            let target_raw_tx = raw_tx.clone();
            let target_clone = target.clone();
            let callback = move |_event: notify::Result<notify::Event>| {
                let _ = target_raw_tx.try_send(RawWorktreeEvent {
                    worktree_id: target_clone.worktree_id.clone(),
                    path: target_clone.path.clone(),
                });
            };

            let mut watcher = create_watcher(callback, &backend)?;
            register_watch_paths(&mut watcher, &target.path)?;
            handles.push(watcher);
        }
        drop(raw_tx);

        let debounce_task = tokio::spawn(async move {
            let mut pending = HashMap::<String, PendingRefresh>::new();

            loop {
                if pending.is_empty() {
                    match raw_rx.recv().await {
                        Some(raw) => {
                            let delay = debounce_delay_for(&raw.path);
                            pending.insert(
                                raw.worktree_id.clone(),
                                PendingRefresh {
                                    path: raw.path,
                                    due_at: Instant::now() + delay,
                                },
                            );
                        }
                        None => break,
                    }
                    continue;
                }

                let next_due = pending
                    .values()
                    .map(|entry| entry.due_at)
                    .min()
                    .expect("pending is not empty");
                tokio::select! {
                    maybe_raw = raw_rx.recv() => {
                        match maybe_raw {
                            Some(raw) => {
                                let delay = debounce_delay_for(&raw.path);
                                pending.insert(
                                    raw.worktree_id.clone(),
                                    PendingRefresh {
                                        path: raw.path,
                                        due_at: Instant::now() + delay,
                                    },
                                );
                            }
                            None => {
                                let now = Instant::now();
                                let due_ids = pending
                                    .iter()
                                    .filter_map(|(id, entry)| (entry.due_at <= now).then_some(id.clone()))
                                    .collect::<Vec<_>>();
                                for due_id in due_ids {
                                    let Some(entry) = pending.remove(&due_id) else {
                                        continue;
                                    };
                                    if let Ok(state) = refresh_worktree_state(&entry.path).await {
                                        let _ = refresh_tx
                                            .send(WorktreeRefreshEvent {
                                                worktree_id: due_id,
                                                path: entry.path,
                                                state,
                                            })
                                            .await;
                                    }
                                }
                                if pending.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(next_due) => {
                        let now = Instant::now();
                        let due_ids = pending
                            .iter()
                            .filter_map(|(id, entry)| (entry.due_at <= now).then_some(id.clone()))
                            .collect::<Vec<_>>();
                        for due_id in due_ids {
                            let Some(entry) = pending.remove(&due_id) else {
                                continue;
                            };
                            if let Ok(state) = refresh_worktree_state(&entry.path).await {
                                let _ = refresh_tx
                                    .send(WorktreeRefreshEvent {
                                        worktree_id: due_id,
                                        path: entry.path,
                                        state,
                                    })
                                    .await;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            _handles: handles,
            debounce_task,
        })
    }
}

impl Drop for WorktreeWatcher {
    fn drop(&mut self) {
        self.debounce_task.abort();
    }
}

fn create_watcher<F>(callback: F, backend: &WatchBackend) -> notify::Result<WatcherHandle>
where
    F: FnMut(notify::Result<notify::Event>) + Send + 'static,
{
    match backend {
        WatchBackend::Recommended => {
            notify::recommended_watcher(callback).map(WatcherHandle::Recommended)
        }
        WatchBackend::Poll { interval } => {
            let config = Config::default().with_poll_interval(*interval);
            PollWatcher::new(callback, config).map(WatcherHandle::Poll)
        }
    }
}

fn register_watch_paths(watcher: &mut WatcherHandle, worktree_path: &Path) -> Result<()> {
    let git_dir = resolve_git_dir(worktree_path)?;
    let head_path = git_dir.join("HEAD");
    let refs_path = git_dir.join("refs");
    let index_path = git_dir.join("index");

    if head_path.exists() {
        watcher
            .watch(&head_path, RecursiveMode::NonRecursive)
            .with_context(|| format!("watching {}", head_path.display()))?;
    }
    if refs_path.exists() {
        watcher
            .watch(&refs_path, RecursiveMode::Recursive)
            .with_context(|| format!("watching {}", refs_path.display()))?;
    }
    if index_path.exists() {
        watcher
            .watch(&index_path, RecursiveMode::NonRecursive)
            .with_context(|| format!("watching {}", index_path.display()))?;
    }

    watcher
        .watch(worktree_path, RecursiveMode::NonRecursive)
        .with_context(|| format!("watching {}", worktree_path.display()))?;
    if is_small_worktree(worktree_path) {
        watcher
            .watch(worktree_path, RecursiveMode::Recursive)
            .with_context(|| format!("watching recursively {}", worktree_path.display()))?;
    }
    Ok(())
}

fn is_small_worktree(path: &Path) -> bool {
    const MAX_ENTRIES: usize = 1_000;
    let mut stack = vec![path.to_path_buf()];
    let mut entries_seen = 0usize;
    while let Some(current) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in read_dir.flatten() {
            entries_seen += 1;
            if entries_seen > MAX_ENTRIES {
                return false;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let candidate = entry.path();
                if candidate.file_name().and_then(|name| name.to_str()) == Some(".git") {
                    continue;
                }
                stack.push(candidate);
            }
        }
    }
    true
}

fn debounce_delay_for(path: &Path) -> Duration {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    let millis = 200 + (hasher.finish() % 301);
    Duration::from_millis(millis)
}

fn resolve_git_dir(worktree_path: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(worktree_path)
        .arg("rev-parse")
        .arg("--git-dir")
        .output()
        .with_context(|| format!("resolving git dir for {}", worktree_path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse --git-dir failed for {}",
            worktree_path.display()
        );
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let git_dir = PathBuf::from(raw);
    if git_dir.is_absolute() {
        Ok(git_dir)
    } else {
        Ok(worktree_path.join(git_dir))
    }
}

pub async fn refresh_worktree_state(path: &Path) -> Result<WorktreeState> {
    let status_output = run_git(path, &["status", "--porcelain=v2", "--branch"])
        .await
        .context("running git status --porcelain=v2 --branch")?;
    let mut branch = String::from("detached");
    let mut head_sha = String::new();
    let mut upstream = None::<String>;
    let mut ahead = 0_i64;
    let mut behind = 0_i64;
    let mut untracked_count = 0_i64;
    let mut staged_count = 0_i64;
    let mut modified_count = 0_i64;

    for line in status_output.lines() {
        if let Some(value) = line.strip_prefix("# branch.head ") {
            if value != "(detached)" {
                branch = value.to_string();
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.oid ") {
            if value != "(initial)" {
                head_sha = value.to_string();
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.upstream ") {
            upstream = Some(value.to_string());
            continue;
        }
        if let Some(value) = line.strip_prefix("# branch.ab +") {
            if let Some((ahead_value, behind_value)) = value.split_once(" -") {
                ahead = ahead_value.parse::<i64>().unwrap_or(0);
                behind = behind_value.parse::<i64>().unwrap_or(0);
            }
            continue;
        }
        if let Some(status) = parse_status_pair(line) {
            if status.0 != '.' {
                staged_count += 1;
            }
            if status.1 != '.' {
                modified_count += 1;
            }
            continue;
        }
        if line.starts_with("? ") {
            untracked_count += 1;
        }
    }

    if !head_sha.is_empty() && upstream.is_some() && (ahead == 0 && behind == 0) {
        if let Some(upstream_ref) = upstream.as_deref() {
            if let Some((next_ahead, next_behind)) = rev_list_ahead_behind(path, upstream_ref)
                .await
                .unwrap_or(None)
            {
                ahead = next_ahead;
                behind = next_behind;
            }
        }
    }

    if head_sha.is_empty() {
        head_sha = run_git(path, &["rev-parse", "HEAD"])
            .await
            .unwrap_or_default();
    }

    Ok(WorktreeState {
        branch,
        head_sha: head_sha.trim().to_string(),
        dirty: untracked_count > 0 || staged_count > 0 || modified_count > 0,
        ahead,
        behind,
        untracked_count,
        staged_count,
        modified_count,
    })
}

fn parse_status_pair(line: &str) -> Option<(char, char)> {
    if !(line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ")) {
        return None;
    }
    let mut parts = line.split_whitespace();
    let _kind = parts.next()?;
    let xy = parts.next()?;
    let mut chars = xy.chars();
    Some((chars.next()?, chars.next()?))
}

async fn rev_list_ahead_behind(path: &Path, upstream: &str) -> Result<Option<(i64, i64)>> {
    let range = format!("{upstream}...HEAD");
    let output = run_git(path, &["rev-list", "--left-right", "--count", &range]).await?;
    let mut parts = output.split_whitespace();
    let behind = parts.next().and_then(|value| value.parse::<i64>().ok());
    let ahead = parts.next().and_then(|value| value.parse::<i64>().ok());
    match (ahead, behind) {
        (Some(ahead), Some(behind)) => Ok(Some((ahead, behind))),
        _ => Ok(None),
    }
}

async fn run_git(path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .await
        .with_context(|| format!("running git {:?} in {}", args, path.display()))?;
    if !output.status.success() {
        anyhow::bail!("git {:?} failed in {}", args, path.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

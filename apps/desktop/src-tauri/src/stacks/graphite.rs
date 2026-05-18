use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct GraphiteVersion {
    pub raw: String,
}

pub fn detect_graphite() -> Option<GraphiteVersion> {
    let output = std::process::Command::new("gt")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    Some(GraphiteVersion { raw })
}

pub async fn run_gt_restack(worktree_path: &Path) -> std::io::Result<std::process::Output> {
    Command::new("gt")
        .arg("restack")
        .current_dir(worktree_path)
        .env("GIT_EDITOR", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

pub async fn run_gt_submit(worktree_path: &Path) -> std::io::Result<std::process::Output> {
    Command::new("gt")
        .arg("submit")
        .arg("--stack")
        .current_dir(worktree_path)
        .env("GIT_EDITOR", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

use std::collections::VecDeque;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::api::GithubClient;

const LOG_STREAM_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct CheckLogStreamRequest<'a> {
    pub account_id: &'a str,
    pub owner: &'a str,
    pub repo: &'a str,
    pub check_run_id: &'a str,
    pub details_url: Option<&'a str>,
    pub tail_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct LogChunk {
    pub kind: String,
    pub text: Option<String>,
    pub details_url: Option<String>,
}

impl LogChunk {
    pub fn chunk(text: String) -> Self {
        Self {
            kind: "chunk".to_string(),
            text: Some(text),
            details_url: None,
        }
    }

    pub fn tail(text: String) -> Self {
        Self {
            kind: "tail".to_string(),
            text: Some(text),
            details_url: None,
        }
    }

    pub fn fallback(details_url: Option<String>) -> Self {
        Self {
            kind: "fallback".to_string(),
            text: None,
            details_url,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            kind: "error".to_string(),
            text: Some(message),
            details_url: None,
        }
    }

    pub fn done() -> Self {
        Self {
            kind: "done".to_string(),
            text: None,
            details_url: None,
        }
    }
}

pub async fn stream_check_run_log<F>(
    github: &GithubClient,
    request: CheckLogStreamRequest<'_>,
    mut emit: F,
) -> Result<()>
where
    F: FnMut(LogChunk) -> Result<()>,
{
    let Some(job_id) = request.details_url.and_then(extract_actions_job_id) else {
        emit(LogChunk::fallback(
            request.details_url.map(ToString::to_string),
        ))?;
        emit(LogChunk::done())?;
        return Ok(());
    };

    let path = format!(
        "/repos/{}/{}/actions/jobs/{job_id}/logs",
        request.owner, request.repo
    );
    let (mut response, _rate_limit) = github
        .rest_get_stream(request.account_id, &path)
        .await
        .with_context(|| format!("fetching check log stream for run {}", request.check_run_id))?;

    let mut tail = TailBuffer::new(request.tail_lines);
    while let Some(bytes) = response.chunk().await? {
        for slice in bytes.chunks(LOG_STREAM_CHUNK_BYTES) {
            let text = String::from_utf8_lossy(slice).to_string();
            tail.push(&text);
            emit(LogChunk::chunk(text))?;
        }
    }

    if let Some(compacted) = tail.finish() {
        emit(LogChunk::tail(compacted))?;
    }
    emit(LogChunk::done())?;
    Ok(())
}

fn extract_actions_job_id(details_url: &str) -> Option<i64> {
    let marker = "/jobs/";
    let start = details_url.find(marker)? + marker.len();
    let tail = &details_url[start..];
    let digits = tail
        .chars()
        .take_while(|value| value.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok()
}

struct TailBuffer {
    max_lines: usize,
    lines: VecDeque<String>,
    partial: String,
}

impl TailBuffer {
    fn new(max_lines: usize) -> Self {
        Self {
            max_lines,
            lines: VecDeque::new(),
            partial: String::new(),
        }
    }

    fn push(&mut self, text: &str) {
        if self.max_lines == 0 {
            return;
        }
        self.partial.push_str(text);
        while let Some(index) = self.partial.find('\n') {
            let line = self.partial[..index].to_string();
            self.lines.push_back(line);
            while self.lines.len() > self.max_lines {
                let _ = self.lines.pop_front();
            }
            self.partial = self.partial[index + 1..].to_string();
        }
    }

    fn finish(mut self) -> Option<String> {
        if self.max_lines == 0 {
            return None;
        }
        if !self.partial.is_empty() {
            self.lines.push_back(self.partial.clone());
            while self.lines.len() > self.max_lines {
                let _ = self.lines.pop_front();
            }
        }
        if self.lines.is_empty() {
            None
        } else {
            Some(self.lines.into_iter().collect::<Vec<_>>().join("\n"))
        }
    }
}

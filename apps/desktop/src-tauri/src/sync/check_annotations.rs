use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::api::{ApiResource, GithubClient};
use crate::db::{CheckAnnotationAuxRecord, CheckAnnotationRecord, CheckRunSyncStateRow, Db};
use crate::sync::RateLimitBudgeter;

const ANNOTATIONS_PER_PAGE: usize = 100;

#[derive(Debug, Clone)]
pub struct SyncCheckAnnotationsRequest {
    pub account_id: String,
    pub owner: String,
    pub repo: String,
    pub pr_id: String,
    pub check_run_id: String,
    pub check_run_rest_id: i64,
}

#[derive(Debug, Deserialize)]
struct GithubCheckAnnotation {
    #[serde(default)]
    id: Option<serde_json::Value>,
    path: String,
    start_line: i64,
    end_line: i64,
    #[serde(default)]
    start_column: Option<i64>,
    #[serde(default)]
    end_column: Option<i64>,
    annotation_level: String,
    #[serde(default)]
    title: Option<String>,
    message: String,
    #[serde(default)]
    raw_details: Option<String>,
}

pub fn should_sync_annotations(
    previous: Option<&CheckRunSyncStateRow>,
    next_status: &str,
    next_conclusion: Option<&str>,
    next_updated_at: i64,
) -> bool {
    if let Some(previous) = previous {
        if next_updated_at > previous.updated_at {
            return true;
        }
        if transitioned_to_failed_completion(&previous.status, next_status, next_conclusion) {
            return true;
        }
        return false;
    }
    true
}

fn transitioned_to_failed_completion(
    previous_status: &str,
    next_status: &str,
    next_conclusion: Option<&str>,
) -> bool {
    let was_inflight = matches!(
        previous_status.to_ascii_lowercase().as_str(),
        "queued" | "in_progress" | "pending" | "waiting" | "requested"
    );
    if !was_inflight || !next_status.eq_ignore_ascii_case("completed") {
        return false;
    }
    match next_conclusion.map(|value| value.to_ascii_lowercase()) {
        Some(conclusion) => !matches!(conclusion.as_str(), "success" | "neutral" | "skipped"),
        None => false,
    }
}

pub async fn sync_check_annotations(
    db: Arc<Db>,
    github: &GithubClient,
    budgeter: &RateLimitBudgeter,
    request: &SyncCheckAnnotationsRequest,
) -> Result<Vec<CheckAnnotationRecord>> {
    let mut page = 1_i64;
    let mut records = Vec::new();
    loop {
        let path = format!(
            "/repos/{}/{}/check-runs/{}/annotations?per_page={ANNOTATIONS_PER_PAGE}&page={page}",
            request.owner, request.repo, request.check_run_rest_id
        );
        let (page_rows, rate_limit) = github
            .rest_get_json::<Vec<GithubCheckAnnotation>>(&request.account_id, &path)
            .await
            .with_context(|| {
                format!(
                    "fetching check annotations for run {} page {}",
                    request.check_run_id, page
                )
            })?;
        if let Some(snapshot) = rate_limit {
            budgeter
                .record(&request.account_id, ApiResource::Core, snapshot)
                .await?;
        }
        if page_rows.is_empty() {
            break;
        }
        for row in page_rows.iter() {
            let source_id = annotation_source_id(row);
            let annotation_id = format!("{}:{source_id}", request.check_run_id);
            records.push(CheckAnnotationRecord {
                id: annotation_id,
                account_id: request.account_id.clone(),
                check_run_id: request.check_run_id.clone(),
                pr_id: request.pr_id.clone(),
                path: row.path.clone(),
                start_line: row.start_line,
                end_line: row.end_line,
                start_column: row.start_column,
                end_column: row.end_column,
                annotation_level: row.annotation_level.clone(),
                title: row.title.clone(),
                message: row.message.clone(),
                raw_details: row.raw_details.clone(),
            });
        }
        if page_rows.len() < ANNOTATIONS_PER_PAGE {
            break;
        }
        page += 1;
    }

    db.delete_check_annotations_for_run(&request.check_run_id)
        .await?;
    for annotation in &records {
        db.upsert_check_annotation(annotation).await?;
        db.upsert_check_annotation_aux(&CheckAnnotationAuxRecord {
            annotation_id: annotation.id.clone(),
            check_run_id: annotation.check_run_id.clone(),
            account_id: annotation.account_id.clone(),
            pr_id: annotation.pr_id.clone(),
            anchor_line: annotation.start_line,
            anchor_side: "RIGHT".to_string(),
            anchor_path: annotation.path.clone(),
            anchor_signature_hash: anchor_signature_hash(annotation),
        })
        .await?;
    }

    Ok(records)
}

fn annotation_source_id(annotation: &GithubCheckAnnotation) -> String {
    if let Some(value) = annotation.id.as_ref() {
        if let Some(id) = value.as_i64() {
            return id.to_string();
        }
        if let Some(id) = value.as_u64() {
            return id.to_string();
        }
        if let Some(id) = value.as_str() {
            return id.to_string();
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(annotation.path.as_bytes());
    hasher.update(b":");
    hasher.update(annotation.start_line.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(annotation.end_line.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(annotation.message.as_bytes());
    hex::encode(hasher.finalize())
}

fn anchor_signature_hash(annotation: &CheckAnnotationRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(annotation.path.as_bytes());
    hasher.update(b":");
    hasher.update(annotation.start_line.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(annotation.end_line.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(annotation.annotation_level.as_bytes());
    hasher.update(b":");
    hasher.update(annotation.message.as_bytes());
    hex::encode(hasher.finalize())
}

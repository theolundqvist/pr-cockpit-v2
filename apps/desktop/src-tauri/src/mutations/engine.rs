use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use sqlx::FromRow;
use tokio::sync::broadcast;

use super::ipc_types::{
    DrainSummary, MutationEvent, PendingMutationView, SubmitPayload, SubmittedMutation,
};
use super::patch::Patch;
use super::projector::{self, PatchSource};
use super::stub_handlers;
use super::{
    ApplyCtx, ErrorKind, HardConflictDiff, Mutation, MutationKind, OptimismLevel, PredictCtx,
    ReconcileCtx, RollbackCtx, ServerCallShape,
};
use crate::api::GithubClient;
use crate::db::Db;
use crate::sync::{CacheInvalidationEmitter, SyncHandle};

const MAX_NETWORK_RETRIES: i64 = 5;

#[derive(Clone)]
pub struct MutationEngine {
    db: Arc<Db>,
    github: GithubClient,
    handlers: HashMap<MutationKind, Arc<dyn Mutation>>,
    sync_handle: Option<Arc<SyncHandle>>,
    cache_emitter: Option<Arc<dyn CacheInvalidationEmitter>>,
    events_tx: broadcast::Sender<MutationEvent>,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct MutationApplyError {
    pub kind: ErrorKind,
    pub retryable: bool,
    pub http_status: Option<i64>,
    pub hard_conflict: Option<HardConflictDiff>,
    pub message: String,
}

impl MutationApplyError {
    pub fn network(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Network,
            retryable: true,
            http_status: None,
            hard_conflict: None,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>, diff: Option<HardConflictDiff>) -> Self {
        Self {
            kind: ErrorKind::Conflict,
            retryable: false,
            http_status: Some(409),
            hard_conflict: diff,
            message: message.into(),
        }
    }

    pub fn http(status: i64, message: impl Into<String>) -> Self {
        let kind = if status == 401 || status == 403 {
            ErrorKind::Auth
        } else if status == 404 {
            ErrorKind::NotFound
        } else if status == 409 || status == 422 {
            ErrorKind::Conflict
        } else if status == 429 {
            ErrorKind::RateLimited
        } else if (400..500).contains(&status) {
            ErrorKind::Other(format!("http_{status}"))
        } else {
            ErrorKind::Server
        };

        let retryable = matches!(
            kind,
            ErrorKind::RateLimited | ErrorKind::Other(_) | ErrorKind::Server
        );
        Self {
            kind,
            retryable,
            http_status: Some(status),
            hard_conflict: None,
            message: message.into(),
        }
    }
}

impl MutationEngine {
    pub fn new(db: Arc<Db>, github: GithubClient) -> Self {
        let (events_tx, _) = broadcast::channel(512);
        let mut handlers = HashMap::<MutationKind, Arc<dyn Mutation>>::new();
        for handler in stub_handlers::stub_handlers() {
            handlers.insert(handler.kind(), handler);
        }

        Self {
            db,
            github,
            handlers,
            sync_handle: None,
            cache_emitter: None,
            events_tx,
        }
    }

    pub fn with_sync_handle(mut self, sync_handle: Arc<SyncHandle>) -> Self {
        self.sync_handle = Some(sync_handle);
        self
    }

    pub fn with_cache_emitter(mut self, emitter: Arc<dyn CacheInvalidationEmitter>) -> Self {
        self.cache_emitter = Some(emitter);
        self
    }

    pub fn register_handler(&mut self, handler: Arc<dyn Mutation>) {
        self.handlers.insert(handler.kind(), handler);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MutationEvent> {
        self.events_tx.subscribe()
    }

    pub async fn submit(
        &self,
        account_id: &str,
        payload: SubmitPayload,
    ) -> Result<SubmittedMutation> {
        if let Some(existing) = self
            .lookup_by_idempotency_key(&payload.idempotency_key)
            .await?
        {
            let requires_confirmation = existing.optimism_level() == OptimismLevel::None;
            return Ok(SubmittedMutation {
                mutation_id: existing.id,
                deduped: true,
                requires_confirmation,
            });
        }

        let handler = self.handlers.get(&payload.kind).ok_or_else(|| {
            anyhow!(
                "no mutation handler registered for {}",
                payload.kind.as_str()
            )
        })?;

        let mutation_id = new_mutation_id(account_id, &payload.idempotency_key);
        let optimism = handler.optimism();
        let mut predicted = if optimism == OptimismLevel::None {
            super::PredictedEffect {
                forward_patch: Patch::empty(),
                inverse_patch: Patch::empty(),
                id_mappings: Vec::new(),
                server_call: ServerCallShape::None,
            }
        } else {
            handler.predict(&PredictCtx {
                db: &self.db,
                github: &self.github,
                blob_store: self.db.blob_store(),
                account_id,
                mutation_id: mutation_id.as_str(),
                idempotency_key: payload.idempotency_key.as_str(),
                input_json: &payload.input_json,
            })?
        };

        if optimism == OptimismLevel::Cautious {
            predicted.forward_patch.pending_overlay_kind = Some("cautious".to_string());
        } else if optimism == OptimismLevel::Full {
            predicted.forward_patch.pending_overlay_kind = Some("full".to_string());
        }

        let mut tx = self.db.pool().begin().await?;
        sqlx::query(
            "INSERT INTO pending_mutations(
                id, account_id, kind, target_type, target_id, idempotency_key, input_json,
                optimistic_patch_json, inverse_patch_json, status, retries,
                created_at, updated_at, last_error, optimism_level, server_call_json
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', 0, ?10, ?10, NULL, ?11, ?12)",
        )
        .bind(&mutation_id)
        .bind(account_id)
        .bind(payload.kind.as_str())
        .bind(&payload.target_type)
        .bind(&payload.target_id)
        .bind(&payload.idempotency_key)
        .bind(serde_json::to_string(&payload.input_json)?)
        .bind(serde_json::to_string(&predicted.forward_patch)?)
        .bind(serde_json::to_string(&predicted.inverse_patch)?)
        .bind(now_epoch_seconds()?)
        .bind(optimism.as_str())
        .bind(serde_json::to_string(&predicted.server_call)?)
        .execute(tx.as_mut())
        .await?;

        if optimism != OptimismLevel::None {
            projector::apply_patch(
                &mut tx,
                &predicted.forward_patch,
                PatchSource::OptimisticPrediction {
                    mutation_id: mutation_id.clone(),
                },
            )
            .await?;
        }

        tx.commit().await?;

        let view = PendingMutationView {
            id: mutation_id.clone(),
            account_id: account_id.to_string(),
            kind: payload.kind,
            optimism,
            target_type: payload.target_type,
            target_id: payload.target_id,
            status: "pending".to_string(),
            retries: 0,
            created_at: now_epoch_seconds()?,
            updated_at: now_epoch_seconds()?,
            last_error: None,
            pending_overlay: None,
        };
        let _ = self
            .events_tx
            .send(MutationEvent::Submitted { mutation: view });

        Ok(SubmittedMutation {
            mutation_id,
            deduped: false,
            requires_confirmation: optimism == OptimismLevel::None,
        })
    }

    pub async fn drain(&self) -> Result<DrainSummary> {
        let rows = self.pending_rows().await?;
        let mut summary = DrainSummary {
            processed: 0,
            applied: 0,
            reconciled: 0,
            failed: 0,
            rolled_back: 0,
            retried: 0,
        };

        for row in rows {
            summary.processed += 1;
            let outcome = self.process_row(row).await?;
            summary.applied += usize::from(outcome.applied);
            summary.reconciled += usize::from(outcome.reconciled);
            summary.failed += usize::from(outcome.failed);
            summary.rolled_back += usize::from(outcome.rolled_back);
            summary.retried += usize::from(outcome.retried);
        }

        Ok(summary)
    }

    pub async fn retry(&self, mutation_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pending_mutations
             SET status = 'pending', updated_at = ?2
             WHERE id = ?1 AND status IN ('failed', 'pending')",
        )
        .bind(mutation_id)
        .bind(now_epoch_seconds()?)
        .execute(self.db.pool())
        .await?;
        let _ = self.process_by_id(mutation_id).await?;
        Ok(())
    }

    pub async fn discard(&self, mutation_id: &str) -> Result<()> {
        let row = self
            .load_pending_row(mutation_id)
            .await?
            .ok_or_else(|| anyhow!("pending mutation not found: {mutation_id}"))?;
        let kind = MutationKind::from_str(&row.kind)?;
        let handler = self
            .handlers
            .get(&kind)
            .ok_or_else(|| anyhow!("no mutation handler registered for {}", kind.as_str()))?;
        let input_json: serde_json::Value = serde_json::from_str(&row.input_json)?;
        let inverse_patch: Patch = serde_json::from_str(&row.inverse_patch_json)?;

        handler
            .rollback(&RollbackCtx {
                db: &self.db,
                github: &self.github,
                blob_store: self.db.blob_store(),
                account_id: row.account_id.as_str(),
                mutation_id: row.id.as_str(),
                idempotency_key: row.idempotency_key.as_str(),
                input_json: &input_json,
                inverse_patch: &inverse_patch,
            })
            .await?;

        sqlx::query(
            "UPDATE pending_mutations
             SET status = 'discarded', updated_at = ?2, last_error = NULL
             WHERE id = ?1",
        )
        .bind(mutation_id)
        .bind(now_epoch_seconds()?)
        .execute(self.db.pool())
        .await?;

        let _ = self.events_tx.send(MutationEvent::RolledBack {
            mutation_id: mutation_id.to_string(),
        });
        Ok(())
    }

    async fn process_by_id(&self, mutation_id: &str) -> Result<ProcessOutcome> {
        let Some(row) = self.load_pending_row(mutation_id).await? else {
            return Ok(ProcessOutcome::default());
        };
        self.process_row(row).await
    }

    async fn process_row(&self, row: PendingMutationRow) -> Result<ProcessOutcome> {
        if row.status != "pending" {
            return Ok(ProcessOutcome::default());
        }

        let kind = MutationKind::from_str(&row.kind)?;
        let Some(handler) = self.handlers.get(&kind) else {
            self.mark_failed(
                &row.id,
                ErrorKind::Other(format!("missing_handler_{}", row.kind)),
                false,
                None,
                Some("No handler registered".to_string()),
                row.retries + 1,
            )
            .await?;
            return Ok(ProcessOutcome {
                failed: true,
                ..ProcessOutcome::default()
            });
        };

        let input_json: serde_json::Value = serde_json::from_str(&row.input_json)?;
        let inverse_patch: Patch = serde_json::from_str(&row.inverse_patch_json)?;
        let server_call: ServerCallShape = if let Some(server_call_json) = &row.server_call_json {
            serde_json::from_str(server_call_json)?
        } else {
            ServerCallShape::None
        };
        let started_at = now_epoch_millis()?;
        let attempt_no = row.retries + 1;

        let apply_result = handler
            .apply(&ApplyCtx {
                db: &self.db,
                github: &self.github,
                blob_store: self.db.blob_store(),
                account_id: row.account_id.as_str(),
                mutation_id: row.id.as_str(),
                idempotency_key: row.idempotency_key.as_str(),
                input_json: &input_json,
                server_call: &server_call,
            })
            .await;

        match apply_result {
            Ok(response) => {
                let _ = self.events_tx.send(MutationEvent::Applied {
                    mutation_id: row.id.clone(),
                });

                handler
                    .reconcile(
                        &ReconcileCtx {
                            db: &self.db,
                            github: &self.github,
                            blob_store: self.db.blob_store(),
                            account_id: row.account_id.as_str(),
                            mutation_id: row.id.as_str(),
                            idempotency_key: row.idempotency_key.as_str(),
                            input_json: &input_json,
                            sync_handle: self.sync_handle.as_deref(),
                            cache_invalidation_emitter: self
                                .cache_emitter
                                .as_ref()
                                .map(|emitter| emitter.as_ref() as &dyn CacheInvalidationEmitter),
                        },
                        response,
                    )
                    .await?;

                let mut tx = self.db.pool().begin().await?;
                projector::apply_patch(
                    &mut tx,
                    &Patch::empty(),
                    PatchSource::ServerReconcile {
                        mutation_id: row.id.clone(),
                    },
                )
                .await?;
                sqlx::query(
                    "UPDATE pending_mutations
                     SET status = 'applied', retries = ?3, updated_at = ?2, last_error = NULL
                     WHERE id = ?1",
                )
                .bind(&row.id)
                .bind(now_epoch_seconds()?)
                .bind(attempt_no)
                .execute(tx.as_mut())
                .await?;
                tx.commit().await?;

                self.record_attempt(&row.id, attempt_no, started_at, "ok", None, None, None)
                    .await?;

                let _ = self.events_tx.send(MutationEvent::Reconciled {
                    mutation_id: row.id.clone(),
                });

                Ok(ProcessOutcome {
                    applied: true,
                    reconciled: true,
                    ..ProcessOutcome::default()
                })
            }
            Err(error) => {
                let classified = classify_error(&error);

                self.record_attempt(
                    &row.id,
                    attempt_no,
                    started_at,
                    "error",
                    Some(classified.kind.as_str().to_string()),
                    classified.http_status,
                    Some(error.to_string()),
                )
                .await?;

                if matches!(classified.kind, ErrorKind::Network) {
                    let retries = row.retries + 1;
                    if retries > MAX_NETWORK_RETRIES {
                        let rollback_result = handler
                            .rollback(&RollbackCtx {
                                db: &self.db,
                                github: &self.github,
                                blob_store: self.db.blob_store(),
                                account_id: row.account_id.as_str(),
                                mutation_id: row.id.as_str(),
                                idempotency_key: row.idempotency_key.as_str(),
                                input_json: &input_json,
                                inverse_patch: &inverse_patch,
                            })
                            .await;
                        if rollback_result.is_ok() {
                            let _ = self.events_tx.send(MutationEvent::RolledBack {
                                mutation_id: row.id.clone(),
                            });
                        }
                        self.mark_failed(
                            &row.id,
                            ErrorKind::Network,
                            true,
                            None,
                            Some(error.to_string()),
                            attempt_no,
                        )
                        .await?;
                        return Ok(ProcessOutcome {
                            failed: true,
                            rolled_back: rollback_result.is_ok(),
                            ..ProcessOutcome::default()
                        });
                    }
                    let wait = compute_backoff_delay(&row.id, retries);
                    sqlx::query(
                        "UPDATE pending_mutations
                         SET retries = ?2, updated_at = ?3, last_error = ?4
                         WHERE id = ?1",
                    )
                    .bind(&row.id)
                    .bind(retries)
                    .bind(now_epoch_seconds()?)
                    .bind(format!("{}", error))
                    .execute(self.db.pool())
                    .await?;
                    tokio::time::sleep(wait).await;
                    return Ok(ProcessOutcome {
                        retried: true,
                        ..ProcessOutcome::default()
                    });
                }

                let rollback_result = handler
                    .rollback(&RollbackCtx {
                        db: &self.db,
                        github: &self.github,
                        blob_store: self.db.blob_store(),
                        account_id: row.account_id.as_str(),
                        mutation_id: row.id.as_str(),
                        idempotency_key: row.idempotency_key.as_str(),
                        input_json: &input_json,
                        inverse_patch: &inverse_patch,
                    })
                    .await;
                if rollback_result.is_ok() {
                    let _ = self.events_tx.send(MutationEvent::RolledBack {
                        mutation_id: row.id.clone(),
                    });
                }

                self.mark_failed(
                    &row.id,
                    classified.kind.clone(),
                    classified.retryable,
                    classified.hard_conflict,
                    Some(error.to_string()),
                    attempt_no,
                )
                .await?;

                Ok(ProcessOutcome {
                    failed: true,
                    rolled_back: rollback_result.is_ok(),
                    ..ProcessOutcome::default()
                })
            }
        }
    }

    async fn mark_failed(
        &self,
        mutation_id: &str,
        kind: ErrorKind,
        retryable: bool,
        hard_conflict: Option<HardConflictDiff>,
        message: Option<String>,
        retries: i64,
    ) -> Result<()> {
        let last_error = serde_json::json!({
            "kind": kind.as_str(),
            "retryable": retryable,
            "message": message,
        });
        sqlx::query(
            "UPDATE pending_mutations
             SET status = 'failed', retries = ?4, updated_at = ?2, last_error = ?3
             WHERE id = ?1",
        )
        .bind(mutation_id)
        .bind(now_epoch_seconds()?)
        .bind(last_error.to_string())
        .bind(retries)
        .execute(self.db.pool())
        .await?;

        let _ = self.events_tx.send(MutationEvent::Failed {
            mutation_id: mutation_id.to_string(),
            error_kind: kind,
            retryable,
            hard_conflict,
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_attempt(
        &self,
        mutation_id: &str,
        attempt_no: i64,
        started_at_ms: i64,
        outcome: &str,
        error_kind: Option<String>,
        http_status: Option<i64>,
        error_message: Option<String>,
    ) -> Result<()> {
        let finished_at_ms = now_epoch_millis()?;
        let latency_ms = finished_at_ms.saturating_sub(started_at_ms);
        sqlx::query(
            "INSERT INTO mutation_attempts(
                mutation_id,
                attempt_no,
                started_at,
                finished_at,
                outcome,
                error_kind,
                http_status,
                latency_ms,
                error_message
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(mutation_id)
        .bind(attempt_no)
        .bind(started_at_ms)
        .bind(finished_at_ms)
        .bind(outcome)
        .bind(error_kind)
        .bind(http_status)
        .bind(latency_ms)
        .bind(error_message)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    async fn lookup_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<PendingMutationRow>> {
        let row = sqlx::query_as::<_, PendingMutationRow>(
            "SELECT
                id,
                account_id,
                kind,
                target_type,
                target_id,
                idempotency_key,
                input_json,
                optimistic_patch_json,
                inverse_patch_json,
                status,
                retries,
                created_at,
                updated_at,
                last_error,
                optimism_level,
                server_call_json
             FROM pending_mutations
             WHERE idempotency_key = ?1
             LIMIT 1",
        )
        .bind(idempotency_key)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row)
    }

    async fn pending_rows(&self) -> Result<Vec<PendingMutationRow>> {
        let rows = sqlx::query_as::<_, PendingMutationRow>(
            "SELECT
                id,
                account_id,
                kind,
                target_type,
                target_id,
                idempotency_key,
                input_json,
                optimistic_patch_json,
                inverse_patch_json,
                status,
                retries,
                created_at,
                updated_at,
                last_error,
                optimism_level,
                server_call_json
             FROM pending_mutations
             WHERE status = 'pending'
             ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows)
    }

    async fn load_pending_row(&self, mutation_id: &str) -> Result<Option<PendingMutationRow>> {
        let row = sqlx::query_as::<_, PendingMutationRow>(
            "SELECT
                id,
                account_id,
                kind,
                target_type,
                target_id,
                idempotency_key,
                input_json,
                optimistic_patch_json,
                inverse_patch_json,
                status,
                retries,
                created_at,
                updated_at,
                last_error,
                optimism_level,
                server_call_json
             FROM pending_mutations
             WHERE id = ?1",
        )
        .bind(mutation_id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row)
    }
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
struct PendingMutationRow {
    id: String,
    account_id: String,
    kind: String,
    target_type: String,
    target_id: String,
    idempotency_key: String,
    input_json: String,
    optimistic_patch_json: String,
    inverse_patch_json: String,
    status: String,
    retries: i64,
    created_at: i64,
    updated_at: i64,
    last_error: Option<String>,
    optimism_level: Option<String>,
    server_call_json: Option<String>,
}

impl PendingMutationRow {
    fn optimism_level(&self) -> OptimismLevel {
        match self.optimism_level.as_deref() {
            Some("cautious") => OptimismLevel::Cautious,
            Some("none") => OptimismLevel::None,
            _ => OptimismLevel::Full,
        }
    }
}

#[derive(Default)]
struct ProcessOutcome {
    applied: bool,
    reconciled: bool,
    failed: bool,
    rolled_back: bool,
    retried: bool,
}

#[derive(Debug, Clone)]
struct ClassifiedError {
    kind: ErrorKind,
    retryable: bool,
    http_status: Option<i64>,
    hard_conflict: Option<HardConflictDiff>,
}

fn classify_error(error: &anyhow::Error) -> ClassifiedError {
    if let Some(apply_error) = error.downcast_ref::<MutationApplyError>() {
        return ClassifiedError {
            kind: apply_error.kind.clone(),
            retryable: apply_error.retryable,
            http_status: apply_error.http_status,
            hard_conflict: apply_error.hard_conflict.clone(),
        };
    }

    let message = error.to_string().to_ascii_lowercase();
    if message.contains("timed out") || message.contains("connection") || message.contains("dns") {
        return ClassifiedError {
            kind: ErrorKind::Network,
            retryable: true,
            http_status: None,
            hard_conflict: None,
        };
    }

    ClassifiedError {
        kind: ErrorKind::Other("unclassified".to_string()),
        retryable: true,
        http_status: None,
        hard_conflict: None,
    }
}

fn compute_backoff_delay(mutation_id: &str, retries: i64) -> Duration {
    let exponent = u32::try_from(retries.clamp(1, 6)).unwrap_or(1);
    let base_ms = 50_u64.saturating_mul(2_u64.saturating_pow(exponent));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    mutation_id.hash(&mut hasher);
    retries.hash(&mut hasher);
    let jitter = hasher.finish() % 30;
    Duration::from_millis(base_ms.saturating_add(jitter).min(500))
}

fn new_mutation_id(account_id: &str, idempotency_key: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    account_id.hash(&mut hasher);
    idempotency_key.hash(&mut hasher);
    let digest = hasher.finish();
    let now = now_epoch_millis().unwrap_or_default();
    format!("mut-{now}-{digest:016x}")
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}

fn now_epoch_millis() -> Result<i64> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_millis()).context("unix timestamp millis exceeds i64")
}

trait ErrorKindExt {
    fn as_str(&self) -> &str;
}

impl ErrorKindExt for ErrorKind {
    fn as_str(&self) -> &str {
        match self {
            ErrorKind::Network => "network",
            ErrorKind::RateLimited => "rate_limited",
            ErrorKind::Auth => "auth",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Conflict => "conflict",
            ErrorKind::Server => "server",
            ErrorKind::Other(_) => "other",
        }
    }
}

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use sqlx::FromRow;
use tokio::sync::broadcast;

use super::dispatch;
use super::ipc_types::{
    DrainSummary, HardConflictPayload, MutationEvent, PendingMutationView, QueueReason,
    SubmitPayload, SubmittedMutation,
};
use super::net::{error_kind_from_status, NetState, NetworkMonitor};
use super::patch::Patch;
use super::projector::{self, PatchSource};
use super::{
    ApplyCtx, ErrorKind, HardConflictDiff, Mutation, MutationKind, OptimismLevel, PredictCtx,
    ReconcileCtx, RollbackCtx, ServerCallShape,
};
use crate::api::GithubClient;
use crate::db::Db;
use crate::sync::{CacheInvalidationEmitter, Clock, SyncHandle, TokioClock};
use tokio::sync::watch;

const MAX_NETWORK_RETRIES: i64 = 5;
static MUTATION_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct MutationEngine {
    db: Arc<Db>,
    github: GithubClient,
    handlers: HashMap<MutationKind, Arc<dyn Mutation>>,
    sync_handle: Option<Arc<SyncHandle>>,
    cache_emitter: Option<Arc<dyn CacheInvalidationEmitter>>,
    network_monitor: Option<NetworkMonitor>,
    net_state_rx: Option<watch::Receiver<NetState>>,
    clock: Arc<dyn Clock>,
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
        let kind = u16::try_from(status)
            .ok()
            .and_then(|code| reqwest::StatusCode::from_u16(code).ok())
            .map(error_kind_from_status)
            .unwrap_or(ErrorKind::Server);

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
        let handlers = dispatch::dispatch_table();

        Self {
            db,
            github,
            handlers,
            sync_handle: None,
            cache_emitter: None,
            network_monitor: None,
            net_state_rx: None,
            clock: Arc::new(TokioClock),
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

    pub fn with_network_monitor(mut self, monitor: NetworkMonitor) -> Self {
        self.net_state_rx = Some(monitor.subscribe());
        self.network_monitor = Some(monitor);
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn register_handler(&mut self, handler: Arc<dyn Mutation>) {
        self.handlers.insert(handler.kind(), handler);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MutationEvent> {
        self.events_tx.subscribe()
    }

    pub fn network_state_receiver(&self) -> Option<watch::Receiver<NetState>> {
        self.net_state_rx.clone()
    }

    pub async fn submit(
        &self,
        account_id: &str,
        payload: SubmitPayload,
    ) -> Result<SubmittedMutation> {
        let submit_account_id = payload
            .posting_account_id
            .as_deref()
            .unwrap_or(account_id)
            .to_string();
        if let Some(posting_account_id) = payload.posting_account_id.as_deref() {
            let account_exists = self
                .db
                .auth_account_by_id(posting_account_id)
                .await?
                .is_some();
            if !account_exists {
                return Err(anyhow!(
                    "posting account `{posting_account_id}` is not configured"
                ));
            }
        }
        if let Some(existing) = self
            .lookup_by_idempotency_key(&payload.idempotency_key)
            .await?
        {
            let optimism_level = existing.optimism_level();
            let requires_confirmation =
                optimism_level == OptimismLevel::None || existing.requires_connection_confirmation;
            let projected_changes = serde_json::from_str::<Patch>(&existing.optimistic_patch_json)
                .map(|patch| projected_change_summary(&patch))
                .unwrap_or_default();
            return Ok(SubmittedMutation {
                mutation_id: existing.id,
                deduped: true,
                requires_confirmation,
                optimism_level,
                projected_changes,
            });
        }

        let handler = self.handlers.get(&payload.kind).ok_or_else(|| {
            anyhow!(
                "no mutation handler registered for {}",
                payload.kind.as_str()
            )
        })?;

        let mutation_id = new_mutation_id(&submit_account_id, &payload.idempotency_key);
        let optimism = handler.optimism();
        let is_offline = self.is_offline();
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
                account_id: &submit_account_id,
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
                created_at, updated_at, last_error, optimism_level, server_call_json,
                requires_connection_confirmation
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', 0, ?10, ?10, NULL, ?11, ?12, 0)",
        )
        .bind(&mutation_id)
        .bind(&submit_account_id)
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
            account_id: submit_account_id,
            kind: payload.kind,
            optimism,
            target_type: payload.target_type,
            target_id: payload.target_id,
            status: "pending".to_string(),
            retries: 0,
            created_at: now_epoch_seconds()?,
            updated_at: now_epoch_seconds()?,
            last_error: None,
            pending_overlay: predicted
                .forward_patch
                .pending_overlay_kind
                .clone()
                .map(|kind| super::PendingOverlay {
                    mutation_id: mutation_id.clone(),
                    kind,
                }),
            requires_connection_confirmation: false,
        };
        if is_offline {
            let _ = self.events_tx.send(MutationEvent::Queued {
                mutation: view,
                reason: QueueReason::Offline,
            });
        } else {
            let _ = self
                .events_tx
                .send(MutationEvent::Submitted { mutation: view });
        }

        Ok(SubmittedMutation {
            mutation_id,
            deduped: false,
            requires_confirmation: optimism == OptimismLevel::None,
            optimism_level: optimism,
            projected_changes: projected_change_summary(&predicted.forward_patch),
        })
    }

    pub async fn drain(&self) -> Result<DrainSummary> {
        let rows = self.pending_rows().await?;
        let offline = self.is_offline();
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
            if offline {
                if row.optimism_level() == OptimismLevel::None {
                    self.mark_requires_connection_confirmation(&row.id).await?;
                }
                continue;
            }
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

        if row.optimism_level() == OptimismLevel::None {
            self.mark_requires_connection_confirmation(&row.id).await?;
            if let Ok(kind) = MutationKind::from_str(&row.kind) {
                let _ = self.events_tx.send(MutationEvent::Queued {
                    mutation: PendingMutationView {
                        id: row.id.clone(),
                        account_id: row.account_id.clone(),
                        kind,
                        optimism: row.optimism_level(),
                        target_type: row.target_type.clone(),
                        target_id: row.target_id.clone(),
                        status: row.status.clone(),
                        retries: row.retries,
                        created_at: row.created_at,
                        updated_at: now_epoch_seconds()?,
                        last_error: row.last_error.clone(),
                        pending_overlay: None,
                        requires_connection_confirmation: true,
                    },
                    reason: QueueReason::RequiresConnectionConfirmation,
                });
            }
            return Ok(ProcessOutcome {
                ..ProcessOutcome::default()
            });
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

        if let Some(monitor) = &self.network_monitor {
            monitor.traffic_started().await;
        }
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
        if let Some(monitor) = &self.network_monitor {
            monitor.traffic_finished().await;
        }

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
                self.report_network_error_if_needed(&classified).await;

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

                if matches!(classified.kind, ErrorKind::Conflict) {
                    let conflict_payload = self
                        .build_hard_conflict_payload(&row, kind, &classified, &input_json)
                        .await
                        .unwrap_or(None);
                    let merged_diff = conflict_payload
                        .as_ref()
                        .map(|payload| payload.diff.clone())
                        .or_else(|| classified.hard_conflict.clone());

                    if let Some(payload) = conflict_payload {
                        let _ = self.events_tx.send(MutationEvent::HardConflict { payload });
                    }

                    self.mark_failed(
                        &row.id,
                        ErrorKind::Conflict,
                        false,
                        merged_diff,
                        Some(error.to_string()),
                        attempt_no,
                    )
                    .await?;
                    return Ok(ProcessOutcome {
                        failed: true,
                        ..ProcessOutcome::default()
                    });
                }

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
                    self.clock.sleep(wait).await;
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

    async fn mark_requires_connection_confirmation(&self, mutation_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pending_mutations
             SET requires_connection_confirmation = 1, updated_at = ?2
             WHERE id = ?1",
        )
        .bind(mutation_id)
        .bind(now_epoch_seconds()?)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    fn is_offline(&self) -> bool {
        self.net_state_rx
            .as_ref()
            .map(|rx| matches!(*rx.borrow(), NetState::Offline { .. }))
            .unwrap_or(false)
    }

    async fn report_network_error_if_needed(&self, classified: &ClassifiedError) {
        let should_report = classified.http_status.is_some()
            || matches!(
                classified.kind,
                ErrorKind::Network
                    | ErrorKind::Auth
                    | ErrorKind::NotFound
                    | ErrorKind::Conflict
                    | ErrorKind::RateLimited
                    | ErrorKind::Server
            );
        if !should_report {
            return;
        }
        if let Some(monitor) = &self.network_monitor {
            monitor.report_api_error(classified.kind.clone()).await;
        }
    }

    async fn build_hard_conflict_payload(
        &self,
        row: &PendingMutationRow,
        kind: MutationKind,
        classified: &ClassifiedError,
        input_json: &serde_json::Value,
    ) -> Result<Option<HardConflictPayload>> {
        if row.target_type != "pull_request" {
            return Ok(None);
        }

        let predicted_snapshot = self
            .projected_pull_request_snapshot(&row.account_id, &row.target_id)
            .await?;
        let server_snapshot = self
            .fetch_server_pull_request_snapshot(row, input_json)
            .await?
            .unwrap_or_else(|| serde_json::json!({}));

        let mut diff = compute_conflict_diff(&predicted_snapshot, &server_snapshot);
        if diff.summary.is_empty() {
            if let Some(existing) = &classified.hard_conflict {
                diff = existing.clone();
            }
        }

        Ok(Some(HardConflictPayload {
            mutation_id: row.id.clone(),
            kind,
            target_id: row.target_id.clone(),
            server_snapshot_json: server_snapshot.to_string(),
            predicted_snapshot_json: predicted_snapshot.to_string(),
            diff,
        }))
    }

    async fn projected_pull_request_snapshot(
        &self,
        account_id: &str,
        pr_id: &str,
    ) -> Result<serde_json::Value> {
        let row = self.db.pr_detail_summary(account_id, pr_id).await?;
        let Some(summary) = row else {
            return Ok(serde_json::json!({}));
        };
        Ok(serde_json::json!({
            "id": summary.pr_id,
            "title": summary.title,
            "body": summary.body,
            "state": summary.state,
            "draft": summary.draft == 1,
            "updated_at": summary.updated_at
        }))
    }

    async fn fetch_server_pull_request_snapshot(
        &self,
        row: &PendingMutationRow,
        input_json: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>> {
        let owner = input_json.get("owner").and_then(serde_json::Value::as_str);
        let repo = input_json.get("repo").and_then(serde_json::Value::as_str);
        let number = input_json
            .get("pr_number")
            .and_then(serde_json::Value::as_i64);
        let (Some(owner), Some(repo), Some(number)) = (owner, repo, number) else {
            return Ok(None);
        };
        let path = format!("/repos/{owner}/{repo}/pulls/{number}");
        let (response, _rate_limit) = self
            .github
            .rest_mutation_json::<serde_json::Value>(
                &row.account_id,
                reqwest::Method::GET,
                &path,
                None,
                Some(row.idempotency_key.as_str()),
            )
            .await?;
        let Some(snapshot) = response else {
            return Ok(None);
        };

        let title = snapshot.get("title").and_then(serde_json::Value::as_str);
        let body = snapshot.get("body").and_then(serde_json::Value::as_str);
        let state = snapshot.get("state").and_then(serde_json::Value::as_str);
        let draft = snapshot.get("draft").and_then(serde_json::Value::as_bool);
        let updated_at = snapshot
            .get("updated_at")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_github_timestamp);

        if title.is_some() || body.is_some() || state.is_some() || draft.is_some() {
            sqlx::query(
                "UPDATE pull_requests
                 SET title = COALESCE(?4, title),
                     body = COALESCE(?5, body),
                     state = COALESCE(?6, state),
                     draft = COALESCE(?7, draft),
                     updated_at = COALESCE(?8, updated_at)
                 WHERE account_id = ?1 AND id = ?2 AND number = ?3",
            )
            .bind(&row.account_id)
            .bind(&row.target_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(state)
            .bind(draft.map(|value| if value { 1_i64 } else { 0_i64 }))
            .bind(updated_at)
            .execute(self.db.pool())
            .await?;
        }

        Ok(Some(snapshot))
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
                server_call_json,
                requires_connection_confirmation
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
                server_call_json,
                requires_connection_confirmation
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
                server_call_json,
                requires_connection_confirmation
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
    requires_connection_confirmation: bool,
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
    let sequence = MUTATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("mut-{now}-{sequence:016x}-{digest:016x}")
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

fn parse_github_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.timestamp())
}

fn compute_conflict_diff(
    predicted_snapshot: &serde_json::Value,
    server_snapshot: &serde_json::Value,
) -> HardConflictDiff {
    let local_body = predicted_snapshot
        .get("body")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let server_body = server_snapshot
        .get("body")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    let mut changed_fields = Vec::new();
    if let (Some(predicted), Some(server)) =
        (predicted_snapshot.as_object(), server_snapshot.as_object())
    {
        let mut keys = predicted
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        keys.extend(server.keys().cloned());
        for key in keys {
            if predicted.get(&key) != server.get(&key) {
                changed_fields.push(key);
            }
        }
    }

    let summary = if changed_fields.is_empty() {
        "Server state diverged from optimistic state.".to_string()
    } else {
        format!(
            "Server state diverged on fields: {}",
            changed_fields.join(", ")
        )
    };

    HardConflictDiff {
        summary,
        local_body,
        server_body,
        changed_fields,
    }
}

fn projected_change_summary(patch: &Patch) -> Vec<String> {
    patch
        .operations
        .iter()
        .map(|operation| {
            let mut keys = operation
                .pk
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{key}={}",
                        serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
                    )
                })
                .collect::<Vec<_>>();
            keys.sort();
            format!("{}({})", operation.table, keys.join(","))
        })
        .collect()
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

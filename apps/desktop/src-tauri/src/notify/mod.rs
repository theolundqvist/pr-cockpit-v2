use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::broadcast;

use crate::db::Db;
use crate::mutations::MutationEvent;
use crate::sync::SyncReconciledEvent;

pub mod dedup;
pub mod dispatcher;
pub mod rules;
pub mod triggers;

use self::dedup::{insert_or_ignore, mark_suppressed};
use self::dispatcher::OsNotificationSender;
use self::rules::{load_effective_settings, rule_enabled, suppression_reason};
use self::triggers::{
    mutation_failure_candidate, mutation_failure_context, sync_triggers, AccountSnapshot,
    NotificationCandidate, NotificationKind, Trigger,
};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationEventPayload {
    pub event_id: String,
    pub account_id: String,
    pub repo_id: Option<String>,
    pub pr_id: Option<String>,
    pub event_type: String,
    pub actor_id: String,
    pub server_event_id: String,
    pub title: String,
    pub body: String,
    pub fired_at: i64,
    pub deduped: bool,
}

impl tauri_specta::Event for NotificationEventPayload {
    const NAME: &'static str = "notification:event";
}

pub trait NotificationEventEmitter: Send + Sync {
    fn emit_notification_event(&self, payload: NotificationEventPayload);
}

#[derive(Clone)]
pub struct NotificationEngine {
    db: Arc<Db>,
    sender: Arc<dyn OsNotificationSender>,
    emitter: Arc<dyn NotificationEventEmitter>,
    snapshots: Arc<tokio::sync::Mutex<HashMap<String, AccountSnapshot>>>,
    triggers: Arc<Vec<Box<dyn Trigger>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct DebugNotificationInput {
    pub kind: String,
    pub repo_id: Option<String>,
    pub repo_full_name: Option<String>,
    pub pr_id: Option<String>,
    pub actor_id: Option<String>,
    pub server_event_id: Option<String>,
    pub title: String,
    pub body: String,
}

impl NotificationEngine {
    pub async fn new(
        db: Arc<Db>,
        sender: Arc<dyn OsNotificationSender>,
        emitter: Arc<dyn NotificationEventEmitter>,
    ) -> Result<Self> {
        let engine = Self {
            db,
            sender,
            emitter,
            snapshots: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            triggers: Arc::new(sync_triggers()),
        };
        engine.prime_snapshots().await?;
        Ok(engine)
    }

    pub fn spawn(
        self: Arc<Self>,
        mut mutation_rx: broadcast::Receiver<MutationEvent>,
        mut sync_rx: broadcast::Receiver<SyncReconciledEvent>,
    ) {
        let sync_engine = Arc::clone(&self);
        tauri::async_runtime::spawn(async move {
            loop {
                match sync_rx.recv().await {
                    Ok(event) => {
                        if let Err(error) =
                            sync_engine.handle_sync_reconciled(&event.account_id).await
                        {
                            tracing::warn!(
                                account_id = event.account_id,
                                scope = event.scope,
                                error = %error,
                                "notification sync trigger failed"
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });

        tauri::async_runtime::spawn(async move {
            loop {
                match mutation_rx.recv().await {
                    Ok(event) => {
                        if let Err(error) = self.handle_mutation_event(event).await {
                            tracing::warn!(error = %error, "notification mutation trigger failed");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        });
    }

    pub async fn process_sync_reconciled(&self, account_id: &str) -> Result<()> {
        self.handle_sync_reconciled(account_id).await
    }

    pub async fn process_mutation_event(&self, event: MutationEvent) -> Result<()> {
        self.handle_mutation_event(event).await
    }

    pub async fn simulate_event(
        &self,
        account_id: &str,
        input: DebugNotificationInput,
    ) -> Result<Option<NotificationEventPayload>> {
        let kind = input
            .kind
            .parse::<NotificationKind>()
            .with_context(|| format!("invalid notification kind `{}`", input.kind))?;
        let candidate = NotificationCandidate {
            account_id: account_id.to_string(),
            repo_id: input.repo_id,
            repo_full_name: input.repo_full_name,
            pr_id: input.pr_id,
            kind,
            actor_id: input.actor_id.unwrap_or_else(|| "debug".to_string()),
            server_event_id: input.server_event_id.unwrap_or_else(|| {
                format!(
                    "debug-{}-{}",
                    kind.as_str(),
                    now_epoch_seconds().unwrap_or_default()
                )
            }),
            title: input.title,
            body: input.body,
        };
        self.process_candidate(candidate).await
    }

    async fn prime_snapshots(&self) -> Result<()> {
        let accounts = self.db.list_auth_accounts().await?;
        let mut lock = self.snapshots.lock().await;
        for account in accounts {
            if let Some(snapshot) = AccountSnapshot::load(self.db.as_ref(), &account.id).await? {
                lock.insert(account.id, snapshot);
            }
        }
        Ok(())
    }

    async fn handle_sync_reconciled(&self, account_id: &str) -> Result<()> {
        let next_snapshot = AccountSnapshot::load(self.db.as_ref(), account_id)
            .await?
            .with_context(|| format!("missing account snapshot for `{account_id}`"))?;
        let previous = {
            let lock = self.snapshots.lock().await;
            lock.get(account_id).cloned()
        };
        let Some(previous) = previous else {
            let mut lock = self.snapshots.lock().await;
            lock.insert(account_id.to_string(), next_snapshot);
            return Ok(());
        };

        for trigger in self.triggers.iter() {
            for candidate in trigger.evaluate(&previous, &next_snapshot) {
                if let Err(error) = self.process_candidate(candidate).await {
                    tracing::warn!(kind = trigger.kind().as_str(), error = %error, "processing notification candidate failed");
                }
            }
        }

        let mut lock = self.snapshots.lock().await;
        lock.insert(account_id.to_string(), next_snapshot);
        Ok(())
    }

    async fn handle_mutation_event(&self, event: MutationEvent) -> Result<()> {
        let MutationEvent::Failed { mutation_id, .. } = &event else {
            return Ok(());
        };
        let Some(context) = mutation_failure_context(self.db.as_ref(), mutation_id).await? else {
            return Ok(());
        };
        let Some(candidate) = mutation_failure_candidate(
            &event,
            &context.account_id,
            context.repo_id,
            context.repo_full_name,
            context.pr_id,
        ) else {
            return Ok(());
        };
        let _ = self.process_candidate(candidate).await?;
        Ok(())
    }

    async fn process_candidate(
        &self,
        candidate: NotificationCandidate,
    ) -> Result<Option<NotificationEventPayload>> {
        if !rule_enabled(self.db.as_ref(), &candidate.account_id, candidate.kind).await? {
            return Ok(None);
        }

        let fired_at = now_epoch_seconds()?;
        let inserted = insert_or_ignore(self.db.as_ref(), &candidate, fired_at).await?;
        if !inserted.inserted {
            return Ok(None);
        }

        let settings = load_effective_settings(self.db.as_ref(), &candidate.account_id).await?;
        let mut deduped =
            suppression_reason(&settings, candidate.repo_full_name.as_deref(), Utc::now())
                .is_some();
        if deduped {
            mark_suppressed(self.db.as_ref(), &inserted.event_id).await?;
        } else if let Err(error) = self.sender.send(&candidate.title, &candidate.body) {
            tracing::warn!(error = %error, "sending OS notification failed");
            mark_suppressed(self.db.as_ref(), &inserted.event_id).await?;
            deduped = true;
        }

        let payload = NotificationEventPayload {
            event_id: inserted.event_id,
            account_id: candidate.account_id,
            repo_id: candidate.repo_id,
            pr_id: candidate.pr_id,
            event_type: candidate.kind.as_str().to_string(),
            actor_id: candidate.actor_id,
            server_event_id: candidate.server_event_id,
            title: candidate.title,
            body: candidate.body,
            fired_at,
            deduped,
        };
        self.emitter.emit_notification_event(payload.clone());
        Ok(Some(payload))
    }
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}

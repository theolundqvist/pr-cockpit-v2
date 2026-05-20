use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use async_trait::async_trait;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::Router;
use hmac::{Hmac, Mac};
use once_cell::sync::Lazy;
use serde::Deserialize;
use specta::Type;
use tokio::sync::{oneshot, Mutex, RwLock};
use tracing::warn;

use crate::db::Db;
use crate::stacks::ops::set_bool_setting;
use crate::sync::reconcile::RefetchTarget;
use crate::sync::SyncHandle;

const RELAY_ENABLED_SETTING_KEY: &str = "relay_enabled";
const RELAY_KEYRING_SERVICE: &str = "pr-cockpit-relay";
const RELAY_KEYRING_USER: &str = "relay-forward-secret";
const TIMESTAMP_WINDOW_SECONDS: i64 = 5 * 60;

const HEADER_EVENT: &str = "x-github-event";
const HEADER_SIGNATURE: &str = "x-relay-signature-256";
const HEADER_NONCE: &str = "x-relay-nonce";
const HEADER_TIMESTAMP: &str = "x-relay-timestamp";

static ACTIVE_SYNC_HANDLES: Lazy<RwLock<Vec<Arc<SyncHandle>>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Type, PartialEq, Eq)]
pub struct RelaySettingsSnapshot {
    pub enabled: bool,
    pub has_forward_secret: bool,
    pub local_url: Option<String>,
}

#[async_trait]
pub trait RefetchDispatcher: Send + Sync {
    async fn enqueue_refetch(&self, targets: Vec<RefetchTarget>) -> Result<()>;
}

#[derive(Default)]
pub struct SyncHandleDispatcher;

#[async_trait]
impl RefetchDispatcher for SyncHandleDispatcher {
    async fn enqueue_refetch(&self, targets: Vec<RefetchTarget>) -> Result<()> {
        let handles = ACTIVE_SYNC_HANDLES.read().await.clone();
        for handle in handles {
            handle.enqueue_refetch(targets.clone()).await?;
        }
        Ok(())
    }
}

pub async fn register_sync_handle(handle: Arc<SyncHandle>) {
    ACTIVE_SYNC_HANDLES.write().await.push(handle);
}

pub async fn clear_registered_sync_handles() {
    ACTIVE_SYNC_HANDLES.write().await.clear();
}

#[derive(Clone)]
pub struct RelayManager {
    db: Arc<Db>,
    runtime: Arc<Mutex<Option<RelayReceiverHandle>>>,
    dispatcher: Arc<dyn RefetchDispatcher>,
}

impl RelayManager {
    pub fn new(db: Arc<Db>) -> Self {
        Self::with_dispatcher(db, Arc::new(SyncHandleDispatcher))
    }

    pub fn with_dispatcher(db: Arc<Db>, dispatcher: Arc<dyn RefetchDispatcher>) -> Self {
        Self {
            db,
            runtime: Arc::new(Mutex::new(None)),
            dispatcher,
        }
    }

    pub async fn bootstrap(&self) -> Result<()> {
        self.reload().await
    }

    pub async fn settings_snapshot(&self) -> Result<RelaySettingsSnapshot> {
        let enabled = read_bool_setting(self.db.as_ref(), RELAY_ENABLED_SETTING_KEY, false).await?;
        let has_forward_secret = load_forward_secret()?.is_some();
        let local_url = self.local_url().await;
        Ok(RelaySettingsSnapshot {
            enabled,
            has_forward_secret,
            local_url,
        })
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<()> {
        set_bool_setting(self.db.as_ref(), RELAY_ENABLED_SETTING_KEY, enabled).await?;
        self.reload().await
    }

    pub async fn set_forward_secret(&self, secret: String) -> Result<()> {
        let trimmed = secret.trim().to_string();
        if trimmed.is_empty() {
            store_forward_secret(None)?;
        } else {
            store_forward_secret(Some(&trimmed))?;
        }
        self.reload().await
    }

    pub async fn local_url(&self) -> Option<String> {
        self.runtime
            .lock()
            .await
            .as_ref()
            .map(|runtime| runtime.local_url.clone())
    }

    async fn reload(&self) -> Result<()> {
        let enabled = read_bool_setting(self.db.as_ref(), RELAY_ENABLED_SETTING_KEY, false).await?;
        let secret = load_forward_secret()?;

        let existing = self.runtime.lock().await.take();
        if let Some(runtime) = existing {
            runtime.shutdown().await;
        }

        if enabled {
            if let Some(secret) = secret {
                let runtime = start_receiver(secret, Arc::clone(&self.dispatcher)).await?;
                *self.runtime.lock().await = Some(runtime);
            }
        }
        Ok(())
    }
}

pub struct RelayReceiverHandle {
    local_url: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl RelayReceiverHandle {
    pub fn local_url(&self) -> &str {
        &self.local_url
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        let _ = self.task.await;
    }
}

#[derive(Clone)]
struct ReceiverState {
    forward_secret: Arc<String>,
    nonce_seen: Arc<Mutex<HashMap<String, i64>>>,
    dispatcher: Arc<dyn RefetchDispatcher>,
}

pub async fn start_receiver(
    forward_secret: String,
    dispatcher: Arc<dyn RefetchDispatcher>,
) -> Result<RelayReceiverHandle> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding relay receiver")?;
    let local_addr = listener
        .local_addr()
        .context("reading relay receiver local address")?;

    let state = ReceiverState {
        forward_secret: Arc::new(forward_secret),
        nonce_seen: Arc::new(Mutex::new(HashMap::new())),
        dispatcher,
    };
    let router = Router::new()
        .route("/webhook", post(handle_webhook))
        .with_state(state);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        });
        if let Err(error) = server.await {
            warn!(target: "relay", error = %error, "relay receiver server exited");
        }
    });

    Ok(RelayReceiverHandle {
        local_url: format!("http://{}:{}/webhook", local_addr.ip(), local_addr.port()),
        shutdown_tx: Some(shutdown_tx),
        task,
    })
}

async fn handle_webhook(
    State(state): State<ReceiverState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(signature) = headers
        .get(HEADER_SIGNATURE)
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED;
    };
    let Some(nonce) = headers
        .get(HEADER_NONCE)
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED;
    };
    let Some(timestamp_raw) = headers
        .get(HEADER_TIMESTAMP)
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED;
    };

    let Ok(timestamp) = timestamp_raw.parse::<i64>() else {
        return StatusCode::UNAUTHORIZED;
    };
    let Ok(now) = now_epoch_seconds() else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };
    if (now - timestamp).abs() > TIMESTAMP_WINDOW_SECONDS {
        return StatusCode::UNAUTHORIZED;
    }

    if !record_nonce(&state.nonce_seen, nonce, now).await {
        return StatusCode::UNAUTHORIZED;
    }

    let Ok(signature_valid) = verify_relay_signature(
        state.forward_secret.as_str(),
        body.as_ref(),
        nonce,
        timestamp_raw,
        signature,
    ) else {
        return StatusCode::UNAUTHORIZED;
    };
    if !signature_valid {
        return StatusCode::UNAUTHORIZED;
    }

    let event = headers
        .get(HEADER_EVENT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if let Some(target) = extract_refetch_target(event, body.as_ref()) {
        if let Err(error) = state.dispatcher.enqueue_refetch(vec![target]).await {
            warn!(target: "relay", error = %error, "failed to enqueue relay refetch target");
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    }

    StatusCode::OK
}

async fn record_nonce(cache: &Mutex<HashMap<String, i64>>, nonce: &str, now: i64) -> bool {
    let mut guard = cache.lock().await;
    guard.retain(|_, seen_at| now.saturating_sub(*seen_at) <= TIMESTAMP_WINDOW_SECONDS);
    if guard.contains_key(nonce) {
        return false;
    }
    guard.insert(nonce.to_string(), now);
    true
}

fn verify_relay_signature(
    secret: &str,
    body: &[u8],
    nonce: &str,
    timestamp: &str,
    signature_header: &str,
) -> Result<bool> {
    type HmacSha256 = Hmac<sha2::Sha256>;
    let provided_signature = signature_header
        .strip_prefix("sha256=")
        .context("missing sha256 signature prefix")
        .and_then(|raw| hex::decode(raw).context("invalid relay signature encoding"))?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).context("invalid relay secret")?;
    mac.update(body);
    mac.update(b".");
    mac.update(nonce.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    Ok(mac.verify_slice(&provided_signature).is_ok())
}

#[derive(Debug, Deserialize)]
struct RelayWebhookPayload {
    repository: Option<RelayRepositoryPayload>,
    pull_request: Option<RelayPullRequestPayload>,
    issue: Option<RelayIssuePayload>,
}

#[derive(Debug, Deserialize)]
struct RelayRepositoryPayload {
    owner: RelayOwnerPayload,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RelayOwnerPayload {
    login: String,
}

#[derive(Debug, Deserialize)]
struct RelayPullRequestPayload {
    number: i64,
}

#[derive(Debug, Deserialize)]
struct RelayIssuePayload {
    number: i64,
    pull_request: Option<serde_json::Value>,
}

fn extract_refetch_target(_event: &str, body: &[u8]) -> Option<RefetchTarget> {
    let payload = serde_json::from_slice::<RelayWebhookPayload>(body).ok()?;
    let repository = payload.repository?;
    let number = payload
        .pull_request
        .map(|pull_request| pull_request.number)
        .or_else(|| {
            payload.issue.and_then(|issue| {
                issue
                    .pull_request
                    .as_ref()
                    .map(|_pull_request_marker| issue.number)
            })
        })?;
    Some(RefetchTarget {
        owner: repository.owner.login,
        repo: repository.name,
        number,
    })
}

fn relay_secret_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(RELAY_KEYRING_SERVICE, RELAY_KEYRING_USER)
        .context("creating relay keyring entry")
}

fn load_forward_secret() -> Result<Option<String>> {
    let entry = relay_secret_entry()?;
    match entry.get_password() {
        Ok(secret) => {
            if secret.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(secret))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(anyhow::Error::new(error).context("reading relay keyring secret")),
    }
}

fn store_forward_secret(secret: Option<&str>) -> Result<()> {
    let entry = relay_secret_entry()?;
    match secret {
        Some(secret) => entry
            .set_password(secret)
            .context("storing relay forward secret"),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(anyhow::Error::new(error).context("deleting relay forward secret")),
        },
    }
}

async fn read_bool_setting(db: &Db, key: &str, default: bool) -> Result<bool> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?1 LIMIT 1")
            .bind(key)
            .fetch_optional(db.pool())
            .await?;
    let Some(value) = value else {
        return Ok(default);
    };
    let normalized = value.trim().to_ascii_lowercase();
    Ok(matches!(normalized.as_str(), "1" | "true" | "yes" | "on"))
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}

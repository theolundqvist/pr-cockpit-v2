use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::api::ApiResource;
use desktop_lib::auth::{
    AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::Db;
use desktop_lib::ipc::{ipc_accounts_list_impl, ipc_system_status_impl, SystemStatusInput};
use desktop_lib::sync::{NoopCacheInvalidationEmitter, RateLimitBudgeter, SyncTierStateStore};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;

const STUB_KEYCHAIN_TOKEN: &str = "ghp_stubbedkeychainsecret1234567890";

#[derive(Default)]
struct FakeTokenStore {
    entries: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl TokenStore for FakeTokenStore {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError> {
        self.entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .insert((host.to_string(), login.to_string()), token.clone());
        Ok(())
    }

    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .get(&(host.to_string(), login.to_string()))
            .cloned())
    }

    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError> {
        self.entries
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .remove(&(host.to_string(), login.to_string()));
        Ok(())
    }
}

struct StubGhCli;

impl GhCli for StubGhCli {
    fn run(&self, _args: &[&str]) -> Result<GhCommandOutput, AuthError> {
        Err(AuthError::GhMissing)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn ipc_flow_tracing_does_not_leak_stubbed_tokens() -> Result<()> {
    let capture = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer {
        events: Arc::clone(&capture),
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let now = 1_i64;
    let account = db
        .upsert_auth_account("github.com", "ipc-user", "pat", "repo,read:org", now)
        .await?;

    let token_store = Arc::new(FakeTokenStore::default());
    token_store.put(
        "github.com",
        "ipc-user",
        &StoredTokenSecret {
            access_token: STUB_KEYCHAIN_TOKEN.to_string(),
            refresh_token: None,
            refresh_token_expires_in: None,
        },
    )?;

    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        token_store,
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        AuthConfig::default(),
    ));
    let sync_state = SyncTierStateStore::default();
    let budgeter = RateLimitBudgeter::start_with_emitter(
        Arc::clone(&db),
        Arc::new(NoopCacheInvalidationEmitter),
    );

    let _accounts = ipc_accounts_list_impl(auth.as_ref())
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    budgeter
        .record(
            &account.id,
            ApiResource::Core,
            desktop_lib::api::RateLimitSnapshot {
                remaining: 1200,
                limit_total: 5000,
                used: Some(3800),
                reset_at_epoch: 1_999_999_999,
            },
        )
        .await?;
    let _status = ipc_system_status_impl(
        db.as_ref(),
        &sync_state,
        SystemStatusInput {
            account_id: account.id.clone(),
        },
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;

    let events = capture.lock().expect("capture lock poisoned");
    for event in events.iter() {
        assert!(
            !event.contains(STUB_KEYCHAIN_TOKEN),
            "captured tracing output leaked keychain token bytes"
        );
    }

    Ok(())
}

#[derive(Clone)]
struct CaptureLayer {
    events: Arc<Mutex<Vec<String>>>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventFieldVisitor::default();
        event.record(&mut visitor);
        if let Ok(mut events) = self.events.lock() {
            events.push(visitor.fields.join(" "));
        }
    }
}

#[derive(Default)]
struct EventFieldVisitor {
    fields: Vec<String>,
}

impl Visit for EventFieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.push(format!("{}={value:?}", field.name()));
    }
}

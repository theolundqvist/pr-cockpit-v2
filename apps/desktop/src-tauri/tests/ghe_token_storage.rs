use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::auth::{
    auth_pat_save_impl, AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput,
    KeyringTokenStore, PatSaveInput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::Db;
use regex::Regex;
use sqlx::Row;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Layer;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const GHE_TOKEN: &str = "ghp_ghe_token_storage_readiness_1234567890";

struct StubGhCli;

impl GhCli for StubGhCli {
    fn run(&self, _args: &[&str]) -> Result<GhCommandOutput, AuthError> {
        Err(AuthError::GhMissing)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn ghe_pat_stays_keyring_only_and_out_of_sqlite_and_logs() -> Result<()> {
    let server = MockServer::start().await;
    let _user_lookup = Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", format!("Bearer {GHE_TOKEN}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"login":"ghe-keyring-user"}))
                .insert_header("x-oauth-scopes", "repo,notifications"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let temp = tempfile::TempDir::new()?;
    let hosts_path = temp.path().join("hosts.toml");
    tokio::fs::write(
        &hosts_path,
        format!(
            r#"[hosts."ghe.local"]
api_base_url = "{uri}/api/v3"
graphql_url = "{uri}/api/graphql"
"#,
            uri = server.uri()
        ),
    )
    .await?;

    let db = Arc::new(Db::open(temp.path()).await?);
    let service_prefix = format!(
        "pr-cockpit-ghe-token-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let keyring_store = Arc::new(KeyringTokenStore::new(service_prefix.clone()));

    let probe_secret = StoredTokenSecret {
        access_token: "probe".to_string(),
        refresh_token: None,
        refresh_token_expires_in: None,
    };
    if keyring_store
        .put("ghe.local", "probe-user", &probe_secret)
        .is_err()
    {
        eprintln!("skipping keyring-backed test: keyring backend unavailable");
        return Ok(());
    }
    let _ = keyring_store.delete("ghe.local", "probe-user");

    let captured = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer {
        events: Arc::clone(&captured),
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        keyring_store,
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        AuthConfig {
            endpoint_overrides_path: Some(hosts_path),
            ..AuthConfig::default()
        },
    ));
    let saved = auth_pat_save_impl(
        auth.as_ref(),
        PatSaveInput {
            host: Some("ghe.local".to_string()),
            token: GHE_TOKEN.to_string(),
        },
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    assert_eq!(saved.host, "ghe.local");
    assert_eq!(saved.login, "ghe-keyring-user");

    let token_regex = Regex::new(r"(gh[pousr]_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})")?;
    let rows = sqlx::query("SELECT * FROM accounts")
        .fetch_all(db.pool())
        .await?;
    for row in rows {
        for (column_index, _) in row.columns().iter().enumerate() {
            if let Ok(value) = row.try_get::<String, _>(column_index) {
                assert!(
                    !token_regex.is_match(&value),
                    "token regex matched accounts.{column_index} value"
                );
                assert!(
                    !value.contains(GHE_TOKEN),
                    "accounts table leaked raw GHE token bytes"
                );
            }
        }
    }

    let keyring_secret = auth
        .token_store()
        .get(&saved.host, &saved.login)
        .map_err(|error| anyhow::anyhow!("keyring read failed: {error:?}"))?;
    let Some(keyring_secret) = keyring_secret else {
        eprintln!("skipping keyring-backed assertion: secure backend did not persist test entry");
        return Ok(());
    };
    assert_eq!(keyring_secret.access_token, GHE_TOKEN);
    let _ = auth.token_store().delete(&saved.host, &saved.login);

    let events = captured.lock().expect("capture lock poisoned");
    for event in events.iter() {
        assert!(
            !event.contains(GHE_TOKEN),
            "captured tracing output leaked GHE token bytes"
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

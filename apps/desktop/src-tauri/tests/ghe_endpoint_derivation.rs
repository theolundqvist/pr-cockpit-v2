use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::auth::{
    auth_list_accounts_impl, token_client::resolve_login_for_host_with_client, AuthConfig,
    AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::Db;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct MemoryTokenStore {
    tokens: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl TokenStore for MemoryTokenStore {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError> {
        self.tokens
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .insert((host.to_string(), login.to_string()), token.clone());
        Ok(())
    }

    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError> {
        Ok(self
            .tokens
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .get(&(host.to_string(), login.to_string()))
            .cloned())
    }

    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError> {
        self.tokens
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

fn build_auth(db: Arc<Db>, config: AuthConfig) -> AuthService {
    AuthService::with_dependencies(
        db,
        Arc::new(MemoryTokenStore::default()),
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        config,
    )
}

#[tokio::test]
async fn derives_default_dotcom_and_ghe_endpoints() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let auth = build_auth(Arc::clone(&db), AuthConfig::default());

    let dotcom = auth.endpoint_config_for_host("github.com");
    assert_eq!(dotcom.api_base_url, "https://api.github.com");
    assert_eq!(dotcom.graphql_url, "https://api.github.com/graphql");

    let ghe = auth.endpoint_config_for_host("ghe.example.com");
    assert_eq!(ghe.api_base_url, "https://ghe.example.com/api/v3");
    assert_eq!(ghe.graphql_url, "https://ghe.example.com/api/graphql");
    Ok(())
}

#[tokio::test]
async fn hosts_toml_override_wins_for_matching_host() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let hosts_path = temp.path().join("hosts.toml");
    tokio::fs::write(
        &hosts_path,
        r#"[hosts."ghe.example.com"]
api_base_url = "http://127.0.0.1:18111/proxy/api"
graphql_url = "http://127.0.0.1:18111/proxy/graphql"
"#,
    )
    .await?;

    let auth = build_auth(
        Arc::clone(&db),
        AuthConfig {
            endpoint_overrides_path: Some(hosts_path),
            ..AuthConfig::default()
        },
    );

    let overridden = auth.endpoint_config_for_host("ghe.example.com");
    assert_eq!(overridden.api_base_url, "http://127.0.0.1:18111/proxy/api");
    assert_eq!(
        overridden.graphql_url,
        "http://127.0.0.1:18111/proxy/graphql"
    );

    db.upsert_auth_account("ghe.example.com", "ghe-user", "pat", "repo", 1)
        .await?;
    let listed = auth_list_accounts_impl(&auth)
        .await
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    let account = listed
        .accounts
        .into_iter()
        .find(|entry| entry.host == "ghe.example.com")
        .expect("ghe account should be returned");
    assert_eq!(account.api_base_url, "http://127.0.0.1:18111/proxy/api");
    assert_eq!(account.graphql_url, "http://127.0.0.1:18111/proxy/graphql");
    Ok(())
}

#[tokio::test]
async fn resolve_login_for_host_targets_derived_and_overridden_api_bases() -> Result<()> {
    let server = MockServer::start().await;
    let hosts_path = tempfile::NamedTempFile::new()?;
    tokio::fs::write(
        hosts_path.path(),
        format!(
            r#"[hosts."github.com"]
api_base_url = "{uri}/dotcom"
graphql_url = "{uri}/dotcom-graphql"

[hosts."ghe.local"]
api_base_url = "{uri}/api/v3"
graphql_url = "{uri}/api/graphql"
"#,
            uri = server.uri()
        ),
    )
    .await?;
    let original_hosts_path = std::env::var_os("PR_COCKPIT_HOSTS_TOML");
    std::env::set_var("PR_COCKPIT_HOSTS_TOML", hosts_path.path());

    let _dotcom_user = Mock::given(method("GET"))
        .and(path("/dotcom/user"))
        .and(header("authorization", "Bearer dotcom-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"login":"dotcom-user"}))
                .insert_header("x-oauth-scopes", "repo,read:org"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let _ghe_user = Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", "Bearer ghe-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"login":"ghe-user"}))
                .insert_header("x-oauth-scopes", "repo,notifications"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;

    let http = reqwest::Client::new();
    let dotcom = resolve_login_for_host_with_client(&http, "github.com", "dotcom-token").await?;
    assert_eq!(dotcom.login, "dotcom-user");
    assert_eq!(dotcom.scopes, vec!["read:org", "repo"]);

    let resolved = resolve_login_for_host_with_client(&http, "ghe.local", "ghe-token").await?;
    assert_eq!(resolved.login, "ghe-user");
    assert_eq!(resolved.scopes, vec!["notifications", "repo"]);

    if let Some(previous) = original_hosts_path {
        std::env::set_var("PR_COCKPIT_HOSTS_TOML", previous);
    } else {
        std::env::remove_var("PR_COCKPIT_HOSTS_TOML");
    }
    Ok(())
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::api::{GithubApiConfig, GithubClient};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AccountLocator, AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret,
    TokenStore,
};
use desktop_lib::db::Db;
use desktop_lib::sync::RateLimitBudgeter;

#[allow(dead_code)]
pub struct TestHarness {
    pub db: Arc<Db>,
    pub github: GithubClient,
    pub locator: AccountLocator,
    pub account_id: String,
    pub budgeter: RateLimitBudgeter,
    _temp: tempfile::TempDir,
    _auth: Arc<AuthService>,
}

pub async fn build_harness(base_url: &str, token: &str, login: &str) -> Result<TestHarness> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let host = "github.com".to_string();
    let account = db
        .upsert_auth_account(&host, login, "pat", "repo,read:org", 1)
        .await?;

    let token_store = Arc::new(StaticTokenStore::new(
        &host,
        login,
        StoredTokenSecret {
            access_token: token.to_string(),
            refresh_token: None,
            refresh_token_expires_in: None,
        },
    ));
    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        token_store,
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        AuthConfig {
            github_web_origin: base_url.to_string(),
            github_api_origin: base_url.to_string(),
            ..AuthConfig::default()
        },
    ));

    let token_client = TokenClient::new(Arc::clone(&auth), reqwest::Client::new());
    let github = GithubClient::with_config(
        token_client,
        Arc::clone(&db),
        GithubApiConfig {
            api_origin: base_url.to_string(),
            graphql_origin: format!("{base_url}/graphql"),
        },
    );
    let budgeter = RateLimitBudgeter::start(Arc::clone(&db));
    Ok(TestHarness {
        db,
        github,
        locator: AccountLocator {
            host,
            login: login.to_string(),
        },
        account_id: account.id,
        budgeter,
        _temp: temp,
        _auth: auth,
    })
}

#[derive(Default)]
struct StaticTokenStore {
    values: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl StaticTokenStore {
    fn new(host: &str, login: &str, token: StoredTokenSecret) -> Self {
        let mut values = HashMap::new();
        values.insert((host.to_string(), login.to_string()), token);
        Self {
            values: Mutex::new(values),
        }
    }
}

impl TokenStore for StaticTokenStore {
    fn put(&self, host: &str, login: &str, token: &StoredTokenSecret) -> Result<(), AuthError> {
        self.values
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .insert((host.to_string(), login.to_string()), token.clone());
        Ok(())
    }

    fn get(&self, host: &str, login: &str) -> Result<Option<StoredTokenSecret>, AuthError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| AuthError::OAuthFlowFailed)?
            .get(&(host.to_string(), login.to_string()))
            .cloned())
    }

    fn delete(&self, host: &str, login: &str) -> Result<(), AuthError> {
        self.values
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

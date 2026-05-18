use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::api::{GithubApiConfig, GithubClient};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::Db;
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::MutationKind;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "mutations_harness.rs"]
mod mutations_harness;

const HOST: &str = "github.com";
const LOGIN_A: &str = "posting-account-a";
const LOGIN_B: &str = "posting-account-b";
const TOKEN_A: &str = "token-a";
const TOKEN_B: &str = "token-b";

#[tokio::test]
async fn submit_uses_posting_account_token_when_provided() -> Result<()> {
    let server = MockServer::start().await;
    let comment_path = "/repos/octo/hello-world/issues/1/comments";
    Mock::given(method("POST"))
        .and(path(comment_path))
        .and(header("authorization", format!("Bearer {TOKEN_B}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 42,
            "node_id": "comment-server-b",
            "body": "server-b-comment",
            "created_at": "2026-05-18T00:00:00Z",
            "updated_at": "2026-05-18T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(comment_path))
        .and(header("authorization", format!("Bearer {TOKEN_A}")))
        .respond_with(ResponseTemplate::new(401))
        .expect(0)
        .mount(&server)
        .await;

    let harness = build_two_account_harness(&server.uri()).await?;

    let mut payload = mutations_harness::payload_for_kind(MutationKind::AddComment, "posting-id");
    payload.posting_account_id = Some(harness.account_b_id.clone());

    let submission = harness
        .engine
        .submit(&harness.account_a_id, payload)
        .await?;
    let summary = harness.engine.drain().await?;
    assert_eq!(summary.failed, 0);

    let queued_account: String = sqlx::query_scalar(
        "SELECT account_id
         FROM pending_mutations
         WHERE id = ?1",
    )
    .bind(&submission.mutation_id)
    .fetch_one(harness.db.pool())
    .await?;
    assert_eq!(
        queued_account, harness.account_b_id,
        "queue row should be attributed to posting identity"
    );

    Ok(())
}

#[tokio::test]
async fn submit_rejects_unknown_posting_account_id() -> Result<()> {
    let server = MockServer::start().await;
    let harness = build_two_account_harness(&server.uri()).await?;

    let mut payload = mutations_harness::payload_for_kind(MutationKind::AddComment, "missing");
    payload.posting_account_id = Some("github.com:missing-user".to_string());

    let error = harness
        .engine
        .submit(&harness.account_a_id, payload)
        .await
        .expect_err("unknown posting account should be rejected");
    assert!(error.to_string().contains("not configured"));

    Ok(())
}

struct PostingHarness {
    db: Arc<Db>,
    engine: MutationEngine,
    account_a_id: String,
    account_b_id: String,
    _temp: tempfile::TempDir,
}

async fn build_two_account_harness(base_url: &str) -> Result<PostingHarness> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let account_a = db
        .upsert_auth_account(HOST, LOGIN_A, "pat", "repo,read:org", 1)
        .await?;
    let account_b = db
        .upsert_auth_account(HOST, LOGIN_B, "pat", "repo,read:org", 1)
        .await?;
    mutations_harness::seed_graph(&db, &account_a.id).await?;
    mutations_harness::seed_graph(&db, &account_b.id).await?;

    let token_store = Arc::new(MapTokenStore::new([
        ((HOST.to_string(), LOGIN_A.to_string()), TOKEN_A.to_string()),
        ((HOST.to_string(), LOGIN_B.to_string()), TOKEN_B.to_string()),
    ]));
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
    let token_client = TokenClient::new(auth, reqwest::Client::new());
    let github = GithubClient::with_config(
        token_client,
        Arc::clone(&db),
        GithubApiConfig {
            api_origin: base_url.to_string(),
            graphql_origin: format!("{base_url}/graphql"),
        },
    );
    let engine = MutationEngine::new(Arc::clone(&db), github);

    Ok(PostingHarness {
        db,
        engine,
        account_a_id: account_a.id,
        account_b_id: account_b.id,
        _temp: temp,
    })
}

#[derive(Default)]
struct MapTokenStore {
    values: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl MapTokenStore {
    fn new(entries: [((String, String), String); 2]) -> Self {
        let mut values = HashMap::new();
        for ((host, login), token) in entries {
            values.insert(
                (host, login),
                StoredTokenSecret {
                    access_token: token,
                    refresh_token: None,
                    refresh_token_expires_in: None,
                },
            );
        }
        Self {
            values: Mutex::new(values),
        }
    }
}

impl TokenStore for MapTokenStore {
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

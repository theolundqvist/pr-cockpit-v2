#![recursion_limit = "512"]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use desktop_lib::api::check_logs::{stream_check_run_log, CheckLogStreamRequest};
use desktop_lib::api::{
    AccountResolver, GithubClient, ResolvedAccountEndpoints, ENQUEUE_PULL_REQUEST_MUTATION,
    UPDATE_PULL_REQUEST_BASE_MUTATION,
};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AccountLocator, AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret,
    TokenStore,
};
use desktop_lib::db::Db;
use desktop_lib::ipc::upload_image_to_github_user_content_impl;
use desktop_lib::mutations::{MutationEngine, MutationKind};
use desktop_lib::range_diff::{
    RangeDiffSource, RangeDiffSourceProvider, RepoLocator, RestCompareRangeDiff,
};
use desktop_lib::stacks::list_stacks;
use desktop_lib::sync::{Priority, RateLimitBudgeter, RealActions, Tier, TierActions};
use reqwest::Method;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "fixtures/ghe/mod.rs"]
mod ghe_fixture;
mod mutations_harness;

#[derive(Default)]
struct MemoryTokenStore {
    values: Mutex<HashMap<(String, String), StoredTokenSecret>>,
}

impl MemoryTokenStore {
    fn insert(&self, host: &str, login: &str, token: &str) {
        self.values
            .lock()
            .expect("token store lock poisoned")
            .insert(
                (host.to_string(), login.to_string()),
                StoredTokenSecret {
                    access_token: token.to_string(),
                    refresh_token: None,
                    refresh_token_expires_in: None,
                },
            );
    }
}

impl TokenStore for MemoryTokenStore {
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

#[derive(Clone)]
struct AuthResolver {
    auth: Arc<AuthService>,
}

#[async_trait]
impl AccountResolver for AuthResolver {
    async fn resolve(&self, account_id: &str) -> Result<ResolvedAccountEndpoints> {
        let (account, secret) = self.auth.account_secret_by_id(account_id).await?;
        let endpoints = self.auth.endpoint_config_for_host(&account.host);
        Ok(ResolvedAccountEndpoints {
            locator: AccountLocator {
                host: account.host,
                login: account.login,
            },
            token: secret.access_token,
            api_base_url: endpoints.api_base_url,
            graphql_url: endpoints.graphql_url,
        })
    }
}

#[tokio::test]
async fn ghe_full_parity_covers_m1_to_m5_happy_paths_without_dotcom_leaks() -> Result<()> {
    let ghe_server = MockServer::start().await;
    ghe_fixture::mount_full_ghe_fixture(&ghe_server).await;

    let dotcom_trap = MockServer::start().await;
    let _dotcom_guard = Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount_as_scoped(&dotcom_trap)
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
            uri = ghe_server.uri()
        ),
    )
    .await?;

    let db = Arc::new(Db::open(temp.path()).await?);
    let ghe_account = db
        .upsert_auth_account("ghe.local", "ghe-user", "pat", "repo", 1)
        .await?;
    let mutation_account = db
        .upsert_auth_account("ghe.local", "ghe-mutations", "pat", "repo", 1)
        .await?;
    db.set_active_account_id(&ghe_account.id, 1).await?;

    let token = "ghe-token-parity-123";
    let token_store = Arc::new(MemoryTokenStore::default());
    token_store.insert("ghe.local", "ghe-user", token);
    token_store.insert("ghe.local", "ghe-mutations", token);

    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        token_store,
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        AuthConfig {
            github_api_origin: dotcom_trap.uri(),
            endpoint_overrides_path: Some(hosts_path),
            ..AuthConfig::default()
        },
    ));

    let github = GithubClient::with_account_resolver(
        TokenClient::new(Arc::clone(&auth), reqwest::Client::new()),
        Arc::clone(&db),
        Arc::new(AuthResolver {
            auth: Arc::clone(&auth),
        }),
        "https://api.github.com/zen".to_string(),
    );

    let budgeter = RateLimitBudgeter::start(Arc::clone(&db));
    let actions = RealActions::new(
        Arc::clone(&db),
        github.clone(),
        ghe_account.id.clone(),
        AccountLocator {
            host: "ghe.local".to_string(),
            login: "ghe-user".to_string(),
        },
        budgeter.clone(),
    );

    let _warm_targets = actions.run_tier(Tier::Warm, Priority::Foreground).await?;
    let targets = actions
        .refresh_inbox(
            &[
                "PR_NODE_1".to_string(),
                "PR_NODE_2".to_string(),
                "PR_NODE_3".to_string(),
            ],
            Priority::Foreground,
        )
        .await?;
    actions.run_refetch(targets, Priority::Foreground).await?;

    let inbox_rows = db.list_inbox_all_accounts(Some(&ghe_account.id)).await?;
    assert_eq!(inbox_rows.len(), 3);
    let pr_summary = db
        .pr_detail_summary(&ghe_account.id, "PR_NODE_1")
        .await?
        .expect("PR detail should be present after refetch");
    assert_eq!(pr_summary.title, "GHE seeded PR #7");

    let stacks = list_stacks(db.as_ref(), &ghe_account.id, "1").await?;
    assert!(
        !stacks.is_empty(),
        "stack detection should run for GHE account"
    );

    let (diff_response, _) = github
        .rest_get_stream(&ghe_account.id, "/repos/ghe-org/ghe-repo/pulls/7.diff")
        .await?;
    let diff_body = diff_response.text().await?;
    assert!(diff_body.contains("diff --git"));

    let (notifications, rate_limit) = github
        .rest_get_json::<Vec<serde_json::Value>>(&ghe_account.id, "/notifications")
        .await?;
    assert!(!notifications.is_empty());
    assert!(rate_limit.is_some());

    let mut log_chunks = Vec::new();
    stream_check_run_log(
        &github,
        CheckLogStreamRequest {
            account_id: &ghe_account.id,
            owner: "ghe-org",
            repo: "ghe-repo",
            check_run_id: "CHECK_RUN_1",
            details_url: Some("https://ghe.local/ghe-org/ghe-repo/actions/runs/7001/jobs/9001"),
            tail_lines: 2,
        },
        |chunk| {
            log_chunks.push(chunk);
            Ok(())
        },
    )
    .await?;
    assert!(log_chunks.iter().any(|chunk| chunk.kind == "tail"));

    mutations_harness::seed_graph(&db, &mutation_account.id).await?;
    let engine = MutationEngine::new(Arc::clone(&db), github.clone());
    for kind in [
        MutationKind::AddComment,
        MutationKind::EditComment,
        MutationKind::AddLabel,
        MutationKind::SetAssignees,
        MutationKind::ResolveThread,
        MutationKind::RerunCheckRun,
        MutationKind::RerunCheckSuite,
        MutationKind::ApplySuggestion,
    ] {
        let payload = mutations_harness::payload_for_kind(kind, "ghe");
        let _submitted = engine.submit(&mutation_account.id, payload).await?;
    }
    let drain_summary = engine.drain().await?;
    assert_eq!(drain_summary.failed, 0);

    let (_merge_payload, _merge_rate) = github
        .rest_mutation_json::<serde_json::Value>(
            &ghe_account.id,
            Method::PUT,
            "/repos/ghe-org/ghe-repo/pulls/7/merge",
            Some(serde_json::json!({"merge_method": "squash"})),
            None,
        )
        .await?;

    let _ = github
        .graphql_mutation::<serde_json::Value>(
            &ghe_account.id,
            ENQUEUE_PULL_REQUEST_MUTATION,
            serde_json::json!({
                "input": {
                    "pullRequestId": "PR_NODE_2"
                }
            }),
            None,
        )
        .await?;
    let _ = github
        .graphql_mutation::<serde_json::Value>(
            &ghe_account.id,
            UPDATE_PULL_REQUEST_BASE_MUTATION,
            serde_json::json!({
                "input": {
                    "pullRequestId": "PR_NODE_2",
                    "baseRefName": "main"
                }
            }),
            None,
        )
        .await?;

    let upload = upload_image_to_github_user_content_impl(
        db.as_ref(),
        &github,
        ghe_account.id.clone(),
        vec![137, 80, 78, 71, 1, 2, 3, 4],
        "image/png".to_string(),
    )
    .await
    .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
    assert!(upload.url.contains("/user-attachments/files/"));

    let provider = RestCompareRangeDiff {
        github: Arc::new(github.clone()),
    };
    let range_diff = provider
        .compute(
            &ghe_account.id,
            &RepoLocator {
                owner: "ghe-org".to_string(),
                name: "ghe-repo".to_string(),
            },
            "base111",
            "head111",
            "head222",
        )
        .await?;
    assert_eq!(range_diff.mode, RangeDiffSource::RestCompare);

    let requests = ghe_server.received_requests().await.unwrap_or_default();
    assert!(!requests.is_empty());
    for request in requests {
        let body = String::from_utf8_lossy(&request.body);
        assert!(
            !body.contains(token),
            "token must never appear in request body: {}",
            request.url
        );
        assert!(
            !request.url.as_str().contains(token),
            "token must never appear in URL/query: {}",
            request.url
        );
        for (name, value) in request.headers.iter() {
            let header_value = value.to_str().unwrap_or_default();
            if name.as_str().eq_ignore_ascii_case("authorization") {
                assert!(
                    header_value.eq_ignore_ascii_case(&format!("Bearer {token}")),
                    "authorization header should be a standard bearer token"
                );
                continue;
            }
            assert!(
                !header_value.contains(token),
                "token leaked into header `{}`: {}",
                name.as_str(),
                header_value
            );
        }
    }

    Ok(())
}

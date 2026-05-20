use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use desktop_lib::api::{AccountResolver, GithubClient, ResolvedAccountEndpoints};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AccountLocator, AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret,
    TokenStore,
};
use desktop_lib::db::Db;
use desktop_lib::sync::{Priority, RateLimitBudgeter, RealActions, Tier, TierActions};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
async fn ghe_account_round_trip_uses_host_overrides_end_to_end() -> Result<()> {
    let server = MockServer::start().await;
    let _notifications = Mock::given(method("GET"))
        .and(path("/api/v3/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(Vec::<serde_json::Value>::new())
                .insert_header("x-poll-interval", "60"),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let _inbox_refresh = Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("InboxRefresh"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::from_str::<serde_json::Value>(
                r#"{
              "data": {
                "nodes": [
                  {
                    "__typename": "PullRequest",
                    "id": "PR_NODE_1",
                    "number": 7,
                    "title": "GHE seeded PR",
                    "updatedAt": "2026-05-18T00:00:00Z",
                    "state": "OPEN",
                    "isDraft": false,
                    "mergeable": "MERGEABLE",
                    "author": {
                      "id": "U_AUTHOR",
                      "login": "ghe-user",
                      "name": "GHE User",
                      "avatarUrl": null,
                      "url": null
                    },
                    "repository": {
                      "id": "R_NODE_1",
                      "owner": { "login": "ghe-org" },
                      "name": "ghe-repo"
                    },
                    "headRefOid": "headsha1",
                    "baseRefOid": "basesha1",
                    "labels": { "nodes": [] },
                    "reviewRequests": { "nodes": [] },
                    "commits": {
                      "nodes": [
                        { "commit": { "statusCheckRollup": { "state": "SUCCESS" } } }
                      ]
                    },
                    "unreadPlaceholder": true
                  }
                ]
              }
            }"#,
            )?),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let _pr_detail = Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("PrDetail"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::from_str::<serde_json::Value>(
                r#"{
              "data": {
                "repository": {
                  "id": "R_NODE_1",
                  "owner": { "login": "ghe-org" },
                  "name": "ghe-repo",
                  "pullRequest": {
                    "id": "PR_NODE_1",
                    "number": 7,
                    "title": "GHE seeded PR",
                    "body": "This PR was fetched from a GHE host override.",
                    "state": "OPEN",
                    "isDraft": false,
                    "mergeable": "MERGEABLE",
                    "mergeStateStatus": "CLEAN",
                    "viewerCanMerge": true,
                    "viewerCanEnableAutoMerge": true,
                    "viewerCanDisableAutoMerge": true,
                    "viewerCanUpdateBranch": true,
                    "viewerCanDeleteHeadRef": true,
                    "autoMergeRequest": null,
                    "mergeQueueEntry": null,
                    "createdAt": "2026-05-18T00:00:00Z",
                    "updatedAt": "2026-05-18T00:00:00Z",
                    "closedAt": null,
                    "mergedAt": null,
                    "additions": 1,
                    "deletions": 1,
                    "changedFiles": 1,
                    "comments": { "totalCount": 0 },
                    "commits": {
                      "totalCount": 1,
                      "nodes": [
                        { "commit": { "oid": "headsha1", "checkSuites": { "nodes": [] } } }
                      ]
                    },
                    "author": {
                      "id": "U_AUTHOR",
                      "login": "ghe-user",
                      "name": "GHE User",
                      "avatarUrl": null,
                      "url": "https://ghe.local/u/ghe-user"
                    },
                    "baseRefName": "main",
                    "baseRefOid": "basesha1",
                    "headRefName": "feature/ghe",
                    "headRef": { "id": "HEAD_REF_1", "name": "feature/ghe" },
                    "headRefOid": "headsha1",
                    "repository": {
                      "mergeCommitAllowed": true,
                      "squashMergeAllowed": true,
                      "rebaseMergeAllowed": true,
                      "deleteBranchOnMerge": false,
                      "mergeQueue": null,
                      "defaultBranchRef": { "branchProtectionRule": null }
                    },
                    "headRepository": { "id": "R_NODE_1" },
                    "labels": { "nodes": [] },
                    "assignees": { "nodes": [] },
                    "reviewRequests": { "nodes": [] },
                    "reviewThreads": { "nodes": [] },
                    "reviews": { "nodes": [] },
                    "timelineItems": { "nodes": [] }
                  }
                }
              }
            }"#,
            )?),
        )
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let _pull_diff = Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghe-org/ghe-repo/pulls/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-remaining", "4500")
                .insert_header("x-ratelimit-limit", "5000")
                .insert_header("x-ratelimit-reset", "1999999999")
                .insert_header("x-ratelimit-used", "500")
                .set_body_string("diff --git a/file.txt b/file.txt\n@@ -1 +1 @@\n-old\n+new\n"),
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
    let dotcom_account = db
        .upsert_auth_account("github.com", "dotcom-user", "pat", "repo", 1)
        .await?;
    let ghe_account = db
        .upsert_auth_account("ghe.local", "ghe-user", "pat", "repo", 1)
        .await?;
    db.set_active_account_id(&ghe_account.id, 1).await?;

    let token_store = Arc::new(MemoryTokenStore::default());
    token_store.insert("github.com", "dotcom-user", "dotcom-token");
    token_store.insert("ghe.local", "ghe-user", "ghe-token");
    let auth = Arc::new(AuthService::with_dependencies(
        Arc::clone(&db),
        token_store,
        Arc::new(StubGhCli),
        reqwest::Client::new(),
        AuthConfig {
            github_api_origin: "http://127.0.0.1:9".to_string(),
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
        github,
        ghe_account.id.clone(),
        AccountLocator {
            host: "ghe.local".to_string(),
            login: "ghe-user".to_string(),
        },
        budgeter,
    );

    let warm_targets = actions.run_tier(Tier::Warm, Priority::Foreground).await?;
    assert!(warm_targets.is_empty(), "notifications should be empty");

    let inbox_targets = actions
        .refresh_inbox(&["PR_NODE_1".to_string()], Priority::Foreground)
        .await?;
    assert_eq!(inbox_targets.len(), 1);
    actions
        .run_refetch(inbox_targets.clone(), Priority::Foreground)
        .await?;

    let inbox_rows = db.list_inbox_all_accounts(Some(&ghe_account.id)).await?;
    assert_eq!(inbox_rows.len(), 1);
    assert_eq!(inbox_rows[0].title, "GHE seeded PR");
    assert_eq!(inbox_rows[0].repo_owner, "ghe-org");
    assert_eq!(inbox_rows[0].repo_name, "ghe-repo");

    let summary = db
        .pr_detail_summary(&ghe_account.id, "PR_NODE_1")
        .await?
        .expect("PR detail should be materialized");
    assert_eq!(summary.title, "GHE seeded PR");
    assert_eq!(summary.pr_number, 7);

    let dotcom_rows = db.list_inbox_all_accounts(Some(&dotcom_account.id)).await?;
    assert!(
        dotcom_rows.is_empty(),
        "GHE sync must not write through the github.com account"
    );
    Ok(())
}

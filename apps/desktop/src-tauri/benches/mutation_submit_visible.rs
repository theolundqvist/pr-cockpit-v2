use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use criterion::{criterion_group, criterion_main, Criterion};
use desktop_lib::api::{GithubApiConfig, GithubClient};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::{Db, PullRequestRecord, RepoRecord, UserRecord};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::{MutationEvent, MutationKind, SubmitPayload};
use tokio::runtime::Runtime;

const HOST: &str = "github.com";
const LOGIN: &str = "bench";
const USER_ID: &str = "user-stub";
const REPO_ID: &str = "repo-bench";
const PR_ID: &str = "pr-bench";

fn mutation_submit_visible(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let (engine, account_id, _temp) = runtime.block_on(async {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let db = Arc::new(Db::open(temp.path()).await.expect("open db"));
        let account = db
            .upsert_auth_account(HOST, LOGIN, "pat", "repo", 1)
            .await
            .expect("upsert account");

        seed_graph(db.as_ref(), &account.id)
            .await
            .expect("seed graph");

        let token_store = Arc::new(StaticTokenStore::new(
            HOST,
            LOGIN,
            StoredTokenSecret {
                access_token: "token".to_string(),
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
                github_web_origin: "https://github.com".to_string(),
                github_api_origin: "https://api.github.com".to_string(),
                ..AuthConfig::default()
            },
        ));

        let token_client = TokenClient::new(auth, reqwest::Client::new());
        let github =
            GithubClient::with_config(token_client, Arc::clone(&db), GithubApiConfig::default());

        let engine = MutationEngine::new(db, github);
        (engine, account.id, temp)
    });

    let mut events = engine.subscribe();
    let idempotency_seq = AtomicU64::new(0);

    c.bench_function("mutation_submit_visible", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let sequence = idempotency_seq.fetch_add(1, Ordering::Relaxed);
                let started = Instant::now();
                runtime.block_on(async {
                    let payload = SubmitPayload {
                        kind: MutationKind::SetAssignees,
                        target_type: "pull_request".to_string(),
                        target_id: PR_ID.to_string(),
                        idempotency_key: format!("bench-idem-{sequence}"),
                        input_json: serde_json::json!({
                            "pr_id": PR_ID,
                            "user_id": USER_ID,
                        }),
                    };
                    let submitted = engine
                        .submit(&account_id, payload)
                        .await
                        .expect("submit mutation");

                    loop {
                        let event = events.recv().await.expect("receive mutation event");
                        if let MutationEvent::Submitted { mutation } = event {
                            if mutation.id == submitted.mutation_id {
                                break;
                            }
                        }
                    }
                });
                total += started.elapsed();
            }
            total
        });
    });
}

async fn seed_graph(db: &Db, account_id: &str) -> Result<()> {
    db.upsert_repo(&RepoRecord {
        id: REPO_ID.to_string(),
        account_id: account_id.to_string(),
        owner: "octo".to_string(),
        name: "bench".to_string(),
        default_branch: Some("main".to_string()),
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;

    db.upsert_user(&UserRecord {
        id: USER_ID.to_string(),
        account_id: account_id.to_string(),
        login: "bench-user".to_string(),
        display_name: None,
        avatar_url: None,
        html_url: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;

    db.upsert_pull_request(&PullRequestRecord {
        id: PR_ID.to_string(),
        account_id: account_id.to_string(),
        repo_id: REPO_ID.to_string(),
        number: 1,
        state: "open".to_string(),
        draft: false,
        title: "bench".to_string(),
        body: "bench".to_string(),
        author_id: Some(USER_ID.to_string()),
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "head".to_string(),
        head_repo_id: Some(REPO_ID.to_string()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: Some("CLEAN".to_string()),
        additions: 0,
        deletions: 0,
        changed_files: 0,
        comments_count: 0,
        reviews_count: 0,
        commits_count: 0,
        is_read: true,
        html_url: None,
        created_at: 1,
        updated_at: 1,
        closed_at: None,
        merged_at: None,
    })
    .await?;

    Ok(())
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

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(250))
        .measurement_time(Duration::from_secs(2));
    targets = mutation_submit_visible
}
criterion_main!(benches);

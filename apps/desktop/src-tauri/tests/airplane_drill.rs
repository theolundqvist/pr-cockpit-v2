use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use desktop_lib::api::{GithubApiConfig, GithubClient};
use desktop_lib::auth::token_client::TokenClient;
use desktop_lib::auth::{
    AuthConfig, AuthError, AuthService, GhCli, GhCommandOutput, StoredTokenSecret, TokenStore,
};
use desktop_lib::db::{
    Db, DraftRecord, PrLabelRecord, PullRequestRecord, RepoRecord, ReviewThreadRecord, UserRecord,
};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::net::{NetProbe, NetState, NetworkMonitor};
use desktop_lib::mutations::{MutationKind, SubmitPayload};
use desktop_lib::sync::Clock;
use reqwest::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const HOST: &str = "github.com";
const LOGIN: &str = "airplane";
const OWNER: &str = "octo";
const REPO: &str = "hello-world";
const ACCOUNT_USER_ID: &str = "user-airplane";
const REPO_ID: &str = "repo-airplane";
const PR1_ID: &str = "pr-air-1";
const PR2_ID: &str = "pr-air-2";

#[tokio::test]
async fn airplane_offline_queue_replay_drill() -> Result<()> {
    let server = MockServer::start().await;
    mount_airplane_mocks(&server).await;

    let db = Arc::new(Db::open_fixture().await?);
    let account = db
        .upsert_auth_account(HOST, LOGIN, "pat", "repo", 1)
        .await?;
    sqlx::query("DELETE FROM pending_mutations WHERE account_id != ?1")
        .bind(&account.id)
        .execute(db.pool())
        .await?;
    seed_drill_graph(&db, &account.id).await?;

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
            github_web_origin: server.uri(),
            github_api_origin: server.uri(),
            ..AuthConfig::default()
        },
    ));
    let token_client = TokenClient::new(auth, reqwest::Client::new());
    let github = GithubClient::with_config(
        token_client,
        Arc::clone(&db),
        GithubApiConfig {
            api_origin: server.uri(),
            graphql_origin: format!("{}/graphql", server.uri()),
        },
    );

    let probe = Arc::new(KillSwitchProbe::new(ProbeState::Online));
    let monitor = NetworkMonitor::start_with_probe_and_clock(
        server.uri(),
        probe.clone(),
        Arc::new(PendingClock),
        Duration::from_secs(15),
    );
    let mut net_rx = monitor.subscribe();

    probe.set(ProbeState::Offline);
    monitor.probe_now().await;
    net_rx.changed().await?;
    assert!(matches!(
        *net_rx.borrow(),
        NetState::Offline {
            error_kind: desktop_lib::mutations::ErrorKind::Server
        }
    ));

    let engine = MutationEngine::new(Arc::clone(&db), github.clone())
        .with_network_monitor(monitor.clone())
        .with_clock(Arc::new(ImmediateClock));

    let comment_bodies = vec![
        "offline one @octo #123",
        "offline two\n\n```rust\nfn main() {}\n```",
        "offline three #321",
        "offline four @reviewer",
        "offline five with `inline code`",
        "offline six\n- a\n- b",
        "offline seven\n\n> quoted",
        "offline eight @team/core",
        "offline nine #9",
        "offline ten fenced\n```md\n* item\n```",
    ];

    for (index, body) in comment_bodies.iter().enumerate() {
        let payload = submit_payload(
            MutationKind::AddComment,
            PR1_ID,
            1,
            format!("air-local-{index}"),
            serde_json::json!({
                "local_id": format!("air-local-{index}"),
                "author_id": ACCOUNT_USER_ID,
                "body": body,
                "kind": "issue"
            }),
        );
        let submitted = engine.submit(&account.id, payload).await?;
        assert!(!submitted.requires_confirmation);
    }

    let label_ops = vec![
        (MutationKind::AddLabel, PR1_ID, 1, "air-label-a"),
        (MutationKind::RemoveLabel, PR1_ID, 1, "air-label-r"),
        (MutationKind::AddLabel, PR2_ID, 2, "air-label-b"),
        (MutationKind::RemoveLabel, PR2_ID, 2, "air-label-s"),
        (MutationKind::AddLabel, PR1_ID, 1, "air-label-c"),
        (MutationKind::RemoveLabel, PR2_ID, 2, "air-label-t"),
    ];
    for (index, (kind, pr_id, pr_number, label)) in label_ops.into_iter().enumerate() {
        let payload = submit_payload(
            kind,
            pr_id,
            pr_number,
            format!("air-label-{index}"),
            serde_json::json!({
                "label_name": label,
                "label_color": "aabbcc"
            }),
        );
        let submitted = engine.submit(&account.id, payload).await?;
        assert!(!submitted.requires_confirmation);
    }

    for index in 0..4 {
        let thread_id = format!("thread-air-{}", index + 1);
        let payload = submit_payload(
            MutationKind::ResolveThread,
            PR1_ID,
            1,
            format!("air-thread-{index}"),
            serde_json::json!({
                "thread_id": thread_id,
                "path": format!("src/file{index}.rs"),
                "previous_is_resolved": 0,
                "previous_updated_at": 10,
                "is_outdated": 0,
                "actor_id": ACCOUNT_USER_ID
            }),
        );
        let submitted = engine.submit(&account.id, payload).await?;
        assert!(!submitted.requires_confirmation);
    }

    let merge_payload = submit_payload(
        MutationKind::Merge,
        PR1_ID,
        1,
        "air-merge".to_string(),
        serde_json::json!({
            "merge_method": "merge",
            "expected_head_sha": "head-air-1"
        }),
    );
    let merge_submitted = engine.submit(&account.id, merge_payload).await?;
    assert!(merge_submitted.requires_confirmation);

    let optimistic_comment_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM comments
         WHERE account_id = ?1 AND pr_id = ?2 AND id LIKE 'air-local-%'",
    )
    .bind(&account.id)
    .bind(PR1_ID)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(optimistic_comment_count, 10);

    let inbox_rows = db.list_inbox(&account.id).await?;
    assert_eq!(inbox_rows.len(), 2);
    let pr1_summary = db
        .pr_detail_summary(&account.id, PR1_ID)
        .await?
        .context("missing PR1 summary")?;
    assert_eq!(pr1_summary.state, "open");
    let thread_rows = db.pr_review_threads(&account.id, PR1_ID, 20, 0).await?;
    assert_eq!(
        thread_rows
            .iter()
            .filter(|row| row.is_resolved == 1)
            .count(),
        4
    );

    db.upsert_draft(&DraftRecord {
        id: format!("draft:{}:{PR1_ID}", account.id),
        account_id: account.id.clone(),
        target_type: "pull_request".to_string(),
        target_id: PR1_ID.to_string(),
        body: "draft body before conflict".to_string(),
        created_at: 100,
        updated_at: 100,
    })
    .await?;

    drop(engine);

    let pre_reboot_pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pending_mutations WHERE account_id = ?1 AND status = 'pending'",
    )
    .bind(&account.id)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(pre_reboot_pending, 21);
    assert_eq!(
        db.list_drafts(&account.id, Some("pull_request"), Some(PR1_ID))
            .await?
            .len(),
        1
    );

    let rebooted_engine = MutationEngine::new(Arc::clone(&db), github)
        .with_network_monitor(monitor.clone())
        .with_clock(Arc::new(ImmediateClock));

    probe.set(ProbeState::Online);
    monitor.probe_now().await;
    net_rx.changed().await?;
    assert_eq!(*net_rx.borrow(), NetState::Online);

    let summary = rebooted_engine.drain().await?;
    assert_eq!(summary.applied, 20);
    assert_eq!(summary.reconciled, 20);
    assert_eq!(summary.failed, 0);

    let requests = server.received_requests().await.unwrap_or_default();
    let sequence = requests
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    expected.extend(std::iter::repeat_n(
        "POST /repos/octo/hello-world/issues/1/comments".to_string(),
        10,
    ));
    expected.extend(vec![
        "POST /repos/octo/hello-world/issues/1/labels".to_string(),
        "DELETE /repos/octo/hello-world/issues/1/labels/air-label-r".to_string(),
        "POST /repos/octo/hello-world/issues/2/labels".to_string(),
        "DELETE /repos/octo/hello-world/issues/2/labels/air-label-s".to_string(),
        "POST /repos/octo/hello-world/issues/1/labels".to_string(),
        "DELETE /repos/octo/hello-world/issues/2/labels/air-label-t".to_string(),
    ]);
    expected.extend(std::iter::repeat_n("POST /graphql".to_string(), 4));
    assert_eq!(sequence, expected);

    let mapped_comments = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM id_mappings
         WHERE account_id = ?1 AND kind = 'comment' AND local_id LIKE 'air-local-%'",
    )
    .bind(&account.id)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(mapped_comments, 10);
    let remaining_local_comments = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM comments
         WHERE account_id = ?1 AND pr_id = ?2 AND id LIKE 'air-local-%'",
    )
    .bind(&account.id)
    .bind(PR1_ID)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(remaining_local_comments, 0);

    let pending_after = sqlx::query_as::<_, (String, i64)>(
        "SELECT id, requires_connection_confirmation
         FROM pending_mutations
         WHERE account_id = ?1 AND status = 'pending'
         ORDER BY created_at ASC, id ASC",
    )
    .bind(&account.id)
    .fetch_all(db.pool())
    .await?;
    assert_eq!(pending_after.len(), 1);
    assert_eq!(pending_after[0].0, merge_submitted.mutation_id);
    assert_eq!(pending_after[0].1, 1);

    let applied_after = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pending_mutations WHERE account_id = ?1 AND status = 'applied'",
    )
    .bind(&account.id)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(applied_after, 20);

    let post_summary = db
        .pr_detail_summary(&account.id, PR1_ID)
        .await?
        .context("missing PR1 summary after drain")?;
    assert_eq!(post_summary.state, "open");
    let post_threads = db.pr_review_threads(&account.id, PR1_ID, 20, 0).await?;
    assert_eq!(
        post_threads
            .iter()
            .filter(|row| row.is_resolved == 1)
            .count(),
        4
    );
    assert_eq!(
        db.list_drafts(&account.id, Some("pull_request"), Some(PR1_ID))
            .await?
            .len(),
        1
    );

    Ok(())
}

fn submit_payload(
    kind: MutationKind,
    pr_id: &str,
    pr_number: i64,
    idempotency_suffix: String,
    extra: serde_json::Value,
) -> SubmitPayload {
    let mut input = serde_json::Map::new();
    input.insert("owner".to_string(), serde_json::json!(OWNER));
    input.insert("repo".to_string(), serde_json::json!(REPO));
    input.insert("repo_id".to_string(), serde_json::json!(REPO_ID));
    input.insert("pr_id".to_string(), serde_json::json!(pr_id));
    input.insert("pr_number".to_string(), serde_json::json!(pr_number));
    input.insert(
        "pull_request_id".to_string(),
        serde_json::json!("PR_node_air"),
    );
    input.insert("author_id".to_string(), serde_json::json!(ACCOUNT_USER_ID));
    input.insert("head_sha".to_string(), serde_json::json!("head-air-1"));
    input.insert("base_sha".to_string(), serde_json::json!("base-air-1"));
    input.insert("head_ref".to_string(), serde_json::json!("feature/air"));
    input.insert("base_ref".to_string(), serde_json::json!("main"));
    input.insert("title".to_string(), serde_json::json!("Airplane PR"));
    input.insert("body".to_string(), serde_json::json!("Airplane body"));
    input.insert("state".to_string(), serde_json::json!("open"));
    input.insert("created_at".to_string(), serde_json::json!(10));
    input.insert("previous_updated_at".to_string(), serde_json::json!(10));
    if let serde_json::Value::Object(extra_map) = extra {
        input.extend(extra_map);
    }
    SubmitPayload {
        kind,
        target_type: "pull_request".to_string(),
        target_id: pr_id.to_string(),
        idempotency_key: format!("airplane-{}-{idempotency_suffix}", kind.as_str()),
        input_json: serde_json::Value::Object(input),
    }
}

async fn seed_drill_graph(db: &Db, account_id: &str) -> Result<()> {
    db.upsert_repo(&RepoRecord {
        id: REPO_ID.to_string(),
        account_id: account_id.to_string(),
        owner: OWNER.to_string(),
        name: REPO.to_string(),
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
        id: ACCOUNT_USER_ID.to_string(),
        account_id: account_id.to_string(),
        login: ACCOUNT_USER_ID.to_string(),
        display_name: None,
        avatar_url: None,
        html_url: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;
    db.upsert_pull_request(&PullRequestRecord {
        id: PR1_ID.to_string(),
        account_id: account_id.to_string(),
        repo_id: REPO_ID.to_string(),
        number: 1,
        state: "open".to_string(),
        draft: false,
        title: "Airplane PR 1".to_string(),
        body: "Body 1".to_string(),
        author_id: Some(ACCOUNT_USER_ID.to_string()),
        base_ref: "main".to_string(),
        base_sha: "base-air-1".to_string(),
        head_ref: "feature/air".to_string(),
        head_sha: "head-air-1".to_string(),
        head_repo_id: Some(REPO_ID.to_string()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: Some("CLEAN".to_string()),
        additions: 1,
        deletions: 1,
        changed_files: 1,
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
    db.upsert_pull_request(&PullRequestRecord {
        id: PR2_ID.to_string(),
        account_id: account_id.to_string(),
        repo_id: REPO_ID.to_string(),
        number: 2,
        state: "open".to_string(),
        draft: false,
        title: "Airplane PR 2".to_string(),
        body: "Body 2".to_string(),
        author_id: Some(ACCOUNT_USER_ID.to_string()),
        base_ref: "main".to_string(),
        base_sha: "base-air-2".to_string(),
        head_ref: "feature/air-2".to_string(),
        head_sha: "head-air-2".to_string(),
        head_repo_id: Some(REPO_ID.to_string()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: Some("CLEAN".to_string()),
        additions: 1,
        deletions: 1,
        changed_files: 1,
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
    for index in 0..4 {
        db.upsert_review_thread(&ReviewThreadRecord {
            id: format!("thread-air-{}", index + 1),
            account_id: account_id.to_string(),
            pr_id: PR1_ID.to_string(),
            path: format!("src/file{index}.rs"),
            line: Some(index as i64 + 1),
            side: Some("RIGHT".to_string()),
            start_line: Some(index as i64 + 1),
            start_side: Some("RIGHT".to_string()),
            original_commit_sha: None,
            original_path: None,
            original_position: None,
            original_line: None,
            is_outdated: false,
            is_resolved: false,
            resolved_by_id: None,
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    }
    db.replace_pr_labels(
        account_id,
        PR1_ID,
        &[PrLabelRecord {
            account_id: account_id.to_string(),
            pr_id: PR1_ID.to_string(),
            label_name: "air-label-r".to_string(),
            label_color: "ccddee".to_string(),
            description: None,
        }],
    )
    .await?;
    db.replace_pr_labels(
        account_id,
        PR2_ID,
        &[PrLabelRecord {
            account_id: account_id.to_string(),
            pr_id: PR2_ID.to_string(),
            label_name: "air-label-s".to_string(),
            label_color: "ccddee".to_string(),
            description: None,
        }],
    )
    .await?;
    Ok(())
}

async fn mount_airplane_mocks(server: &MockServer) {
    let comment_responses = Arc::new(Mutex::new(VecDeque::from(
        (1..=10)
            .map(|index| {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": index,
                    "node_id": format!("comment-srv-{index}"),
                    "body": format!("server-body-{index}"),
                    "created_at": "2026-05-17T00:00:00Z",
                    "updated_at": "2026-05-17T00:00:00Z"
                }))
            })
            .collect::<Vec<_>>(),
    )));
    Mock::given(method("POST"))
        .and(path("/repos/octo/hello-world/issues/1/comments"))
        .respond_with(SequenceResponder::new(comment_responses))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/repos/octo/hello-world/issues/1/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "air-label-a", "color": "aabbcc", "description": null}
        ])))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repos/octo/hello-world/issues/2/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name": "air-label-b", "color": "aabbcc", "description": null}
        ])))
        .mount(server)
        .await;
    for label in ["air-label-r", "air-label-s", "air-label-t"] {
        Mock::given(method("DELETE"))
            .and(path(format!(
                "/repos/octo/hello-world/issues/1/labels/{label}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!(
                "/repos/octo/hello-world/issues/2/labels/{label}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(server)
            .await;
    }

    let thread_responses = Arc::new(Mutex::new(VecDeque::from(
        (1..=4)
            .map(|index| {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {
                        "resolveReviewThread": {
                            "thread": {
                                "id": format!("thread-air-{index}"),
                                "path": format!("src/file{}.rs", index - 1),
                                "line": index,
                                "side": "RIGHT",
                                "startLine": index,
                                "startSide": "RIGHT",
                                "isOutdated": false,
                                "isResolved": true,
                                "updatedAt": "2026-05-17T00:00:00Z",
                                "resolvedBy": {"id": ACCOUNT_USER_ID}
                            }
                        }
                    }
                }))
            })
            .collect::<Vec<_>>(),
    )));
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(SequenceResponder::new(thread_responses))
        .mount(server)
        .await;
}

#[derive(Clone)]
struct SequenceResponder {
    responses: Arc<Mutex<VecDeque<ResponseTemplate>>>,
}

impl SequenceResponder {
    fn new(responses: Arc<Mutex<VecDeque<ResponseTemplate>>>) -> Self {
        Self { responses }
    }
}

impl Respond for SequenceResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.responses
            .lock()
            .expect("sequence responder lock poisoned")
            .pop_front()
            .unwrap_or_else(|| {
                ResponseTemplate::new(500).set_body_string("sequence responder exhausted")
            })
    }
}

#[derive(Clone, Copy)]
enum ProbeState {
    Offline = 0,
    Online = 1,
}

#[derive(Clone)]
struct KillSwitchProbe {
    state: Arc<AtomicU8>,
}

impl KillSwitchProbe {
    fn new(initial: ProbeState) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(initial as u8)),
        }
    }

    fn set(&self, next: ProbeState) {
        self.state.store(next as u8, Ordering::Relaxed);
    }
}

impl NetProbe for KillSwitchProbe {
    fn head<'a>(
        &'a self,
        _url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<StatusCode>> + Send + 'a>> {
        Box::pin(async move {
            let status = if self.state.load(Ordering::Relaxed) == ProbeState::Online as u8 {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            Ok(status)
        })
    }
}

#[derive(Default)]
struct ImmediateClock;

impl Clock for ImmediateClock {
    fn sleep<'a>(&'a self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

#[derive(Default)]
struct PendingClock;

impl Clock for PendingClock {
    fn sleep<'a>(&'a self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
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

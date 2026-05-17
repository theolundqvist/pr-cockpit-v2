use std::sync::Arc;

use anyhow::Result;
use desktop_lib::db::{
    CommentRecord, PullRequestRecord, RepoRecord, ReviewThreadRecord, UserRecord,
};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::ipc_types::SubmitPayload;
use desktop_lib::mutations::MutationKind;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "support/mod.rs"]
mod support;

const OWNER: &str = "octo";
const REPO: &str = "hello-world";
const PR_ID: &str = "pr-review-comments";
const REPO_ID: &str = "repo-review-comments";
const USER_ID: &str = "user-reviewer";

#[tokio::test]
async fn review_comment_happy_path_line_multiline_and_reply_match_snapshot() -> Result<()> {
    let server = MockServer::start().await;
    mount_review_comment_success(&server).await;
    let harness = support::build_harness(&server.uri(), "token", "review-comment").await?;
    seed_review_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    let line_submission = engine
        .submit(
            &harness.account_id,
            SubmitPayload {
                kind: MutationKind::AddReviewComment,
                target_type: "pull_request".to_string(),
                target_id: PR_ID.to_string(),
                idempotency_key: "idem-review-line".to_string(),
                input_json: serde_json::json!({
                    "owner": OWNER,
                    "repo": REPO,
                    "repo_id": REPO_ID,
                    "pr_id": PR_ID,
                    "pr_number": 1,
                    "pull_request_id": "PR_node_review",
                    "author_id": USER_ID,
                    "head_sha": "head",
                    "body": "line comment",
                    "path": "src/lib.rs",
                    "line": 7,
                    "side": "RIGHT",
                    "subject_type": "LINE",
                    "local_id": "local-review-comment-line",
                    "local_thread_id": "local-review-thread-line"
                }),
            },
        )
        .await?;
    assert!(!line_submission.mutation_id.is_empty());

    let range_submission = engine
        .submit(
            &harness.account_id,
            SubmitPayload {
                kind: MutationKind::AddReviewComment,
                target_type: "pull_request".to_string(),
                target_id: PR_ID.to_string(),
                idempotency_key: "idem-review-range".to_string(),
                input_json: serde_json::json!({
                    "owner": OWNER,
                    "repo": REPO,
                    "repo_id": REPO_ID,
                    "pr_id": PR_ID,
                    "pr_number": 1,
                    "pull_request_id": "PR_node_review",
                    "author_id": USER_ID,
                    "head_sha": "head",
                    "body": "range comment",
                    "path": "src/lib.rs",
                    "line": 20,
                    "start_line": 18,
                    "side": "RIGHT",
                    "start_side": "RIGHT",
                    "subject_type": "LINE",
                    "local_id": "local-review-comment-range",
                    "local_thread_id": "local-review-thread-range"
                }),
            },
        )
        .await?;
    assert!(!range_submission.mutation_id.is_empty());

    let reply_submission = engine
        .submit(
            &harness.account_id,
            SubmitPayload {
                kind: MutationKind::AddReviewComment,
                target_type: "pull_request".to_string(),
                target_id: PR_ID.to_string(),
                idempotency_key: "idem-review-reply".to_string(),
                input_json: serde_json::json!({
                    "owner": OWNER,
                    "repo": REPO,
                    "repo_id": REPO_ID,
                    "pr_id": PR_ID,
                    "pr_number": 1,
                    "pull_request_id": "PR_node_review",
                    "author_id": USER_ID,
                    "head_sha": "head",
                    "body": "thread reply",
                    "path": "src/lib.rs",
                    "line": 30,
                    "side": "RIGHT",
                    "thread_id": "thread-existing",
                    "in_reply_to_id": "comment-existing",
                    "local_id": "local-review-comment-reply"
                }),
            },
        )
        .await?;
    assert!(!reply_submission.mutation_id.is_empty());

    let drained = engine.drain().await?;
    let failed_snapshot: Vec<String> = sqlx::query_scalar(
        "SELECT id || ':' || status || ':' || COALESCE(last_error, '')
         FROM pending_mutations
         WHERE account_id = ?1
         ORDER BY created_at ASC",
    )
    .bind(&harness.account_id)
    .fetch_all(harness.db.pool())
    .await?;
    assert_eq!(
        drained.failed, 0,
        "expected no failed mutations, rows={failed_snapshot:?}"
    );

    let comment_snapshot = sqlx::query_scalar::<_, String>(
        "SELECT id || '|' || kind || '|' || COALESCE(thread_id,'') || '|' || COALESCE(path,'') || '|' || COALESCE(CAST(line AS TEXT),'') || '|' || COALESCE(CAST(start_line AS TEXT),'') || '|' || COALESCE(in_reply_to_id,'')
         FROM comments
         WHERE account_id = ?1 AND pr_id = ?2
         ORDER BY id ASC",
    )
    .bind(&harness.account_id)
    .bind(PR_ID)
    .fetch_all(harness.db.pool())
    .await?;
    let thread_snapshot = sqlx::query_scalar::<_, String>(
        "SELECT id || '|' || path || '|' || COALESCE(CAST(line AS TEXT),'') || '|' || COALESCE(CAST(start_line AS TEXT),'')
         FROM review_threads
         WHERE account_id = ?1 AND pr_id = ?2
         ORDER BY id ASC",
    )
    .bind(&harness.account_id)
    .bind(PR_ID)
    .fetch_all(harness.db.pool())
    .await?;

    let snapshot = format!(
        "comments={}\nthreads={}",
        comment_snapshot.join("\n"),
        thread_snapshot.join("\n")
    );
    let expected = "\
comments=comment-existing|review|thread-existing|src/lib.rs|30||
comment-reply-server|review_thread_reply|thread-existing|src/lib.rs|30||comment-existing
comment-review-line-server|review|thread-review-line-server|src/lib.rs|7||
comment-review-range-server|review|thread-review-range-server|src/lib.rs|20|18|
threads=thread-existing|src/lib.rs|30|28
thread-review-line-server|src/lib.rs|7|
thread-review-range-server|src/lib.rs|20|18";
    assert_eq!(snapshot, expected);

    let local_comment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM comments WHERE account_id = ?1 AND id LIKE 'local-review-comment-%'",
    )
    .bind(&harness.account_id)
    .fetch_one(harness.db.pool())
    .await?;
    assert_eq!(local_comment_count, 0);

    Ok(())
}

#[tokio::test]
async fn review_comment_422_rolls_back_and_marks_failed_mutation() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
            "message": "Validation Failed"
        })))
        .mount(&server)
        .await;
    let harness = support::build_harness(&server.uri(), "token", "review-comment-fail").await?;
    seed_review_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    let submitted = engine
        .submit(
            &harness.account_id,
            SubmitPayload {
                kind: MutationKind::AddReviewComment,
                target_type: "pull_request".to_string(),
                target_id: PR_ID.to_string(),
                idempotency_key: "idem-review-fail".to_string(),
                input_json: serde_json::json!({
                    "owner": OWNER,
                    "repo": REPO,
                    "repo_id": REPO_ID,
                    "pr_id": PR_ID,
                    "pr_number": 1,
                    "pull_request_id": "PR_node_review",
                    "author_id": USER_ID,
                    "head_sha": "head",
                    "body": "will fail",
                    "path": "src/lib.rs",
                    "line": 42,
                    "side": "RIGHT",
                    "subject_type": "LINE",
                    "local_id": "local-review-comment-fail",
                    "local_thread_id": "local-review-thread-fail"
                }),
            },
        )
        .await?;
    let drained = engine.drain().await?;
    assert_eq!(drained.failed, 1);

    let local_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM comments WHERE account_id = ?1 AND id = 'local-review-comment-fail'",
    )
    .bind(&harness.account_id)
    .fetch_one(harness.db.pool())
    .await?;
    assert_eq!(local_rows, 0, "optimistic local rows must rollback on 422");

    let status: String =
        sqlx::query_scalar("SELECT status FROM pending_mutations WHERE id = ?1 LIMIT 1")
            .bind(&submitted.mutation_id)
            .fetch_one(harness.db.pool())
            .await?;
    assert_eq!(status, "failed");

    Ok(())
}

async fn mount_review_comment_success(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThread"))
        .and(body_string_contains("\"line\":7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "addPullRequestReviewThread": {
                    "thread": {
                        "id": "thread-review-line-server",
                        "path": "src/lib.rs",
                        "side": "RIGHT",
                        "line": 7,
                        "startSide": null,
                        "startLine": null,
                        "isOutdated": false,
                        "isResolved": false,
                        "updatedAt": "2026-05-17T00:00:00Z",
                        "comments": {
                            "nodes": [{
                                "id": "comment-review-line-server",
                                "body": "line comment",
                                "createdAt": "2026-05-17T00:00:00Z",
                                "updatedAt": "2026-05-17T00:00:00Z",
                                "path": "src/lib.rs",
                                "side": "RIGHT",
                                "line": 7,
                                "startSide": null,
                                "startLine": null
                            }]
                        }
                    }
                }
            }
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThread"))
        .and(body_string_contains("\"line\":20"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "addPullRequestReviewThread": {
                    "thread": {
                        "id": "thread-review-range-server",
                        "path": "src/lib.rs",
                        "side": "RIGHT",
                        "line": 20,
                        "startSide": "RIGHT",
                        "startLine": 18,
                        "isOutdated": false,
                        "isResolved": false,
                        "updatedAt": "2026-05-17T00:00:00Z",
                        "comments": {
                            "nodes": [{
                                "id": "comment-review-range-server",
                                "body": "range comment",
                                "createdAt": "2026-05-17T00:00:00Z",
                                "updatedAt": "2026-05-17T00:00:00Z",
                                "path": "src/lib.rs",
                                "side": "RIGHT",
                                "line": 20,
                                "startSide": "RIGHT",
                                "startLine": 18
                            }]
                        }
                    }
                }
            }
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "addPullRequestReviewThreadReply": {
                    "comment": {
                        "id": "comment-reply-server",
                        "body": "thread reply",
                        "createdAt": "2026-05-17T00:00:00Z",
                        "updatedAt": "2026-05-17T00:00:00Z"
                    }
                }
            }
        })))
        .mount(server)
        .await;
}

async fn seed_review_graph(db: &desktop_lib::db::Db, account_id: &str) -> Result<()> {
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
        id: USER_ID.to_string(),
        account_id: account_id.to_string(),
        login: "reviewer".to_string(),
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
        title: "review seed".to_string(),
        body: "review body".to_string(),
        author_id: Some(USER_ID.to_string()),
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "head".to_string(),
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
    db.upsert_review_thread(&ReviewThreadRecord {
        id: "thread-existing".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        path: "src/lib.rs".to_string(),
        line: Some(30),
        side: Some("RIGHT".to_string()),
        start_line: Some(28),
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
    db.upsert_comment(&CommentRecord {
        id: "comment-existing".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        kind: "review".to_string(),
        author_id: USER_ID.to_string(),
        body: "existing".to_string(),
        created_at: 1,
        updated_at: 1,
        deleted_at: None,
        in_reply_to_id: None,
        review_id: None,
        thread_id: Some("thread-existing".to_string()),
        path: Some("src/lib.rs".to_string()),
        line: Some(30),
        side: Some("RIGHT".to_string()),
        start_line: None,
        start_side: None,
        original_commit_sha: None,
    })
    .await?;
    Ok(())
}

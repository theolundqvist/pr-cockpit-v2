use std::sync::Arc;

use anyhow::{Context, Result};
use desktop_lib::db::{
    CommentRecord, Db, PrLabelRecord, PullRequestRecord, RepoRecord, UserRecord,
};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::ipc_types::SubmitPayload;
use desktop_lib::mutations::projector;
use desktop_lib::mutations::{MutationKind, Patch};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

mod support;

const OWNER: &str = "octo";
const REPO: &str = "hello-world";
const PR_ID: &str = "pr-prop";
const REPO_ID: &str = "repo-prop";
const USER_ID: &str = "user-stub";

#[test]
fn prop_submit_then_rollback_restores_domain_state() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(24, [7_u8; 32]);
    let strategy = (
        prop::sample::select(vec![
            MutationKind::AddComment,
            MutationKind::EditComment,
            MutationKind::DeleteComment,
            MutationKind::AddReaction,
            MutationKind::RemoveReaction,
            MutationKind::AddLabel,
            MutationKind::RemoveLabel,
            MutationKind::SetAssignees,
        ]),
        "[a-z0-9_]{3,20}",
    );

    runner
        .run(&strategy, |(kind, suffix)| {
            let result = runtime.block_on(async {
                let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
                seed_graph(&harness.db, &harness.account_id).await?;
                prepare_kind_fixture(&harness.db, &harness.account_id, kind).await?;

                let before = snapshot_domain_tables(harness.db.pool()).await?;
                let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
                let payload = payload_for_kind(kind, &suffix);
                let submitted = engine.submit(&harness.account_id, payload).await?;
                engine.discard(&submitted.mutation_id).await?;
                let after = snapshot_domain_tables(harness.db.pool()).await?;
                anyhow::ensure!(before == after, "domain snapshot changed after rollback");
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn prop_submit_then_reconcile_matches_authoritative_upsert() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(16, [9_u8; 32]);
    let strategy = "[a-z0-9]{4,12}";

    runner
        .run(&strategy, |suffix| {
            let result = runtime.block_on(async {
                let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
                seed_graph(&harness.db, &harness.account_id).await?;
                let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

                let payload = SubmitPayload {
                    kind: MutationKind::AddComment,
                    target_type: "pull_request".to_string(),
                    target_id: PR_ID.to_string(),
                    idempotency_key: format!("idem-reconcile-{suffix}"),
                    input_json: serde_json::json!({
                        "local_id": format!("local-comment-{suffix}"),
                        "pr_id": PR_ID,
                        "author_id": USER_ID,
                        "body": format!("body-{suffix}"),
                        "server_body": format!("server-{suffix}"),
                    }),
                };

                let submitted = engine.submit(&harness.account_id, payload).await?;
                let summary = engine.drain().await?;
                anyhow::ensure!(summary.reconciled >= 1, "expected a reconciled mutation");

                let comment_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM comments WHERE id = ?1",
                )
                .bind(format!("srv-local-comment-{suffix}"))
                .fetch_one(harness.db.pool())
                .await?;
                anyhow::ensure!(comment_count == 1, "server authoritative comment missing");

                let local_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM comments WHERE id = ?1",
                )
                .bind(format!("local-comment-{suffix}"))
                .fetch_one(harness.db.pool())
                .await?;
                anyhow::ensure!(local_count == 0, "local temporary comment id still present");

                let mapping_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM id_mappings WHERE account_id = ?1 AND kind = 'comment' AND local_id = ?2 AND server_id = ?3",
                )
                .bind(&harness.account_id)
                .bind(format!("local-comment-{suffix}"))
                .bind(format!("srv-local-comment-{suffix}"))
                .fetch_one(harness.db.pool())
                .await?;
                anyhow::ensure!(mapping_count == 1, "expected id mapping to be written exactly once");

                let status: String = sqlx::query_scalar(
                    "SELECT status FROM pending_mutations WHERE id = ?1",
                )
                .bind(submitted.mutation_id)
                .fetch_one(harness.db.pool())
                .await?;
                anyhow::ensure!(status == "applied", "mutation should finish applied");

                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn prop_interleaved_runtime_transitions_converge() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(20, [11_u8; 32]);
    let strategy = (
        prop::collection::vec(
            prop_oneof![Just("ok"), Just("transient"), Just("hard_conflict")],
            1..20,
        ),
        prop::collection::vec(0_u8..=3, 1..20),
    );

    runner
        .run(&strategy, |(responses, ops)| {
            let result = runtime.block_on(async {
                let left = run_interleaving_case(&responses, &ops).await?;
                let right = run_interleaving_case(&responses, &ops).await?;
                anyhow::ensure!(left == right, "terminal state must be deterministic");
                anyhow::ensure!(
                    left.values().all(|status| matches!(
                        status.as_str(),
                        "applied" | "failed" | "discarded" | "pending"
                    )),
                    "terminal statuses must be from known set"
                );
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn prop_id_mappings_are_monotonic() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(12, [13_u8; 32]);
    let strategy = (1_u8..5_u8, "[a-z0-9]{4,8}");

    runner
        .run(&strategy, |(extra_retries, suffix)| {
            let result = runtime.block_on(async {
                let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
                seed_graph(&harness.db, &harness.account_id).await?;
                let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
                let local_id = format!("local-comment-{suffix}");
                let payload = SubmitPayload {
                    kind: MutationKind::AddComment,
                    target_type: "pull_request".to_string(),
                    target_id: PR_ID.to_string(),
                    idempotency_key: format!("idem-monotonic-{suffix}"),
                    input_json: serde_json::json!({
                        "local_id": local_id,
                        "pr_id": PR_ID,
                        "author_id": USER_ID,
                        "body": "body",
                        "transient_once": true,
                    }),
                };

                let submitted = engine.submit(&harness.account_id, payload).await?;
                let _ = engine.drain().await?;
                for _ in 0..extra_retries {
                    engine.retry(&submitted.mutation_id).await?;
                    let _ = engine.drain().await?;
                }

                let mappings = sqlx::query_as::<_, (String, String)>(
                    "SELECT local_id, server_id
                     FROM id_mappings
                     WHERE account_id = ?1 AND kind = 'comment' AND local_id = ?2",
                )
                .bind(&harness.account_id)
                .bind(format!("local-comment-{suffix}"))
                .fetch_all(harness.db.pool())
                .await?;
                anyhow::ensure!(mappings.len() == 1, "expected exactly one mapping row");
                let (local, server) = &mappings[0];
                anyhow::ensure!(
                    server == &format!("srv-{local}"),
                    "mapping must stay stable"
                );
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn prop_projector_is_involution() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(32, [17_u8; 32]);
    let strategy = "[a-z0-9 ]{1,30}";

    runner
        .run(&strategy, |body| {
            let result = runtime.block_on(async {
                let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
                sqlx::query(
                    "CREATE TABLE comments (
                        id TEXT PRIMARY KEY,
                        body TEXT NOT NULL,
                        pending_state TEXT,
                        body_server_adjusted INTEGER NOT NULL DEFAULT 0,
                        server_adjusted_at INTEGER
                     )",
                )
                .execute(&pool)
                .await?;
                sqlx::query("INSERT INTO comments(id, body) VALUES ('comment-1', 'seed')")
                    .execute(&pool)
                    .await?;

                let before = sqlx::query_scalar::<_, String>(
                    "SELECT body FROM comments WHERE id = 'comment-1'",
                )
                .fetch_one(&pool)
                .await?;

                let patch = Patch {
                    operations: vec![desktop_lib::mutations::patch::RowMutation {
                        table: "comments".to_string(),
                        pk: std::collections::BTreeMap::from([(
                            String::from("id"),
                            desktop_lib::mutations::patch::PatchValue::from("comment-1"),
                        )]),
                        before: Some(std::collections::BTreeMap::from([
                            (
                                String::from("id"),
                                desktop_lib::mutations::patch::PatchValue::from("comment-1"),
                            ),
                            (
                                String::from("body"),
                                desktop_lib::mutations::patch::PatchValue::from("seed"),
                            ),
                        ])),
                        after: Some(std::collections::BTreeMap::from([
                            (
                                String::from("id"),
                                desktop_lib::mutations::patch::PatchValue::from("comment-1"),
                            ),
                            (
                                String::from("body"),
                                desktop_lib::mutations::patch::PatchValue::from(body.clone()),
                            ),
                        ])),
                    }],
                    pending_overlay_kind: Some("full".to_string()),
                };
                let inverse = patch.inverse();

                let mut tx = pool.begin().await?;
                projector::apply_patch(
                    &mut tx,
                    &patch,
                    projector::PatchSource::OptimisticPrediction {
                        mutation_id: "m-prop".to_string(),
                    },
                )
                .await?;
                projector::apply_patch(
                    &mut tx,
                    &inverse,
                    projector::PatchSource::Rollback {
                        mutation_id: "m-prop".to_string(),
                    },
                )
                .await?;
                tx.commit().await?;

                let after = sqlx::query_scalar::<_, String>(
                    "SELECT body FROM comments WHERE id = 'comment-1'",
                )
                .fetch_one(&pool)
                .await?;
                anyhow::ensure!(before == after, "forward+inverse must preserve row");
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

async fn run_interleaving_case(
    responses: &[&str],
    ops: &[u8],
) -> Result<std::collections::BTreeMap<String, String>> {
    let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
    seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    for (index, response) in responses.iter().enumerate() {
        let force_error = match *response {
            "transient" => Some("transient"),
            "hard_conflict" => Some("hard_conflict"),
            _ => None,
        };

        let payload = SubmitPayload {
            kind: MutationKind::AddComment,
            target_type: "pull_request".to_string(),
            target_id: PR_ID.to_string(),
            idempotency_key: format!("idem-int-{index}"),
            input_json: serde_json::json!({
                "local_id": format!("local-int-{index}"),
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "body": format!("body-{index}"),
                "force_error": force_error,
            }),
        };
        let _ = engine.submit(&harness.account_id, payload).await?;
    }

    for op in ops {
        match op {
            0 => {
                let _ = engine.drain().await?;
            }
            1 => {
                if let Some(id) = first_mutation_id_with_status(harness.db.pool(), "failed").await?
                {
                    engine.retry(&id).await?;
                }
            }
            2 => {
                if let Some(id) =
                    first_mutation_id_with_status(harness.db.pool(), "pending").await?
                {
                    engine.discard(&id).await?;
                }
            }
            _ => {
                let _ = engine.drain().await?;
            }
        }
    }

    let _ = engine.drain().await?;

    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT idempotency_key, status
         FROM pending_mutations
         ORDER BY idempotency_key ASC",
    )
    .fetch_all(harness.db.pool())
    .await?;

    Ok(rows.into_iter().collect())
}

async fn first_mutation_id_with_status(
    pool: &sqlx::SqlitePool,
    status: &str,
) -> Result<Option<String>> {
    let mutation_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM pending_mutations WHERE status = ?1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(status)
    .fetch_optional(pool)
    .await?;
    Ok(mutation_id)
}

fn payload_for_kind(kind: MutationKind, suffix: &str) -> SubmitPayload {
    match kind {
        MutationKind::AddComment => SubmitPayload {
            kind,
            target_type: "pull_request".to_string(),
            target_id: PR_ID.to_string(),
            idempotency_key: format!("idem-add-comment-{suffix}"),
            input_json: serde_json::json!({
                "local_id": format!("local-comment-{suffix}"),
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "body": format!("body-{suffix}"),
            }),
        },
        MutationKind::EditComment => SubmitPayload {
            kind,
            target_type: "comment".to_string(),
            target_id: "comment-edit".to_string(),
            idempotency_key: format!("idem-edit-comment-{suffix}"),
            input_json: serde_json::json!({
                "comment_id": "comment-edit",
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "previous_body": "seed-edit",
                "next_body": format!("next-{suffix}"),
                "created_at": 10,
                "previous_updated_at": 10,
            }),
        },
        MutationKind::DeleteComment => SubmitPayload {
            kind,
            target_type: "comment".to_string(),
            target_id: "comment-delete".to_string(),
            idempotency_key: format!("idem-delete-comment-{suffix}"),
            input_json: serde_json::json!({
                "comment_id": "comment-delete",
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "body": "seed-delete",
                "created_at": 10,
                "previous_updated_at": 10,
            }),
        },
        MutationKind::AddReaction => SubmitPayload {
            kind,
            target_type: "comment".to_string(),
            target_id: "comment-react".to_string(),
            idempotency_key: format!("idem-add-reaction-{suffix}"),
            input_json: serde_json::json!({
                "comment_id": "comment-react",
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "body": "seed-react",
                "previous_updated_at": 1,
            }),
        },
        MutationKind::RemoveReaction => SubmitPayload {
            kind,
            target_type: "comment".to_string(),
            target_id: "comment-react".to_string(),
            idempotency_key: format!("idem-remove-reaction-{suffix}"),
            input_json: serde_json::json!({
                "comment_id": "comment-react",
                "pr_id": PR_ID,
                "author_id": USER_ID,
                "body": "seed-react",
                "previous_updated_at": 1,
            }),
        },
        MutationKind::AddLabel => SubmitPayload {
            kind,
            target_type: "pull_request".to_string(),
            target_id: PR_ID.to_string(),
            idempotency_key: format!("idem-add-label-{suffix}"),
            input_json: serde_json::json!({
                "pr_id": PR_ID,
                "label_name": format!("label-{suffix}"),
                "label_color": "aabbcc",
            }),
        },
        MutationKind::RemoveLabel => SubmitPayload {
            kind,
            target_type: "pull_request".to_string(),
            target_id: PR_ID.to_string(),
            idempotency_key: format!("idem-remove-label-{suffix}"),
            input_json: serde_json::json!({
                "pr_id": PR_ID,
                "label_name": "remove-me",
                "label_color": "ddeeff",
            }),
        },
        MutationKind::SetAssignees => SubmitPayload {
            kind,
            target_type: "pull_request".to_string(),
            target_id: PR_ID.to_string(),
            idempotency_key: format!("idem-assignees-{suffix}"),
            input_json: serde_json::json!({
                "pr_id": PR_ID,
                "user_id": USER_ID,
            }),
        },
        _ => unreachable!("strategy only generates covered stub kinds"),
    }
}

async fn prepare_kind_fixture(db: &Db, account_id: &str, kind: MutationKind) -> Result<()> {
    let now = 10;
    match kind {
        MutationKind::EditComment => {
            db.upsert_comment(&CommentRecord {
                id: "comment-edit".to_string(),
                account_id: account_id.to_string(),
                pr_id: PR_ID.to_string(),
                kind: "issue".to_string(),
                author_id: USER_ID.to_string(),
                body: "seed-edit".to_string(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                in_reply_to_id: None,
                review_id: None,
                thread_id: None,
                path: None,
                line: None,
                side: None,
                start_line: None,
                start_side: None,
                original_commit_sha: None,
            })
            .await?;
        }
        MutationKind::DeleteComment => {
            db.upsert_comment(&CommentRecord {
                id: "comment-delete".to_string(),
                account_id: account_id.to_string(),
                pr_id: PR_ID.to_string(),
                kind: "issue".to_string(),
                author_id: USER_ID.to_string(),
                body: "seed-delete".to_string(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                in_reply_to_id: None,
                review_id: None,
                thread_id: None,
                path: None,
                line: None,
                side: None,
                start_line: None,
                start_side: None,
                original_commit_sha: None,
            })
            .await?;
        }
        MutationKind::AddReaction | MutationKind::RemoveReaction => {
            db.upsert_comment(&CommentRecord {
                id: "comment-react".to_string(),
                account_id: account_id.to_string(),
                pr_id: PR_ID.to_string(),
                kind: "issue".to_string(),
                author_id: USER_ID.to_string(),
                body: "seed-react".to_string(),
                created_at: now,
                updated_at: 1,
                deleted_at: None,
                in_reply_to_id: None,
                review_id: None,
                thread_id: None,
                path: None,
                line: None,
                side: None,
                start_line: None,
                start_side: None,
                original_commit_sha: None,
            })
            .await?;
        }
        MutationKind::RemoveLabel => {
            db.replace_pr_labels(
                account_id,
                PR_ID,
                &[PrLabelRecord {
                    account_id: account_id.to_string(),
                    pr_id: PR_ID.to_string(),
                    label_name: "remove-me".to_string(),
                    label_color: "ddeeff".to_string(),
                    description: None,
                }],
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}

async fn seed_graph(db: &Db, account_id: &str) -> Result<()> {
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
        login: "user-stub".to_string(),
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
        title: "seed".to_string(),
        body: "seed body".to_string(),
        author_id: Some(USER_ID.to_string()),
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feat".to_string(),
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

async fn snapshot_domain_tables(pool: &sqlx::SqlitePool) -> Result<String> {
    let comments = table_dump(
        pool,
        "SELECT id || '|' || account_id || '|' || pr_id || '|' || body || '|' || updated_at
         FROM comments
         ORDER BY id",
    )
    .await?;
    let labels = table_dump(
        pool,
        "SELECT account_id || '|' || pr_id || '|' || label_name || '|' || label_color
         FROM pr_labels
         ORDER BY account_id, pr_id, label_name",
    )
    .await?;
    let assignees = table_dump(
        pool,
        "SELECT account_id || '|' || pr_id || '|' || user_id || '|' || assigned_at
         FROM pr_assignees
         ORDER BY account_id, pr_id, user_id",
    )
    .await?;
    let prs = table_dump(
        pool,
        "SELECT id || '|' || title || '|' || body || '|' || state
         FROM pull_requests
         ORDER BY id",
    )
    .await?;

    Ok(format!(
        "comments={comments}\nlabels={labels}\nassignees={assignees}\nprs={prs}"
    ))
}

async fn table_dump(pool: &sqlx::SqlitePool, sql: &str) -> Result<String> {
    let rows = sqlx::query_scalar::<_, String>(sql)
        .fetch_all(pool)
        .await
        .with_context(|| format!("running snapshot query: {sql}"))?;
    Ok(rows.join("\n"))
}

fn deterministic_runner(cases: u32, seed: [u8; 32]) -> TestRunner {
    let config = Config {
        cases,
        ..Config::default()
    };
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    TestRunner::new_with_rng(config, rng)
}

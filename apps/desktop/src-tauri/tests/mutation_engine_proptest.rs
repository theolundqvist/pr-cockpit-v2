use std::sync::Arc;

use anyhow::{Context, Result};
use desktop_lib::db::{
    CommentRecord, Db, PrAssigneeRecord, PrFileRecord, PrLabelRecord, PrReviewerRecord,
    PullRequestRecord, RepoRecord, ReviewThreadRecord, UserRecord,
};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::ipc_types::SubmitPayload;
use desktop_lib::mutations::projector;
use desktop_lib::mutations::{dispatch, MutationKind, Patch};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

mod support;

const OWNER: &str = "octo";
const REPO: &str = "hello-world";
const PR_ID: &str = "pr-prop";
const REPO_ID: &str = "repo-prop";
const USER_ID: &str = "user-stub";

#[test]
fn prop_submit_then_rollback_restores_domain_state_for_all_kinds() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(40, [7_u8; 32]);
    let strategy = (
        prop::sample::select(all_mutation_kinds()),
        "[a-z0-9_]{3,20}",
    );

    runner
        .run(&strategy, |(kind, suffix)| {
            let result = runtime.block_on(async {
                let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
                seed_graph(&harness.db, &harness.account_id).await?;

                let before = snapshot_domain_tables(harness.db.pool()).await?;
                let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
                let payload = payload_for_kind(kind, &suffix);
                let submitted = engine.submit(&harness.account_id, payload).await?;
                engine.discard(&submitted.mutation_id).await?;
                let after = snapshot_domain_tables(harness.db.pool()).await?;
                anyhow::ensure!(
                    before == after,
                    "domain snapshot changed after rollback\nbefore:\n{before}\nafter:\n{after}"
                );
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn prop_interleaved_submit_discard_is_deterministic() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let mut runner = deterministic_runner(24, [11_u8; 32]);
    let strategy = (
        prop::collection::vec(prop::sample::select(all_mutation_kinds()), 1..16),
        prop::collection::vec(0_u8..=2, 1..20),
    );

    runner
        .run(&strategy, |(kinds, ops)| {
            let result = runtime.block_on(async {
                let left = run_interleaving_case(&kinds, &ops).await?;
                let right = run_interleaving_case(&kinds, &ops).await?;
                anyhow::ensure!(left == right, "terminal state must be deterministic");
                anyhow::ensure!(
                    left.values().all(|status| {
                        matches!(
                            status.as_str(),
                            "pending" | "discarded" | "failed" | "applied"
                        )
                    }),
                    "terminal statuses must be valid pending/discarded/failed/applied"
                );
                Ok::<_, anyhow::Error>(())
            });
            result.map_err(|error| TestCaseError::fail(error.to_string()))
        })
        .expect("property should pass");
}

#[test]
fn dispatch_table_covers_full_kind_set() {
    let table = dispatch::dispatch_table();
    for kind in all_mutation_kinds() {
        assert!(
            table.contains_key(&kind),
            "dispatch table missing {}",
            kind.as_str()
        );
    }
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
    kinds: &[MutationKind],
    ops: &[u8],
) -> Result<std::collections::BTreeMap<String, String>> {
    let harness = support::build_harness("http://127.0.0.1:1", "token", "prop").await?;
    seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    for (index, kind) in kinds.iter().enumerate() {
        let payload = payload_for_kind(*kind, &format!("{index}"));
        let _ = engine.submit(&harness.account_id, payload).await?;
    }
    for op in ops {
        match op {
            0 => {
                if let Some(id) =
                    first_mutation_id_with_status(harness.db.pool(), "pending").await?
                {
                    engine.discard(&id).await?;
                }
            }
            1 => {
                let _ = engine.drain().await?;
            }
            _ => {
                let _ = engine.drain().await?;
            }
        }
    }
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
    fn merge_with_base(extra: serde_json::Value) -> serde_json::Value {
        let mut base = serde_json::Map::new();
        base.insert("owner".to_string(), serde_json::json!(OWNER));
        base.insert("repo".to_string(), serde_json::json!(REPO));
        base.insert("repo_id".to_string(), serde_json::json!(REPO_ID));
        base.insert("pr_id".to_string(), serde_json::json!(PR_ID));
        base.insert("pr_number".to_string(), serde_json::json!(1));
        base.insert(
            "pull_request_id".to_string(),
            serde_json::json!("PR_node_1"),
        );
        base.insert("author_id".to_string(), serde_json::json!(USER_ID));
        base.insert("head_sha".to_string(), serde_json::json!("head"));
        base.insert("base_sha".to_string(), serde_json::json!("base"));
        base.insert("head_ref".to_string(), serde_json::json!("feature"));
        base.insert("base_ref".to_string(), serde_json::json!("main"));
        base.insert("title".to_string(), serde_json::json!("seed title"));
        base.insert("body".to_string(), serde_json::json!("seed body"));
        base.insert("state".to_string(), serde_json::json!("open"));
        base.insert("created_at".to_string(), serde_json::json!(10));
        base.insert("previous_updated_at".to_string(), serde_json::json!(10));
        if let serde_json::Value::Object(extra_map) = extra {
            for (key, value) in extra_map {
                base.insert(key, value);
            }
        }
        serde_json::Value::Object(base)
    }

    let input_json = match kind {
        MutationKind::AddComment => merge_with_base(serde_json::json!({
            "local_id": format!("local-comment-{suffix}"),
            "body": format!("comment-{suffix}"),
            "kind": "issue",
        })),
        MutationKind::EditComment => merge_with_base(serde_json::json!({
            "comment_id": "comment-edit",
            "previous_body": "before",
            "next_body": format!("after-{suffix}"),
            "kind": "issue",
        })),
        MutationKind::DeleteComment => merge_with_base(serde_json::json!({
            "comment_id": "comment-delete",
            "body": "delete-me",
            "kind": "issue",
        })),
        MutationKind::AddReaction => merge_with_base(serde_json::json!({
            "comment_id": "comment-react",
            "target_id": "comment-react",
            "target_type": "issue_comment",
            "content": "heart",
            "body": "seed",
            "kind": "issue",
        })),
        MutationKind::RemoveReaction => merge_with_base(serde_json::json!({
            "comment_id": "comment-react",
            "target_id": "comment-react",
            "target_type": "issue_comment",
            "reaction_id": "1",
            "content": "heart",
            "body": "seed",
            "kind": "issue",
        })),
        MutationKind::AddLabel => merge_with_base(serde_json::json!({
            "label_name": format!("label-{suffix}"),
            "label_color": "aabbcc",
        })),
        MutationKind::RemoveLabel => merge_with_base(serde_json::json!({
            "label_name": "label-remove",
            "label_color": "ddeeff",
        })),
        MutationKind::SetAssignees => merge_with_base(serde_json::json!({
            "previous_user_ids": ["user-old"],
            "next_user_ids": ["user-new"],
            "user_id": "user-new",
            "previous_assigned_at": 10,
        })),
        MutationKind::RequestReview => merge_with_base(serde_json::json!({
            "previous_reviewers": [],
            "next_reviewers": ["reviewer-1"],
            "reviewer_id": "reviewer-1",
        })),
        MutationKind::RemoveReviewRequest => merge_with_base(serde_json::json!({
            "previous_reviewers": ["reviewer-1"],
            "next_reviewers": [],
            "reviewer_id": "reviewer-1",
        })),
        MutationKind::SubmitReview => merge_with_base(serde_json::json!({
            "event": "COMMENT",
            "body": format!("review-{suffix}"),
            "local_review_id": format!("local-review-{suffix}"),
        })),
        MutationKind::ResolveThread | MutationKind::UnresolveThread => {
            merge_with_base(serde_json::json!({
                "thread_id": "thread-1",
                "path": "src/lib.rs",
                "previous_is_resolved": 0,
                "is_outdated": 0,
            }))
        }
        MutationKind::MarkFileViewed | MutationKind::UnmarkFileViewed => {
            merge_with_base(serde_json::json!({
                "path": "src/lib.rs",
                "status": "modified",
                "is_binary": 0,
                "additions": 1,
                "deletions": 1,
                "previous_viewed_by_account_id": serde_json::Value::Null,
                "previous_viewed_at_head_sha": serde_json::Value::Null,
            }))
        }
        MutationKind::UpdatePrTitle => merge_with_base(serde_json::json!({
            "previous_title": "seed title",
            "previous_body": "seed body",
            "title": format!("title-{suffix}"),
        })),
        MutationKind::UpdatePrDescription => merge_with_base(serde_json::json!({
            "previous_title": "seed title",
            "previous_body": "seed body",
            "body": format!("body-{suffix}"),
        })),
        MutationKind::SetMilestone => merge_with_base(serde_json::json!({
            "milestone_id": "milestone-1",
            "milestone_number": 1,
            "milestone_title": "M1",
            "milestone_state": "open",
        })),
        MutationKind::SetProject => merge_with_base(serde_json::json!({
            "project_id": "project-1",
            "project_title": "Roadmap",
            "project_field_id": "field-1",
            "project_field_option_id": "option-1",
        })),
        MutationKind::ConvertToDraft | MutationKind::MarkReadyForReview => {
            merge_with_base(serde_json::json!({
                "previous_draft": 0,
                "previous_title": "seed title",
                "previous_body": "seed body",
            }))
        }
        MutationKind::EnableAutoMerge | MutationKind::DisableAutoMerge => {
            merge_with_base(serde_json::json!({
                "merge_method": "SQUASH",
            }))
        }
        MutationKind::UpdateBranch => merge_with_base(serde_json::json!({
            "merge_state_status": "CLEAN",
        })),
        MutationKind::Merge => merge_with_base(serde_json::json!({
            "merge_method": "merge",
            "expected_head_sha": "head",
        })),
        MutationKind::ClosePr | MutationKind::ReopenPr => merge_with_base(serde_json::json!({
            "previous_state": "open",
        })),
    };
    SubmitPayload {
        kind,
        target_type: "pull_request".to_string(),
        target_id: PR_ID.to_string(),
        idempotency_key: format!("idem-{}-{suffix}", kind.as_str()),
        input_json,
    }
}

fn all_mutation_kinds() -> Vec<MutationKind> {
    vec![
        MutationKind::AddComment,
        MutationKind::EditComment,
        MutationKind::DeleteComment,
        MutationKind::AddReaction,
        MutationKind::RemoveReaction,
        MutationKind::AddLabel,
        MutationKind::RemoveLabel,
        MutationKind::SetAssignees,
        MutationKind::RequestReview,
        MutationKind::RemoveReviewRequest,
        MutationKind::SubmitReview,
        MutationKind::ResolveThread,
        MutationKind::UnresolveThread,
        MutationKind::MarkFileViewed,
        MutationKind::UnmarkFileViewed,
        MutationKind::UpdatePrTitle,
        MutationKind::UpdatePrDescription,
        MutationKind::SetMilestone,
        MutationKind::SetProject,
        MutationKind::ConvertToDraft,
        MutationKind::MarkReadyForReview,
        MutationKind::EnableAutoMerge,
        MutationKind::DisableAutoMerge,
        MutationKind::UpdateBranch,
        MutationKind::Merge,
        MutationKind::ClosePr,
        MutationKind::ReopenPr,
    ]
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

    for login in ["user-old", "user-new", "reviewer-1"] {
        db.upsert_user(&UserRecord {
            id: login.to_string(),
            account_id: account_id.to_string(),
            login: login.to_string(),
            display_name: None,
            avatar_url: None,
            html_url: None,
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    }

    db.upsert_pull_request(&PullRequestRecord {
        id: PR_ID.to_string(),
        account_id: account_id.to_string(),
        repo_id: REPO_ID.to_string(),
        number: 1,
        state: "open".to_string(),
        draft: false,
        title: "seed title".to_string(),
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
        created_at: 10,
        updated_at: 10,
        closed_at: None,
        merged_at: None,
    })
    .await?;

    db.upsert_comment(&CommentRecord {
        id: "comment-edit".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        kind: "issue".to_string(),
        author_id: USER_ID.to_string(),
        body: "before".to_string(),
        created_at: 10,
        updated_at: 10,
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

    db.upsert_comment(&CommentRecord {
        id: "comment-delete".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        kind: "issue".to_string(),
        author_id: USER_ID.to_string(),
        body: "delete-me".to_string(),
        created_at: 10,
        updated_at: 10,
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

    db.upsert_comment(&CommentRecord {
        id: "comment-react".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        kind: "issue".to_string(),
        author_id: USER_ID.to_string(),
        body: "seed".to_string(),
        created_at: 10,
        updated_at: 10,
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

    db.upsert_review_thread(&ReviewThreadRecord {
        id: "thread-1".to_string(),
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        path: "src/lib.rs".to_string(),
        line: Some(1),
        side: Some("RIGHT".to_string()),
        start_line: Some(1),
        start_side: Some("RIGHT".to_string()),
        original_commit_sha: None,
        original_path: None,
        original_position: None,
        original_line: None,
        is_outdated: false,
        is_resolved: false,
        resolved_by_id: None,
        created_at: 10,
        updated_at: 10,
    })
    .await?;

    db.replace_pr_labels(
        account_id,
        PR_ID,
        &[PrLabelRecord {
            account_id: account_id.to_string(),
            pr_id: PR_ID.to_string(),
            label_name: "label-remove".to_string(),
            label_color: "ddeeff".to_string(),
            description: None,
        }],
    )
    .await?;

    db.replace_pr_assignees(
        account_id,
        PR_ID,
        &[PrAssigneeRecord {
            account_id: account_id.to_string(),
            pr_id: PR_ID.to_string(),
            user_id: "user-old".to_string(),
            assigned_at: 10,
        }],
    )
    .await?;

    db.replace_pr_reviewers(
        account_id,
        PR_ID,
        &[PrReviewerRecord {
            account_id: account_id.to_string(),
            pr_id: PR_ID.to_string(),
            user_id: "reviewer-1".to_string(),
            reviewer_type: "user".to_string(),
            reviewer_state: "requested".to_string(),
            requested_at: 10,
        }],
    )
    .await?;

    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: PR_ID.to_string(),
        head_sha: "head".to_string(),
        path: "src/lib.rs".to_string(),
        old_path: None,
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        is_binary: false,
        patch_blob_sha: None,
        viewed_by_account_id: None,
        viewed_at_head_sha: None,
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

use std::sync::Arc;

use anyhow::Result;
use desktop_lib::db::{CheckRunRecord, CheckSuiteRecord, PullRequestRecord, RepoRecord};
use desktop_lib::sync::check_annotations::{sync_check_annotations, SyncCheckAnnotationsRequest};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn syncs_paginated_annotations_into_primary_and_aux_tables() -> Result<()> {
    let server = MockServer::start().await;
    let harness = support::build_harness(&server.uri(), "token", "check-annotations").await?;
    seed_check_graph(&harness.db, &harness.account_id).await?;

    let page_1 = annotation_page(1, 100);
    let page_2 = annotation_page(101, 100);
    let page_3 = annotation_page(201, 2);

    for (page, payload) in [(1, page_1), (2, page_2), (3, page_3)] {
        Mock::given(method("GET"))
            .and(path("/repos/octo/hello-world/check-runs/9001/annotations"))
            .and(query_param("per_page", "100"))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .mount(&server)
            .await;
    }

    let records = sync_check_annotations(
        Arc::clone(&harness.db),
        &harness.github,
        &harness.budgeter,
        &SyncCheckAnnotationsRequest {
            account_id: harness.account_id.clone(),
            owner: "octo".to_string(),
            repo: "hello-world".to_string(),
            pr_id: "pr-checks".to_string(),
            check_run_id: "run-checks".to_string(),
            check_run_rest_id: 9001,
        },
    )
    .await?;

    assert_eq!(records.len(), 202);

    let annotation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM check_annotations WHERE check_run_id = ?1")
            .bind("run-checks")
            .fetch_one(harness.db.pool())
            .await?;
    let aux_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM check_annotation_aux WHERE check_run_id = ?1")
            .bind("run-checks")
            .fetch_one(harness.db.pool())
            .await?;
    let non_right_anchors: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM check_annotation_aux WHERE check_run_id = ?1 AND anchor_side != 'RIGHT'",
    )
    .bind("run-checks")
    .fetch_one(harness.db.pool())
    .await?;

    assert_eq!(annotation_count, 202);
    assert_eq!(aux_count, 202);
    assert_eq!(non_right_anchors, 0);
    Ok(())
}

fn annotation_page(start: i64, count: i64) -> Vec<serde_json::Value> {
    (start..start + count)
        .map(|id| {
            serde_json::json!({
                "id": id,
                "path": "src/foo.ts",
                "start_line": id,
                "end_line": id,
                "start_column": 1,
                "end_column": 10,
                "annotation_level": "warning",
                "title": format!("Annotation {id}"),
                "message": format!("Message {id}"),
                "raw_details": format!("Details {id}")
            })
        })
        .collect()
}

async fn seed_check_graph(db: &desktop_lib::db::Db, account_id: &str) -> Result<()> {
    db.upsert_repo(&RepoRecord {
        id: "repo-checks".to_string(),
        account_id: account_id.to_string(),
        owner: "octo".to_string(),
        name: "hello-world".to_string(),
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
    db.upsert_pull_request(&PullRequestRecord {
        id: "pr-checks".to_string(),
        account_id: account_id.to_string(),
        repo_id: "repo-checks".to_string(),
        number: 1,
        state: "open".to_string(),
        draft: false,
        title: "Checks".to_string(),
        body: String::new(),
        author_id: None,
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feature".to_string(),
        head_sha: "head".to_string(),
        head_repo_id: Some("repo-checks".to_string()),
        mergeable_state: None,
        merge_state_status: None,
        merge_commit_allowed: None,
        squash_merge_allowed: None,
        rebase_merge_allowed: None,
        delete_branch_on_merge_default: None,
        viewer_can_merge: None,
        viewer_can_enable_auto_merge: None,
        viewer_can_disable_auto_merge: None,
        viewer_can_update_branch: None,
        viewer_can_delete_head_ref: None,
        auto_merge_enabled: None,
        auto_merge_method: None,
        auto_merge_commit_headline: None,
        auto_merge_commit_body: None,
        auto_merge_enabled_by_login: None,
        auto_merge_enabled_at: None,
        merge_queue_entry_id: None,
        merge_queue_entry_position: None,
        merge_queue_entry_state: None,
        merge_queue_entry_estimated_ms: None,
        branch_protection_summary_json: None,
        repo_has_merge_queue: None,
        head_ref_state: None,
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
    db.upsert_check_suite(&CheckSuiteRecord {
        id: "suite-checks".to_string(),
        account_id: account_id.to_string(),
        pr_id: "pr-checks".to_string(),
        head_sha: "head".to_string(),
        app_name: "ci-linux".to_string(),
        status: "completed".to_string(),
        conclusion: Some("failure".to_string()),
        details_url: Some("https://github.com/octo/hello-world/actions/runs/1".to_string()),
        created_at: 1,
        updated_at: 2,
    })
    .await?;
    db.upsert_check_run(&CheckRunRecord {
        id: "run-checks".to_string(),
        rest_id: Some(9001),
        account_id: account_id.to_string(),
        check_suite_id: "suite-checks".to_string(),
        pr_id: "pr-checks".to_string(),
        name: "lint".to_string(),
        status: "completed".to_string(),
        conclusion: Some("failure".to_string()),
        details_url: Some(
            "https://github.com/octo/hello-world/actions/runs/1/jobs/9001".to_string(),
        ),
        output_title: None,
        output_summary: None,
        started_at: Some(1),
        completed_at: Some(2),
        created_at: 1,
        updated_at: 2,
    })
    .await?;
    Ok(())
}

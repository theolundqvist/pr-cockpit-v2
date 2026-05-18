use std::sync::Arc;

use anyhow::Result;
use desktop_lib::mutations::MutationEngine;
use desktop_lib::stacks::ops::{
    get_stack_op, merge_stack, CommandGitOps, MergeMethod, StackOperationStatus,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "stack_test_support.rs"]
mod stack_test_support;
#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn stack_merge_sequential_runs_merge_and_retarget_order() -> Result<()> {
    let server = MockServer::start().await;
    for (number, sha) in [
        (1_i64, "merge-sha-1"),
        (2_i64, "merge-sha-2"),
        (3_i64, "merge-sha-3"),
    ] {
        Mock::given(method("PUT"))
            .and(path(format!(
                "/repos/octo/hello-world/pulls/{number}/merge"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "merged": true,
                "sha": sha
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "updatePullRequest": {
                    "pullRequest": { "id": "ignored", "baseRefName": "main" }
                }
            }
        })))
        .mount(&server)
        .await;

    let harness = support::build_harness(&server.uri(), "token", "stack-merge").await?;
    let repo_id = "repo-stack";
    harness
        .db
        .upsert_repo(&desktop_lib::db::RepoRecord {
            id: repo_id.to_string(),
            account_id: harness.account_id.clone(),
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

    let prs = vec![
        stack_test_support::pr_row(
            &harness.account_id,
            repo_id,
            "pr_root",
            1,
            "main",
            "feature/root",
        ),
        stack_test_support::pr_row(
            &harness.account_id,
            repo_id,
            "pr_mid",
            2,
            "feature/root",
            "feature/mid",
        ),
        stack_test_support::pr_row(
            &harness.account_id,
            repo_id,
            "pr_top",
            3,
            "feature/mid",
            "feature/top",
        ),
    ];
    stack_test_support::seed_prs(harness.db.as_ref(), &prs).await?;
    let stack_id =
        stack_test_support::seed_stack(harness.db.as_ref(), &harness.account_id, repo_id, &prs)
            .await?;

    let mutations = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
    let git = CommandGitOps;
    let op_id = merge_stack(
        harness.db.as_ref(),
        &git,
        &harness.github,
        &mutations,
        &harness.account_id,
        &stack_id,
        MergeMethod::Merge,
    )
    .await?;
    let operation = get_stack_op(harness.db.as_ref(), &op_id)
        .await?
        .expect("operation row should exist");
    assert_eq!(operation.status, StackOperationStatus::Succeeded);

    let requests = server
        .received_requests()
        .await
        .expect("request capture to work");
    let mut sequence = Vec::new();
    for request in requests {
        let path = request.url.path().to_string();
        if path.ends_with("/pulls/1/merge") {
            sequence.push("merge-root".to_string());
            continue;
        }
        if path.ends_with("/pulls/2/merge") {
            sequence.push("merge-mid".to_string());
            continue;
        }
        if path.ends_with("/pulls/3/merge") {
            sequence.push("merge-top".to_string());
            continue;
        }
        if path == "/graphql" {
            let body = String::from_utf8_lossy(&request.body).to_string();
            if body.contains("updatePullRequest") && body.contains("pr_mid") {
                sequence.push("retarget-mid".to_string());
            } else if body.contains("updatePullRequest") && body.contains("pr_top") {
                sequence.push("retarget-top".to_string());
            }
        }
    }
    assert_eq!(
        sequence,
        vec![
            "merge-root",
            "retarget-mid",
            "merge-mid",
            "retarget-top",
            "merge-top"
        ]
    );
    Ok(())
}

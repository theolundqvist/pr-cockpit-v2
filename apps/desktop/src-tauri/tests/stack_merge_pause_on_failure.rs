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
async fn stack_merge_pauses_when_middle_merge_fails() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/repos/octo/hello-world/pulls/1/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "merged": true,
            "sha": "merge-sha-1"
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/repos/octo/hello-world/pulls/2/merge"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
            "message": "Branch protection blocked merge"
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/repos/octo/hello-world/pulls/3/merge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "merged": true,
            "sha": "merge-sha-3"
        })))
        .mount(&server)
        .await;
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

    let harness = support::build_harness(&server.uri(), "token", "stack-merge-fail").await?;
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
    assert_eq!(operation.status, StackOperationStatus::PausedFailure);
    assert!(operation
        .last_error
        .as_deref()
        .unwrap_or_default()
        .contains("Branch protection blocked merge"));

    let requests = server
        .received_requests()
        .await
        .expect("request capture to work");
    let merged_top = requests
        .iter()
        .any(|request| request.url.path().ends_with("/pulls/3/merge"));
    assert!(!merged_top, "top PR should not merge after middle failure");
    Ok(())
}

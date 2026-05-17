#![allow(dead_code, unused_imports)]

use std::sync::Arc;

use anyhow::Result;
use desktop_lib::db::{Db, PullRequestRecord, RepoRecord, UserRecord};
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::ipc_types::SubmitPayload;
use desktop_lib::mutations::MutationKind;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "support/mod.rs"]
mod support;
pub use support::build_harness;

pub const OWNER: &str = "octo";
pub const REPO: &str = "hello-world";
pub const PR_ID: &str = "pr-test";
pub const REPO_ID: &str = "repo-test";
pub const USER_ID: &str = "user-stub";

pub async fn run_happy(kind: MutationKind, suffix: &str) -> Result<()> {
    let server = MockServer::start().await;
    mount_success(&server, kind).await;
    let harness = support::build_harness(&server.uri(), "token", "mutation").await?;
    seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
    let payload = payload_for_kind(kind, suffix);
    let submitted = engine.submit(&harness.account_id, payload).await?;
    let summary = engine.drain().await?;
    anyhow::ensure!(
        summary.processed >= 1,
        "expected at least one processed mutation"
    );
    let (status, requires_connection_confirmation): (String, i64) = sqlx::query_as(
        "SELECT status, requires_connection_confirmation
         FROM pending_mutations
         WHERE id = ?1",
    )
    .bind(submitted.mutation_id)
    .fetch_one(harness.db.pool())
    .await?;
    if is_requires_confirmation_kind(kind) {
        anyhow::ensure!(
            status == "pending" && requires_connection_confirmation == 1,
            "mutation {} must remain pending with requires_connection_confirmation=1, got status `{status}` and flag `{requires_connection_confirmation}`",
            kind.as_str()
        );
    } else {
        anyhow::ensure!(
            status == "applied",
            "mutation {} must be applied, got status `{status}`",
            kind.as_str()
        );
    }
    Ok(())
}

pub async fn run_failure(kind: MutationKind, suffix: &str) -> Result<()> {
    let server = MockServer::start().await;
    mount_failure(&server, kind).await;
    let harness = support::build_harness(&server.uri(), "token", "mutation").await?;
    seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());
    let payload = payload_for_kind(kind, suffix);
    let submitted = engine.submit(&harness.account_id, payload).await?;
    let _ = engine.drain().await?;
    let (status, requires_connection_confirmation): (String, i64) = sqlx::query_as(
        "SELECT status, requires_connection_confirmation
         FROM pending_mutations
         WHERE id = ?1",
    )
    .bind(&submitted.mutation_id)
    .fetch_one(harness.db.pool())
    .await?;
    if is_requires_confirmation_kind(kind) {
        anyhow::ensure!(
            status == "pending" && requires_connection_confirmation == 1,
            "mutation {} must remain pending with requires_connection_confirmation=1, got status `{status}` and flag `{requires_connection_confirmation}`",
            kind.as_str()
        );
    } else {
        anyhow::ensure!(
            status == "failed",
            "mutation {} must fail on 422, got `{status}`",
            kind.as_str()
        );
    }
    Ok(())
}

fn is_requires_confirmation_kind(kind: MutationKind) -> bool {
    matches!(
        kind,
        MutationKind::Merge | MutationKind::EnableAutoMerge | MutationKind::DisableAutoMerge
    )
}

pub async fn mount_success(server: &MockServer, kind: MutationKind) {
    let graphql_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "data": {
            "addPullRequestReviewThreadReply": {
                "comment": {
                    "id": "comment-gql-1",
                    "body": "server body",
                    "createdAt": "2026-05-17T00:00:00Z",
                    "updatedAt": "2026-05-17T00:00:00Z"
                }
            },
            "addPullRequestReviewComment": {
                "comment": {
                    "id": "comment-review-1",
                    "body": "server review comment body",
                    "createdAt": "2026-05-17T00:00:00Z",
                    "updatedAt": "2026-05-17T00:00:00Z",
                    "path": "src/lib.rs",
                    "side": "RIGHT",
                    "line": 7,
                    "startSide": "RIGHT",
                    "startLine": 5,
                    "pullRequestReviewThread": {
                        "id": "thread-review-1",
                        "path": "src/lib.rs",
                        "side": "RIGHT",
                        "line": 7,
                        "startSide": "RIGHT",
                        "startLine": 5,
                        "isOutdated": false,
                        "isResolved": false,
                        "updatedAt": "2026-05-17T00:00:00Z"
                    }
                }
            },
            "addPullRequestReviewThread": {
                "thread": {
                    "id": "thread-review-1",
                    "path": "src/lib.rs",
                    "side": "RIGHT",
                    "line": 7,
                    "startSide": "RIGHT",
                    "startLine": 5,
                    "isOutdated": false,
                    "isResolved": false,
                    "updatedAt": "2026-05-17T00:00:00Z",
                    "comments": {
                        "nodes": [{
                            "id": "comment-review-1",
                            "body": "server review comment body",
                            "createdAt": "2026-05-17T00:00:00Z",
                            "updatedAt": "2026-05-17T00:00:00Z",
                            "path": "src/lib.rs",
                            "side": "RIGHT",
                            "line": 7,
                            "startSide": "RIGHT",
                            "startLine": 5
                        }]
                    }
                }
            },
            "submitPullRequestReview": {
                "pullRequestReview": {
                    "id": "review-1",
                    "state": "COMMENTED",
                    "body": "review body",
                    "createdAt": "2026-05-17T00:00:00Z",
                    "updatedAt": "2026-05-17T00:00:00Z",
                    "submittedAt": "2026-05-17T00:00:00Z",
                    "author": {"id": USER_ID},
                    "comments": {"nodes": []}
                }
            },
            "resolveReviewThread": {
                "thread": {
                    "id": "thread-1",
                    "path": "src/lib.rs",
                    "line": 1,
                    "side": "RIGHT",
                    "startLine": 1,
                    "startSide": "RIGHT",
                    "isOutdated": false,
                    "isResolved": true,
                    "updatedAt": "2026-05-17T00:00:00Z",
                    "resolvedBy": {"id": USER_ID}
                }
            },
            "unresolveReviewThread": {
                "thread": {
                    "id": "thread-1",
                    "path": "src/lib.rs",
                    "line": 1,
                    "side": "RIGHT",
                    "startLine": 1,
                    "startSide": "RIGHT",
                    "isOutdated": false,
                    "isResolved": false,
                    "updatedAt": "2026-05-17T00:00:00Z",
                    "resolvedBy": null
                }
            },
            "addProjectV2ItemById": {
                "item": {"id": "project-item-1"}
            },
            "updateProjectV2ItemFieldValue": {
                "projectV2Item": {"id": "project-item-1"}
            },
            "convertPullRequestToDraft": {
                "pullRequest": {
                    "id": "PR_node_1",
                    "title": "seed title",
                    "body": "seed body",
                    "state": "OPEN",
                    "isDraft": true,
                    "updatedAt": "2026-05-17T00:00:00Z"
                }
            },
            "markPullRequestReadyForReview": {
                "pullRequest": {
                    "id": "PR_node_1",
                    "title": "seed title",
                    "body": "seed body",
                    "state": "OPEN",
                    "isDraft": false,
                    "updatedAt": "2026-05-17T00:00:00Z"
                }
            },
            "enablePullRequestAutoMerge": {
                "pullRequest": {"id": "PR_node_1", "state": "OPEN"}
            },
            "disablePullRequestAutoMerge": {
                "pullRequest": {"id": "PR_node_1", "state": "OPEN"}
            }
        }
    }));
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(graphql_response)
        .mount(server)
        .await;

    let base_pull = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "title": "server title",
        "body": "server body",
        "state": "open",
        "draft": false,
        "updated_at": "2026-05-17T00:00:00Z"
    }));
    let labels = ResponseTemplate::new(200).set_body_json(serde_json::json!([
        {"name": "label-1", "color": "aabbcc", "description": null}
    ]));
    let assignees = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "assignees": [{"id": "user-new"}]
    }));
    let reviewers = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "requested_reviewers": [{"id": "reviewer-1"}],
        "requested_teams": []
    }));
    let comment = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": 1,
        "node_id": "comment-1",
        "body": "server comment",
        "created_at": "2026-05-17T00:00:00Z",
        "updated_at": "2026-05-17T00:00:00Z"
    }));
    let reaction = ResponseTemplate::new(200).set_body_json(serde_json::json!({ "id": 1 }));
    let merged = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "merged": true,
        "sha": "merged-sha"
    }));

    match kind {
        MutationKind::AddComment => {
            Mock::given(method("POST"))
                .and(path("/repos/octo/hello-world/issues/1/comments"))
                .respond_with(comment)
                .mount(server)
                .await;
        }
        MutationKind::EditComment => {
            Mock::given(method("PATCH"))
                .and(path("/repos/octo/hello-world/issues/comments/comment-edit"))
                .respond_with(comment)
                .mount(server)
                .await;
        }
        MutationKind::DeleteComment => {
            Mock::given(method("DELETE"))
                .and(path(
                    "/repos/octo/hello-world/issues/comments/comment-delete",
                ))
                .respond_with(ResponseTemplate::new(204))
                .mount(server)
                .await;
        }
        MutationKind::AddReaction => {
            Mock::given(method("POST"))
                .and(path(
                    "/repos/octo/hello-world/issues/comments/comment-react/reactions",
                ))
                .respond_with(reaction)
                .mount(server)
                .await;
        }
        MutationKind::RemoveReaction => {
            Mock::given(method("DELETE"))
                .and(path(
                    "/repos/octo/hello-world/issues/comments/comment-react/reactions/1",
                ))
                .respond_with(ResponseTemplate::new(204))
                .mount(server)
                .await;
        }
        MutationKind::AddLabel => {
            Mock::given(method("POST"))
                .and(path("/repos/octo/hello-world/issues/1/labels"))
                .respond_with(labels)
                .mount(server)
                .await;
        }
        MutationKind::RemoveLabel => {
            Mock::given(method("DELETE"))
                .and(path("/repos/octo/hello-world/issues/1/labels/label-remove"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
                .mount(server)
                .await;
        }
        MutationKind::SetAssignees => {
            for http_method in ["POST", "DELETE", "GET"] {
                Mock::given(method(http_method))
                    .and(path("/repos/octo/hello-world/issues/1/assignees"))
                    .respond_with(assignees.clone())
                    .mount(server)
                    .await;
            }
        }
        MutationKind::RequestReview => {
            Mock::given(method("POST"))
                .and(path("/repos/octo/hello-world/pulls/1/requested_reviewers"))
                .respond_with(reviewers)
                .mount(server)
                .await;
        }
        MutationKind::RemoveReviewRequest => {
            Mock::given(method("DELETE"))
                .and(path("/repos/octo/hello-world/pulls/1/requested_reviewers"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "requested_reviewers": [],
                    "requested_teams": []
                })))
                .mount(server)
                .await;
        }
        MutationKind::SubmitReview
        | MutationKind::AddReviewComment
        | MutationKind::ResolveThread
        | MutationKind::UnresolveThread
        | MutationKind::SetProject
        | MutationKind::ConvertToDraft
        | MutationKind::MarkReadyForReview
        | MutationKind::EnableAutoMerge
        | MutationKind::DisableAutoMerge => {}
        MutationKind::MarkFileViewed => {
            Mock::given(method("PUT"))
                .and(path(
                    "/repos/octo/hello-world/pulls/1/files/src%2Flib.rs/viewed",
                ))
                .respond_with(ResponseTemplate::new(204))
                .mount(server)
                .await;
        }
        MutationKind::UnmarkFileViewed => {
            Mock::given(method("DELETE"))
                .and(path(
                    "/repos/octo/hello-world/pulls/1/files/src%2Flib.rs/viewed",
                ))
                .respond_with(ResponseTemplate::new(204))
                .mount(server)
                .await;
        }
        MutationKind::UpdatePrTitle
        | MutationKind::UpdatePrDescription
        | MutationKind::ClosePr
        | MutationKind::ReopenPr => {
            Mock::given(method("PATCH"))
                .and(path("/repos/octo/hello-world/pulls/1"))
                .respond_with(base_pull)
                .mount(server)
                .await;
        }
        MutationKind::SetMilestone => {
            Mock::given(method("PATCH"))
                .and(path("/repos/octo/hello-world/issues/1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
                .mount(server)
                .await;
        }
        MutationKind::UpdateBranch => {
            Mock::given(method("PUT"))
                .and(path("/repos/octo/hello-world/pulls/1/update-branch"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
                .mount(server)
                .await;
        }
        MutationKind::Merge => {
            Mock::given(method("PUT"))
                .and(path("/repos/octo/hello-world/pulls/1/merge"))
                .respond_with(merged)
                .mount(server)
                .await;
        }
    }
}

pub async fn mount_failure(server: &MockServer, kind: MutationKind) {
    let conflict = ResponseTemplate::new(422).set_body_json(serde_json::json!({
        "message": "Conflict",
        "errors": [{"code": "conflict"}]
    }));
    let endpoint = match kind {
        MutationKind::AddComment => "/repos/octo/hello-world/issues/1/comments",
        MutationKind::EditComment => "/repos/octo/hello-world/issues/comments/comment-edit",
        MutationKind::DeleteComment => "/repos/octo/hello-world/issues/comments/comment-delete",
        MutationKind::AddReaction => {
            "/repos/octo/hello-world/issues/comments/comment-react/reactions"
        }
        MutationKind::RemoveReaction => {
            "/repos/octo/hello-world/issues/comments/comment-react/reactions/1"
        }
        MutationKind::AddLabel => "/repos/octo/hello-world/issues/1/labels",
        MutationKind::RemoveLabel => "/repos/octo/hello-world/issues/1/labels/label-remove",
        MutationKind::SetAssignees => "/repos/octo/hello-world/issues/1/assignees",
        MutationKind::RequestReview | MutationKind::RemoveReviewRequest => {
            "/repos/octo/hello-world/pulls/1/requested_reviewers"
        }
        MutationKind::SubmitReview
        | MutationKind::AddReviewComment
        | MutationKind::ResolveThread
        | MutationKind::UnresolveThread
        | MutationKind::SetProject
        | MutationKind::ConvertToDraft
        | MutationKind::MarkReadyForReview
        | MutationKind::EnableAutoMerge
        | MutationKind::DisableAutoMerge => "/graphql",
        MutationKind::MarkFileViewed | MutationKind::UnmarkFileViewed => {
            "/repos/octo/hello-world/pulls/1/files/src%2Flib.rs/viewed"
        }
        MutationKind::UpdatePrTitle
        | MutationKind::UpdatePrDescription
        | MutationKind::ClosePr
        | MutationKind::ReopenPr => "/repos/octo/hello-world/pulls/1",
        MutationKind::SetMilestone => "/repos/octo/hello-world/issues/1",
        MutationKind::UpdateBranch => "/repos/octo/hello-world/pulls/1/update-branch",
        MutationKind::Merge => "/repos/octo/hello-world/pulls/1/merge",
    };
    Mock::given(path(endpoint))
        .respond_with(conflict)
        .mount(server)
        .await;
}

pub fn payload_for_kind(kind: MutationKind, suffix: &str) -> SubmitPayload {
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
    let extra = match kind {
        MutationKind::AddComment => serde_json::json!({
            "local_id": format!("local-comment-{suffix}"),
            "body": format!("comment-{suffix}"),
            "kind": "issue",
        }),
        MutationKind::AddReviewComment => serde_json::json!({
            "local_id": format!("local-review-comment-{suffix}"),
            "local_thread_id": format!("local-review-thread-{suffix}"),
            "body": format!("review-comment-{suffix}"),
            "path": "src/lib.rs",
            "line": 7,
            "side": "RIGHT",
            "start_line": 5,
            "start_side": "RIGHT",
            "subject_type": "LINE",
            "review_id": "review-existing-1",
        }),
        MutationKind::EditComment => serde_json::json!({
            "comment_id": "comment-edit",
            "previous_body": "before",
            "next_body": format!("after-{suffix}"),
            "kind": "issue",
        }),
        MutationKind::DeleteComment => serde_json::json!({
            "comment_id": "comment-delete",
            "body": "delete-me",
            "kind": "issue",
        }),
        MutationKind::AddReaction => serde_json::json!({
            "comment_id": "comment-react",
            "target_id": "comment-react",
            "target_type": "issue_comment",
            "content": "heart",
            "body": "seed",
            "kind": "issue",
        }),
        MutationKind::RemoveReaction => serde_json::json!({
            "comment_id": "comment-react",
            "target_id": "comment-react",
            "target_type": "issue_comment",
            "reaction_id": "1",
            "content": "heart",
            "body": "seed",
            "kind": "issue",
        }),
        MutationKind::AddLabel => serde_json::json!({
            "label_name": format!("label-{suffix}"),
            "label_color": "aabbcc",
        }),
        MutationKind::RemoveLabel => serde_json::json!({
            "label_name": "label-remove",
            "label_color": "ddeeff",
        }),
        MutationKind::SetAssignees => serde_json::json!({
            "previous_user_ids": ["user-old"],
            "next_user_ids": ["user-new"],
            "previous_logins": ["user-old"],
            "next_logins": ["user-new"],
            "user_id": "user-new",
            "user_login": "user-new",
            "previous_assigned_at": 10,
        }),
        MutationKind::RequestReview => serde_json::json!({
            "previous_reviewers": [],
            "next_reviewers": ["reviewer-1"],
            "reviewer_id": "reviewer-1",
            "reviewer_logins": ["reviewer-1"],
        }),
        MutationKind::RemoveReviewRequest => serde_json::json!({
            "previous_reviewers": ["reviewer-1"],
            "next_reviewers": [],
            "reviewer_id": "reviewer-1",
            "reviewer_logins": ["reviewer-1"],
        }),
        MutationKind::SubmitReview => serde_json::json!({
            "event": "COMMENT",
            "body": format!("review-{suffix}"),
            "local_review_id": format!("local-review-{suffix}"),
        }),
        MutationKind::ResolveThread | MutationKind::UnresolveThread => serde_json::json!({
            "thread_id": "thread-1",
            "path": "src/lib.rs",
            "previous_is_resolved": 0,
            "is_outdated": 0,
        }),
        MutationKind::MarkFileViewed | MutationKind::UnmarkFileViewed => serde_json::json!({
            "path": "src/lib.rs",
            "status": "modified",
            "is_binary": 0,
            "additions": 1,
            "deletions": 1,
        }),
        MutationKind::UpdatePrTitle => serde_json::json!({
            "previous_title": "before title",
            "title": format!("title-{suffix}"),
        }),
        MutationKind::UpdatePrDescription => serde_json::json!({
            "previous_body": "before body",
            "body": format!("body-{suffix}"),
        }),
        MutationKind::SetMilestone => serde_json::json!({
            "milestone_id": "milestone-1",
            "milestone_number": 1,
            "milestone_title": "M1",
            "milestone_state": "open",
        }),
        MutationKind::SetProject => serde_json::json!({
            "project_id": "project-1",
            "project_title": "Roadmap",
            "project_field_id": "field-1",
            "project_field_option_id": "option-1",
        }),
        MutationKind::ConvertToDraft | MutationKind::MarkReadyForReview => serde_json::json!({
            "previous_draft": 0,
        }),
        MutationKind::EnableAutoMerge | MutationKind::DisableAutoMerge => serde_json::json!({
            "merge_method": "SQUASH",
        }),
        MutationKind::UpdateBranch => serde_json::json!({
            "merge_state_status": "CLEAN",
        }),
        MutationKind::Merge => serde_json::json!({
            "merge_method": "merge",
            "expected_head_sha": "head",
        }),
        MutationKind::ClosePr | MutationKind::ReopenPr => serde_json::json!({
            "previous_state": "open",
        }),
    };
    if let serde_json::Value::Object(extra_map) = extra {
        for (key, value) in extra_map {
            base.insert(key, value);
        }
    }
    SubmitPayload {
        kind,
        target_type: "pull_request".to_string(),
        target_id: PR_ID.to_string(),
        idempotency_key: format!("idem-{}-{suffix}", kind.as_str()),
        input_json: serde_json::Value::Object(base),
    }
}

pub async fn seed_graph(db: &Db, account_id: &str) -> Result<()> {
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
        login: USER_ID.to_string(),
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
    Ok(())
}

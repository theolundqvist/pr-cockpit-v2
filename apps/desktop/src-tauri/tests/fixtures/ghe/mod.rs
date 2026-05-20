use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

pub async fn mount_full_ghe_fixture(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(GheGraphqlResponder)
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/notifications"))
        .respond_with(
            with_rate_limits(ResponseTemplate::new(200))
                .insert_header("x-poll-interval", "60")
                .set_body_json(serde_json::json!([
                    {
                        "id": "notif-1",
                        "unread": true,
                        "reason": "review_requested",
                        "updated_at": "2026-05-18T00:00:00Z",
                        "last_read_at": null,
                        "subject": {
                            "title": "GHE seeded PR",
                            "type": "PullRequest",
                            "url": "https://ghe.local/api/v3/repos/ghe-org/ghe-repo/pulls/7"
                        },
                        "repository": {
                            "id": 1,
                            "name": "ghe-repo",
                            "full_name": "ghe-org/ghe-repo",
                            "private": true,
                            "owner": { "login": "ghe-org" }
                        },
                        "url": "https://ghe.local/api/v3/notifications/threads/1"
                    }
                ])),
        )
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghe-org/ghe-repo/pulls/7.diff"))
        .respond_with(with_rate_limits(
            ResponseTemplate::new(200).set_body_string(
                "diff --git a/src/a.ts b/src/a.ts\n@@ -1 +1 @@\n-old line\n+new line\n\
                 diff --git a/src/b.ts b/src/b.ts\n@@ -2 +2 @@\n-old b\n+new b\n",
            ),
        ))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/check-runs/9001/annotations"))
        .and(query_param("per_page", "100"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!([
                {
                    "id": 1,
                    "path": "src/a.ts",
                    "start_line": 3,
                    "end_line": 3,
                    "start_column": 1,
                    "end_column": 10,
                    "annotation_level": "warning",
                    "title": "lint",
                    "message": "unexpected any",
                    "raw_details": "prefer explicit type"
                }
            ]),
        )))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/pulls/\d+$"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "title": "server title",
                "body": "server body",
                "state": "open",
                "draft": false,
                "updated_at": "2026-05-18T00:00:00Z"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/issues/\d+/labels"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!([
                { "name": "ghe-label", "color": "aabbcc", "description": null }
            ]),
        )))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/actions/jobs/9001/logs"))
        .respond_with(with_rate_limits(ResponseTemplate::new(302)).insert_header(
            "location",
            format!("{}/user-attachments/files/log-9001.txt", server.uri()),
        ))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path("/user-attachments/files/log-9001.txt"))
        .respond_with(with_rate_limits(
            ResponseTemplate::new(200).set_body_string("line 1\nline 2\nline 3\n"),
        ))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v3/repos/.+/check-runs/9001/rerequest"))
        .respond_with(with_rate_limits(
            ResponseTemplate::new(201).set_body_json(serde_json::json!({})),
        ))
        .mount(server)
        .await;

    Mock::given(method("PUT"))
        .and(path_regex(r"/api/v3/repos/.+/pulls/.+/merge"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "merged": true,
                "sha": "merged-sha-1"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/compare/.+\.\.\..+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "commits": [{ "sha": "head-new" }]
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/api/v3/repos/.+/commits/.+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "sha": "head-new",
                "commit": {
                    "message": "Commit message",
                    "author": {
                        "name": "GHE User",
                        "date": "2026-05-18T00:00:00Z"
                    }
                },
                "files": [{
                    "filename": "src/a.ts",
                    "patch": "@@ -1 +1 @@\n-old\n+new",
                    "additions": 1,
                    "deletions": 1
                }]
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v3/repos/.+/issues/.+/comments"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "id": 1001,
                "node_id": "COMMENT_NODE_1001",
                "body": "server comment",
                "created_at": "2026-05-18T00:00:00Z",
                "updated_at": "2026-05-18T00:00:00Z"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("PATCH"))
        .and(path_regex(r"/api/v3/repos/.+/issues/comments/.+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "id": 1001,
                "node_id": "COMMENT_NODE_1001",
                "body": "edited comment",
                "created_at": "2026-05-18T00:00:00Z",
                "updated_at": "2026-05-18T00:05:00Z"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v3/repos/.+/issues/.+/labels"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!([
                { "name": "ghe-label", "color": "aabbcc", "description": null }
            ]),
        )))
        .mount(server)
        .await;

    for http_method in ["POST", "DELETE", "GET"] {
        Mock::given(method(http_method))
            .and(path_regex(r"/api/v3/repos/.+/issues/.+/assignees"))
            .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({
                    "assignees": [{ "id": "user-new" }]
                }),
            )))
            .mount(server)
            .await;
    }

    Mock::given(method("POST"))
        .and(path_regex(r"/api/v3/repos/.+/pulls/.+/requested_reviewers"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "requested_reviewers": [{ "id": "USER_NODE_2" }],
                "requested_teams": []
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("PUT"))
        .and(path_regex(r"/api/v3/repos/.+/pulls/comments/.+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "commit_sha": "headsha1"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/upload/assets/users/.+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "url": format!("{}/user-attachments/files/uploaded-image.png", server.uri()),
                "alt": "pasted-image"
            }),
        )))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path_regex(r"/upload/assets/.+"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "url": format!("{}/user-attachments/files/uploaded-image-fallback.png", server.uri()),
            "alt": "pasted-image"
        }))))
        .mount(server)
        .await;

    Mock::given(method("POST"))
        .and(path("/api/v3/markdown"))
        .respond_with(with_rate_limits(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({
                "html": "<p>ok</p>"
            }),
        )))
        .mount(server)
        .await;
}

fn with_rate_limits(template: ResponseTemplate) -> ResponseTemplate {
    template
        .insert_header("x-ratelimit-remaining", "4500")
        .insert_header("x-ratelimit-limit", "5000")
        .insert_header("x-ratelimit-reset", "1999999999")
        .insert_header("x-ratelimit-used", "500")
}

struct GheGraphqlResponder;

impl Respond for GheGraphqlResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        if body.contains("InboxRefresh") {
            return with_rate_limits(
                ResponseTemplate::new(200).set_body_json(inbox_refresh_payload()),
            );
        }
        if body.contains("PrDetail") {
            return with_rate_limits(ResponseTemplate::new(200).set_body_json(pr_detail_payload()));
        }
        with_rate_limits(ResponseTemplate::new(200).set_body_json(mutation_payload()))
    }
}

fn inbox_refresh_payload() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "nodes": [
                inbox_pr("PR_NODE_1", 7, "feature/a", "main", "headsha1", "basesha1"),
                inbox_pr("PR_NODE_2", 8, "feature/b", "feature/a", "headsha2", "headsha1"),
                inbox_pr("PR_NODE_3", 9, "feature/c", "feature/b", "headsha3", "headsha2")
            ],
            "pageInfo": {
                "hasNextPage": false,
                "endCursor": "cursor-end"
            }
        }
    })
}

fn inbox_pr(
    id: &str,
    number: i64,
    head_ref: &str,
    base_ref: &str,
    head_sha: &str,
    base_sha: &str,
) -> serde_json::Value {
    serde_json::json!({
        "__typename": "PullRequest",
        "id": id,
        "number": number,
        "title": format!("GHE seeded PR #{number}"),
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
            "id": "1",
            "owner": { "login": "ghe-org" },
            "name": "ghe-repo"
        },
        "headRefName": head_ref,
        "baseRefName": base_ref,
        "headRefOid": head_sha,
        "baseRefOid": base_sha,
        "labels": { "nodes": [] },
        "reviewRequests": { "nodes": [] },
        "commits": {
            "nodes": [
                { "commit": { "statusCheckRollup": { "state": "SUCCESS" } } }
            ]
        },
        "unreadPlaceholder": true
    })
}

fn pr_detail_payload() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "repository": {
                "id": "1",
                "owner": { "login": "ghe-org" },
                "name": "ghe-repo",
                "pullRequest": {
                    "id": "PR_NODE_1",
                    "number": 7,
                    "title": "GHE seeded PR #7",
                    "body": "Parity test PR body",
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
                    "additions": 4,
                    "deletions": 2,
                    "changedFiles": 2,
                    "comments": { "totalCount": 1 },
                    "commits": {
                        "totalCount": 1,
                        "nodes": [{
                            "commit": {
                                "oid": "headsha1",
                                "checkSuites": {
                                    "nodes": [{
                                        "id": "CHECK_SUITE_1",
                                        "databaseId": 8001,
                                        "status": "COMPLETED",
                                        "conclusion": "FAILURE",
                                        "workflowRun": {
                                            "databaseId": 7001,
                                            "url": "https://ghe.local/ghe-org/ghe-repo/actions/runs/7001"
                                        },
                                        "checkRuns": {
                                            "nodes": [{
                                                "id": "CHECK_RUN_1",
                                                "databaseId": 9001,
                                                "name": "lint",
                                                "status": "COMPLETED",
                                                "conclusion": "FAILURE",
                                                "detailsUrl": "https://ghe.local/ghe-org/ghe-repo/actions/runs/7001/jobs/9001",
                                                "startedAt": "2026-05-18T00:00:00Z",
                                                "completedAt": "2026-05-18T00:01:00Z",
                                                "output": {
                                                    "title": "Lint failure",
                                                    "summary": "Found 1 issue"
                                                }
                                            }]
                                        }
                                    }]
                                }
                            }
                        }]
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
                    "headRefName": "feature/a",
                    "headRef": { "id": "HEAD_REF_1", "name": "feature/a" },
                    "headRefOid": "headsha1",
                    "repository": {
                        "mergeCommitAllowed": true,
                        "squashMergeAllowed": true,
                        "rebaseMergeAllowed": true,
                        "deleteBranchOnMerge": false,
                        "mergeQueue": null,
                        "defaultBranchRef": { "branchProtectionRule": null }
                    },
                    "headRepository": { "id": "1" },
                    "labels": { "nodes": [] },
                    "assignees": { "nodes": [] },
                    "reviewRequests": { "nodes": [] },
                    "reviewThreads": {
                        "nodes": [{
                            "id": "thread-1",
                            "path": "src/a.ts",
                            "line": 3,
                            "side": "RIGHT",
                            "startLine": 3,
                            "startSide": "RIGHT",
                            "isOutdated": false,
                            "isResolved": false,
                            "updatedAt": "2026-05-18T00:00:00Z",
                            "comments": {
                                "nodes": [{
                                    "id": "review-comment-1",
                                    "body": "review body",
                                    "createdAt": "2026-05-18T00:00:00Z",
                                    "updatedAt": "2026-05-18T00:00:00Z",
                                    "author": {
                                        "id": "U_AUTHOR",
                                        "login": "ghe-user",
                                        "name": "GHE User",
                                        "avatarUrl": null,
                                        "url": null
                                    }
                                }]
                            }
                        }]
                    },
                    "reviews": { "nodes": [] },
                    "timelineItems": { "nodes": [] }
                }
            }
        }
    })
}

fn mutation_payload() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "resolveReviewThread": {
                "thread": {
                    "id": "thread-1",
                    "path": "src/a.ts",
                    "line": 3,
                    "side": "RIGHT",
                    "startLine": 3,
                    "startSide": "RIGHT",
                    "isOutdated": false,
                    "isResolved": true,
                    "updatedAt": "2026-05-18T00:00:00Z",
                    "resolvedBy": { "id": "U_AUTHOR" }
                }
            },
            "unresolveReviewThread": {
                "thread": {
                    "id": "thread-1",
                    "path": "src/a.ts",
                    "line": 3,
                    "side": "RIGHT",
                    "startLine": 3,
                    "startSide": "RIGHT",
                    "isOutdated": false,
                    "isResolved": false,
                    "updatedAt": "2026-05-18T00:00:00Z",
                    "resolvedBy": null
                }
            },
            "rerunCheckSuite": {
                "checkSuite": {
                    "id": "CHECK_SUITE_1",
                    "status": "REQUESTED",
                    "conclusion": null
                }
            },
            "updatePullRequest": {
                "pullRequest": {
                    "id": "PR_NODE_2",
                    "baseRefName": "main"
                }
            },
            "enqueuePullRequest": {
                "mergeQueueEntry": {
                    "id": "MQE_1",
                    "position": 1,
                    "state": "QUEUED"
                }
            },
            "applyPullRequestReviewThreadSuggestion": {
                "pullRequestReviewThread": {
                    "id": "thread-1"
                }
            }
        }
    })
}

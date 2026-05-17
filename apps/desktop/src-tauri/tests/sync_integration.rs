mod support;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use desktop_lib::api::{ApiResource, ConditionalResponse, PullDiffRequest, RateLimitSnapshot};
use desktop_lib::sync::reconcile::{reconcile_pr_detail, PrDetailData};
use desktop_lib::sync::{
    run_mergeable_backoff, tier_cadence, Clock, FocusState, Priority, RateLimitBudgeter, Tier,
    TierActions,
};
use mockito::{Matcher, Server};

#[tokio::test]
async fn tiered_scheduler_honors_focus_pause_and_foreground_bypass() -> Result<()> {
    let harness =
        support::build_harness("http://127.0.0.1:9", "ghp_test_token", "sync-tester").await?;

    assert_eq!(
        tier_cadence(Tier::Warm, FocusState::Focused, None),
        Some(Duration::from_secs(30)),
        "focused warm cadence should be 30s"
    );
    assert_eq!(
        tier_cadence(
            Tier::Warm,
            FocusState::Unfocused,
            Some(Duration::from_secs(120))
        ),
        Some(Duration::from_secs(120)),
        "recently unfocused warm cadence should drift to 2m"
    );
    for tier in [Tier::Hot, Tier::Warm, Tier::Cool, Tier::Cold] {
        assert_eq!(
            tier_cadence(tier, FocusState::Unfocused, Some(Duration::from_secs(301))),
            None,
            "all tiers should pause after 5 minutes of unfocus"
        );
    }
    assert_eq!(
        tier_cadence(Tier::Cool, FocusState::Focused, None),
        Some(Duration::from_secs(180)),
        "focused cool cadence should be 3m"
    );
    assert_eq!(
        tier_cadence(
            Tier::Cool,
            FocusState::Unfocused,
            Some(Duration::from_secs(60))
        ),
        Some(Duration::from_secs(900)),
        "background cool cadence should be 15m"
    );
    assert_eq!(
        tier_cadence(
            Tier::Cold,
            FocusState::Unfocused,
            Some(Duration::from_secs(60))
        ),
        None,
        "cold tier should be on-demand in background"
    );

    harness
        .budgeter
        .record(
            &harness.account_id,
            ApiResource::Graphql,
            RateLimitSnapshot {
                remaining: 999,
                limit_total: 5000,
                used: Some(4001),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    harness
        .budgeter
        .record(
            &harness.account_id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 499,
                limit_total: 5000,
                used: Some(4501),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    tokio::task::yield_now().await;
    assert!(
        !harness.budgeter.allow(Priority::Background).await?,
        "background lane should be throttled under budget thresholds"
    );
    assert!(
        harness.budgeter.allow(Priority::Foreground).await?,
        "foreground lane should bypass back-pressure"
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_budgeter_throttles_background_only() -> Result<()> {
    let harness =
        support::build_harness("http://127.0.0.1:9", "ghp_test_token", "budgeter").await?;
    let budgeter = RateLimitBudgeter::start(Arc::clone(&harness.db));

    budgeter
        .record(
            &harness.account_id,
            ApiResource::Graphql,
            RateLimitSnapshot {
                remaining: 999,
                limit_total: 5000,
                used: Some(4001),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    budgeter
        .record(
            &harness.account_id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 1000,
                limit_total: 5000,
                used: Some(4000),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;

    assert!(!budgeter.allow(Priority::Background).await?);
    assert!(budgeter.allow(Priority::Foreground).await?);

    budgeter
        .record(
            &harness.account_id,
            ApiResource::Graphql,
            RateLimitSnapshot {
                remaining: 1200,
                limit_total: 5000,
                used: Some(3800),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    budgeter
        .record(
            &harness.account_id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 499,
                limit_total: 5000,
                used: Some(4501),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    assert!(!budgeter.allow(Priority::Background).await?);
    assert!(budgeter.allow(Priority::Foreground).await?);

    budgeter
        .record(
            &harness.account_id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 800,
                limit_total: 5000,
                used: Some(4200),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    assert!(budgeter.allow(Priority::Background).await?);

    let graphql_bucket = harness
        .db
        .rate_limit_bucket(&harness.account_id, "graphql")
        .await?
        .expect("graphql bucket should be persisted");
    let core_bucket = harness
        .db
        .rate_limit_bucket(&harness.account_id, "core")
        .await?
        .expect("core bucket should be persisted");
    assert_eq!(graphql_bucket.remaining, 1200);
    assert_eq!(core_bucket.remaining, 800);
    Ok(())
}

#[tokio::test]
async fn etag_304_round_trip_avoids_counter_decrement_and_db_writes() -> Result<()> {
    let mut server = Server::new_async().await;
    let harness = support::build_harness(&server.url(), "ghp_etag_token", "etag-user").await?;

    let first_mock = server
        .mock("GET", "/repos/octo/repo/pulls/1")
        .with_status(200)
        .with_header("etag", "\"etag-a\"")
        .with_header("x-ratelimit-remaining", "499")
        .with_header("x-ratelimit-limit", "5000")
        .with_header("x-ratelimit-reset", "1999999999")
        .with_header("x-ratelimit-used", "4501")
        .with_body("diff --git a/a b/a\n@@ -1 +1 @@\n-old\n+new\n")
        .expect(1)
        .create_async()
        .await;
    let second_mock = server
        .mock("GET", "/repos/octo/repo/pulls/1")
        .match_header("if-none-match", Matcher::Exact("\"etag-a\"".to_string()))
        .with_status(304)
        .expect(1)
        .create_async()
        .await;

    harness
        .db
        .upsert_repo(&desktop_lib::db::RepoRecord {
            id: "R_NODE_1".to_string(),
            account_id: harness.account_id.clone(),
            owner: "octo".to_string(),
            name: "repo".to_string(),
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
    harness
        .db
        .upsert_pull_request(&desktop_lib::db::PullRequestRecord {
            id: "PR_NODE_1".to_string(),
            account_id: harness.account_id.clone(),
            repo_id: "R_NODE_1".to_string(),
            number: 1,
            state: "open".to_string(),
            draft: false,
            title: "Seed PR".to_string(),
            body: String::new(),
            author_id: None,
            base_ref: "main".to_string(),
            base_sha: "BASESHA".to_string(),
            head_ref: "feature".to_string(),
            head_sha: "HEADSHA1".to_string(),
            head_repo_id: Some("R_NODE_1".to_string()),
            mergeable_state: None,
            merge_state_status: None,
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

    let first = harness
        .github
        .fetch_pull_diff(PullDiffRequest {
            account_id: &harness.account_id,
            locator: &harness.locator,
            owner: "octo",
            repo: "repo",
            number: 1,
            pr_id: "PR_NODE_1",
            head_sha: "HEADSHA1",
        })
        .await?;
    let first_blob_sha = match first {
        ConditionalResponse::Modified { payload, metadata } => {
            let snapshot = metadata
                .rate_limit
                .expect("200 responses should include rate limit metadata");
            harness
                .db
                .update_rate_limit_bucket(&desktop_lib::db::RateLimitBucketUpdate {
                    account_id: harness.account_id.clone(),
                    resource: "core".to_string(),
                    remaining: snapshot.remaining,
                    limit_total: snapshot.limit_total,
                    reset_at: snapshot.reset_at_epoch,
                    updated_at: 1,
                })
                .await?;
            payload.patch_blob_sha
        }
        ConditionalResponse::NotModified(_) => panic!("first request must be modified"),
    };
    assert!(harness
        .db
        .pr_patch(&harness.account_id, "PR_NODE_1", "HEADSHA1")
        .await?
        .is_some());
    let blob_count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blob_refs")
        .fetch_one(harness.db.pool())
        .await?;

    let second = harness
        .github
        .fetch_pull_diff(PullDiffRequest {
            account_id: &harness.account_id,
            locator: &harness.locator,
            owner: "octo",
            repo: "repo",
            number: 1,
            pr_id: "PR_NODE_1",
            head_sha: "HEADSHA1",
        })
        .await?;
    match second {
        ConditionalResponse::NotModified(metadata) => {
            assert!(
                metadata.rate_limit.is_none(),
                "304 path must not mutate counters"
            );
        }
        ConditionalResponse::Modified { .. } => panic!("second request should be 304"),
    }
    let blob_count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM blob_refs")
        .fetch_one(harness.db.pool())
        .await?;
    assert_eq!(blob_count_before, blob_count_after);

    let stored_bucket = harness
        .db
        .rate_limit_bucket(&harness.account_id, "core")
        .await?
        .expect("core bucket row should persist after first call");
    assert_eq!(stored_bucket.remaining, 499);

    let patch = harness
        .db
        .pr_patch(&harness.account_id, "PR_NODE_1", "HEADSHA1")
        .await?
        .expect("patch should remain present");
    assert!(patch.starts_with(b"diff --git"));
    assert!(!first_blob_sha.is_empty());
    first_mock.assert_async().await;
    second_mock.assert_async().await;
    Ok(())
}

#[tokio::test]
async fn mergeable_null_recovery_uses_expected_virtual_clock_schedule() -> Result<()> {
    let clock = Arc::new(MockClock::default());
    let attempts = Arc::new(AtomicUsize::new(0));
    run_mergeable_backoff(clock.as_ref(), {
        let attempts = Arc::clone(&attempts);
        move || {
            let attempts = Arc::clone(&attempts);
            async move {
                let index = attempts.fetch_add(1, Ordering::SeqCst);
                Ok(index < 5)
            }
        }
    })
    .await?;
    assert_eq!(
        clock.durations(),
        vec![
            Duration::from_secs(2),
            Duration::from_secs(5),
            Duration::from_secs(15),
            Duration::from_secs(45),
            Duration::from_secs(120),
            Duration::from_secs(300),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn pr_detail_reconcile_upserts_expected_sqlite_rows() -> Result<()> {
    let harness =
        support::build_harness("http://127.0.0.1:9", "ghp_reconcile", "reconcile-user").await?;
    let data: PrDetailData = serde_json::from_str(
        r#"{
      "repository": {
        "id": "R_NODE_1",
        "owner": { "login": "acme" },
        "name": "rocket",
        "pullRequest": {
          "id": "PR_NODE_1",
          "number": 12,
          "title": "Improve launch sequence",
          "body": "Detailed body",
          "state": "OPEN",
          "isDraft": false,
          "mergeable": "MERGEABLE",
          "mergeStateStatus": "CLEAN",
          "createdAt": "2026-05-16T10:00:00Z",
          "updatedAt": "2026-05-17T10:00:00Z",
          "closedAt": null,
          "mergedAt": null,
          "additions": 42,
          "deletions": 7,
          "changedFiles": 3,
          "comments": { "totalCount": 1 },
          "commits": {
            "totalCount": 1,
            "nodes": [{
              "commit": {
                "oid": "HEADSHA1",
                "checkSuites": {
                  "nodes": [{
                    "id": "CS_NODE_1",
                    "app": { "name": "GitHub Actions" },
                    "status": "COMPLETED",
                    "conclusion": "SUCCESS",
                    "workflowRun": { "url": "https://ci.example/run/1" },
                    "checkRuns": {
                      "nodes": [{
                        "id": "CR_NODE_1",
                        "name": "build",
                        "status": "COMPLETED",
                        "conclusion": "SUCCESS",
                        "detailsUrl": "https://ci.example/run/1/logs",
                        "title": "build",
                        "summary": "ok",
                        "startedAt": "2026-05-17T09:00:00Z",
                        "completedAt": "2026-05-17T09:05:00Z"
                      }]
                    }
                  }]
                }
              }
            }]
          },
          "author": { "id": "U1", "login": "author", "name": "Author", "avatarUrl": null, "url": null },
          "baseRefName": "main",
          "baseRefOid": "BASESHA",
          "headRefName": "feature",
          "headRefOid": "HEADSHA1",
          "headRepository": { "id": "R_NODE_1" },
          "labels": {
            "nodes": [{ "name": "ready", "color": "0e8a16", "description": "Ready to merge" }]
          },
          "assignees": {
            "nodes": [{ "id": "U2", "login": "assignee", "name": "Assignee", "avatarUrl": null, "url": null }]
          },
          "reviewRequests": {
            "nodes": [{
              "requestedReviewer": {
                "__typename": "User",
                "id": "U3",
                "login": "reviewer",
                "name": "Reviewer",
                "avatarUrl": null,
                "url": null
              }
            }]
          },
          "reviewThreads": {
            "nodes": [{
              "id": "THREAD_1",
              "isOutdated": false,
              "isResolved": false,
              "resolvedBy": null,
              "path": "src/lib.rs",
              "line": 10,
              "side": "RIGHT",
              "startLine": 9,
              "startSide": "RIGHT",
              "originalCommit": { "oid": "BASESHA" },
              "originalStartLine": 9,
              "originalLine": 10,
              "comments": {
                "nodes": [{
                  "id": "THREAD_COMMENT_1",
                  "body": "Please adjust",
                  "createdAt": "2026-05-17T08:00:00Z",
                  "updatedAt": "2026-05-17T08:10:00Z",
                  "author": { "id": "U4", "login": "threader", "name": "Thread User", "avatarUrl": null, "url": null }
                }]
              }
            }]
          },
          "reviews": {
            "nodes": [{
              "id": "REVIEW_1",
              "state": "APPROVED",
              "body": "looks good",
              "createdAt": "2026-05-17T07:00:00Z",
              "updatedAt": "2026-05-17T07:10:00Z",
              "submittedAt": "2026-05-17T07:10:00Z",
              "commit": { "oid": "HEADSHA1" },
              "author": { "id": "U5", "login": "review-author", "name": "Review Author", "avatarUrl": null, "url": null }
            }]
          },
          "timelineItems": {
            "nodes": [
              {
                "__typename": "IssueComment",
                "id": "ISSUE_COMMENT_1",
                "body": "timeline comment",
                "createdAt": "2026-05-17T06:00:00Z",
                "updatedAt": "2026-05-17T06:05:00Z",
                "author": { "id": "U6", "login": "timeline-user", "name": "Timeline User", "avatarUrl": null, "url": null }
              },
              {
                "__typename": "PullRequestReview",
                "id": "REVIEW_1",
                "state": "APPROVED",
                "body": "looks good",
                "createdAt": "2026-05-17T07:00:00Z",
                "updatedAt": "2026-05-17T07:10:00Z",
                "submittedAt": "2026-05-17T07:10:00Z",
                "commit": { "oid": "HEADSHA1" },
                "author": { "id": "U5", "login": "review-author", "name": "Review Author", "avatarUrl": null, "url": null }
              }
            ]
          }
        }
      }
    }"#,
    )?;
    reconcile_pr_detail(Arc::clone(&harness.db), &harness.account_id, data).await?;

    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pull_requests")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM comments")
            .fetch_one(harness.db.pool())
            .await?,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM review_threads")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM reviews")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM check_suites")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM check_runs")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pr_labels")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pr_assignees")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pr_reviewers")
            .fetch_one(harness.db.pool())
            .await?,
        1
    );
    Ok(())
}

#[tokio::test]
async fn notifications_change_detector_honors_headers_and_emits_refetch_targets() -> Result<()> {
    let mut server = Server::new_async().await;
    let harness = support::build_harness(&server.url(), "ghp_notify", "notify-user").await?;

    let last_modified = "Sun, 17 May 2026 07:00:00 GMT";
    let first_mock = server
        .mock("GET", "/notifications")
        .match_query(Matcher::UrlEncoded(
            "participating".to_string(),
            "false".to_string(),
        ))
        .with_status(200)
        .with_header("Last-Modified", last_modified)
        .with_header("X-Poll-Interval", "1")
        .with_header("X-RateLimit-Remaining", "4500")
        .with_header("X-RateLimit-Limit", "5000")
        .with_header("X-RateLimit-Reset", "1999999999")
        .with_header("X-RateLimit-Used", "500")
        .with_body(serde_json::to_string(&serde_json::json!([
          {
            "id": "N1",
            "unread": true,
            "reason": "review_requested",
            "updated_at": "2026-05-17T07:00:00Z",
            "last_read_at": null,
            "subject": {
              "title": "PR needs review",
              "type": "PullRequest",
              "url": "https://api.github.com/repos/acme/rocket/pulls/12",
              "latest_comment_url": null
            },
            "repository": {
              "id": 101,
              "name": "rocket",
              "owner": { "login": "acme" },
              "html_url": "https://github.com/acme/rocket",
              "description": null,
              "private": false,
              "archived": false
            },
            "url": null
          }
        ]))?)
        .expect(1)
        .create_async()
        .await;
    let second_mock = server
        .mock("GET", "/notifications")
        .match_header(
            "if-modified-since",
            Matcher::Exact(last_modified.to_string()),
        )
        .match_query(Matcher::UrlEncoded(
            "since".to_string(),
            last_modified.to_string(),
        ))
        .match_query(Matcher::UrlEncoded(
            "participating".to_string(),
            "false".to_string(),
        ))
        .with_status(304)
        .with_header("Last-Modified", last_modified)
        .with_header("X-Poll-Interval", "1")
        .expect(1)
        .create_async()
        .await;

    let actions = desktop_lib::sync::RealActions::new(
        Arc::clone(&harness.db),
        harness.github.clone(),
        harness.account_id.clone(),
        harness.locator.clone(),
        harness.budgeter.clone(),
    );

    let first_targets = actions.run_tier(Tier::Warm, Priority::Background).await?;
    assert_eq!(first_targets.len(), 1);
    assert_eq!(first_targets[0].owner, "acme");
    assert_eq!(first_targets[0].repo, "rocket");
    assert_eq!(first_targets[0].number, 12);

    let skipped_targets = actions.run_tier(Tier::Warm, Priority::Background).await?;
    assert!(
        skipped_targets.is_empty(),
        "poll interval floor should suppress immediate poll"
    );

    tokio::time::sleep(Duration::from_secs(1)).await;
    let second_targets = actions.run_tier(Tier::Warm, Priority::Background).await?;
    assert!(
        second_targets.is_empty(),
        "304 response should only signal no-op refetch"
    );
    first_mock.assert_async().await;
    second_mock.assert_async().await;
    Ok(())
}

#[derive(Default)]
struct MockClock {
    sleeps: Mutex<Vec<Duration>>,
}

impl MockClock {
    fn durations(&self) -> Vec<Duration> {
        self.sleeps.lock().expect("lock poisoned").clone()
    }
}

impl Clock for MockClock {
    fn sleep<'a>(&'a self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.sleeps.lock().expect("lock poisoned").push(duration);
        })
    }
}

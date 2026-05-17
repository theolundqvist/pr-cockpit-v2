mod support;

use anyhow::Result;
use desktop_lib::api::{INBOX_REFRESH_QUERY, PR_DETAIL_QUERY};
use desktop_lib::sync::reconcile::{InboxRefreshData, PrDetailData};

#[tokio::test]
async fn optional_online_inbox_refresh_demo() -> Result<()> {
    let token = match std::env::var("GITHUB_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ => {
            eprintln!("skipping online_demo: GITHUB_TOKEN is not set");
            return Ok(());
        }
    };

    let harness = support::build_harness("https://api.github.com", &token, "online-demo").await?;
    let (detail, _) = harness
        .github
        .graphql::<PrDetailData>(
            &harness.locator,
            PR_DETAIL_QUERY,
            serde_json::json!({
              "owner": "theolundqvist",
              "repo": "pr-cockpit-v2",
              "number": 1,
              "timelineFirst": 1,
              "timelineAfter": serde_json::Value::Null,
              "threadFirst": 1,
              "reviewFirst": 1
            }),
        )
        .await?;

    let pr_id = detail
        .repository
        .and_then(|repository| repository.pull_request)
        .map(|pr| pr.id);
    let Some(pr_id) = pr_id else {
        eprintln!("skipping online_demo: repository has no PR #1");
        return Ok(());
    };

    let (inbox, _) = harness
        .github
        .graphql::<InboxRefreshData>(
            &harness.locator,
            INBOX_REFRESH_QUERY,
            serde_json::json!({ "ids": [pr_id] }),
        )
        .await?;
    assert!(
        inbox
            .nodes
            .iter()
            .flatten()
            .any(|node| node.typename == "PullRequest"),
        "expected at least one PullRequest node from InboxRefresh"
    );
    Ok(())
}

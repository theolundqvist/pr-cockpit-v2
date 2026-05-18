mod support;

use anyhow::Result;
use desktop_lib::sync::reconcile::{reconcile_pr_detail, PrDetailData};

fn pr_detail_data(base_sha: &str, head_sha: &str, commit_oids: &[&str]) -> Result<PrDetailData> {
    let commits = commit_oids
        .iter()
        .map(|oid| {
            serde_json::json!({
                "commit": {
                    "oid": oid,
                    "checkSuites": null
                }
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::from_value(serde_json::json!({
        "repository": {
            "id": "R_NODE_1",
            "owner": { "login": "acme" },
            "name": "rocket",
            "pullRequest": {
                "id": "PR_NODE_1",
                "number": 12,
                "title": "Push history fixture",
                "body": "Body",
                "state": "OPEN",
                "isDraft": false,
                "mergeable": "MERGEABLE",
                "mergeStateStatus": "CLEAN",
                "createdAt": "2026-05-18T00:00:00Z",
                "updatedAt": "2026-05-18T00:00:00Z",
                "closedAt": null,
                "mergedAt": null,
                "additions": 0,
                "deletions": 0,
                "changedFiles": 1,
                "comments": { "totalCount": 0 },
                "commits": {
                    "totalCount": commits.len(),
                    "nodes": commits
                },
                "author": null,
                "baseRefName": "main",
                "baseRefOid": base_sha,
                "headRefName": "feature",
                "headRefOid": head_sha,
                "headRepository": null,
                "labels": { "nodes": [] },
                "assignees": { "nodes": [] },
                "reviewRequests": { "nodes": [] },
                "reviewThreads": { "nodes": [] },
                "reviews": { "nodes": [] },
                "timelineItems": { "nodes": [] }
            }
        }
    }))?)
}

#[tokio::test]
async fn reconcile_records_push_rows_with_expected_kind_transitions() -> Result<()> {
    let harness =
        support::build_harness("http://127.0.0.1:9", "ghp_push_history", "push-history").await?;
    let account_id = harness.account_id.clone();
    let db = harness.db.clone();

    let base_one = "base1111111111111111111111111111111111111111";
    let base_two = "base2222222222222222222222222222222222222222";
    let head_one = "1111111111111111111111111111111111111111";
    let head_two = "2222222222222222222222222222222222222222";
    let head_three = "3333333333333333333333333333333333333333";
    let head_four = "4444444444444444444444444444444444444444";

    reconcile_pr_detail(
        db.clone(),
        &account_id,
        pr_detail_data(base_one, head_one, &[head_one])?,
    )
    .await?;
    reconcile_pr_detail(
        db.clone(),
        &account_id,
        pr_detail_data(base_one, head_two, &[head_one, head_two])?,
    )
    .await?;
    reconcile_pr_detail(
        db.clone(),
        &account_id,
        pr_detail_data(base_one, head_three, &[head_three])?,
    )
    .await?;
    reconcile_pr_detail(
        db.clone(),
        &account_id,
        pr_detail_data(base_two, head_four, &[head_three, head_four])?,
    )
    .await?;

    let pushes = db.list_pr_pushes("PR_NODE_1").await?;
    let kinds = pushes
        .iter()
        .map(|push| push.push_kind.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec!["initial", "fast-forward", "force-push", "merge-back"]
    );

    let force_push_pairs = db.list_pr_force_push_pairs("PR_NODE_1").await?;
    assert_eq!(force_push_pairs.len(), 1);
    assert_eq!(force_push_pairs[0].old_head_sha, head_two);
    assert_eq!(force_push_pairs[0].new_head_sha, head_three);

    Ok(())
}

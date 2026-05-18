mod support;

use anyhow::Result;
use desktop_lib::range_diff::{
    CommitPairStatus, HighlightKind, RangeDiffSource, RangeDiffSourceProvider, RepoLocator,
    RestCompareRangeDiff,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn rest_compare_provider_matches_modified_commit_pair() -> Result<()> {
    let server = MockServer::start().await;
    let harness =
        support::build_harness(&server.uri(), "ghp_range_diff", "range-diff-user").await?;

    let base_sha = "base000000000000000000000000000000000000";
    let old_head_sha = "1111111111111111111111111111111111111111";
    let new_head_sha = "2222222222222222222222222222222222222222";

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/rocket/compare/{base_sha}...{old_head_sha}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "commits": [{ "sha": old_head_sha }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!(
            "/repos/acme/rocket/compare/{base_sha}...{new_head_sha}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "commits": [{ "sha": new_head_sha }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/rocket/commits/{old_head_sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": old_head_sha,
            "commit": {
                "message": "Adjust greeting wording",
                "author": {
                    "name": "Fixture User",
                    "date": "2026-05-18T00:00:00Z"
                }
            },
            "files": [{
                "filename": "src/example.txt",
                "patch": "@@ -1 +1 @@\n-hello\n+hello old",
                "additions": 1,
                "deletions": 1
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(format!("/repos/acme/rocket/commits/{new_head_sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": new_head_sha,
            "commit": {
                "message": "Adjust greeting wording",
                "author": {
                    "name": "Fixture User",
                    "date": "2026-05-18T00:05:00Z"
                }
            },
            "files": [{
                "filename": "src/example.txt",
                "patch": "@@ -1 +1 @@\n-hello\n+hello new",
                "additions": 1,
                "deletions": 1
            }]
        })))
        .mount(&server)
        .await;

    let provider = RestCompareRangeDiff {
        github: std::sync::Arc::new(harness.github.clone()),
    };
    let range_diff = provider
        .compute(
            &harness.account_id,
            &RepoLocator {
                owner: "acme".to_string(),
                name: "rocket".to_string(),
            },
            base_sha,
            old_head_sha,
            new_head_sha,
        )
        .await?;

    assert_eq!(range_diff.mode, RangeDiffSource::RestCompare);
    let modified = range_diff
        .commit_pairs
        .iter()
        .find(|pair| pair.status == CommitPairStatus::Modified)
        .expect("expected modified pair");
    let intra = modified
        .intra_diff
        .as_ref()
        .expect("modified pair should include intra-line diff");
    let first_hunk = intra.hunks.first().expect("expected a hunk");
    assert!(
        first_hunk
            .new_lines
            .iter()
            .flat_map(|line| line.segments.iter())
            .any(|segment| segment.kind == HighlightKind::Added),
        "new side should include added highlight segments"
    );

    Ok(())
}

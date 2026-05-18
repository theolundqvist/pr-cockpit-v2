use std::sync::Arc;

use anyhow::Result;
use desktop_lib::mutations::{MutationEngine, MutationKind, OptimismLevel};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod mutations_harness;

#[tokio::test]
async fn rerun_mutations_dispatch_with_cautious_optimism() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/octo/hello-world/check-runs/9001/rerequest"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "rerunCheckSuite": {
                    "checkSuite": {
                        "id": "suite-node-1",
                        "status": "REQUESTED",
                        "conclusion": null
                    }
                }
            }
        })))
        .mount(&server)
        .await;

    let harness = mutations_harness::build_harness(&server.uri(), "token", "rerun").await?;
    mutations_harness::seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    let submitted_run = engine
        .submit(
            &harness.account_id,
            mutations_harness::payload_for_kind(MutationKind::RerunCheckRun, "run"),
        )
        .await?;
    let submitted_suite = engine
        .submit(
            &harness.account_id,
            mutations_harness::payload_for_kind(MutationKind::RerunCheckSuite, "suite"),
        )
        .await?;

    assert_eq!(submitted_run.optimism_level, OptimismLevel::Cautious);
    assert_eq!(submitted_suite.optimism_level, OptimismLevel::Cautious);

    let summary = engine.drain().await?;
    assert_eq!(summary.failed, 0);

    let statuses: Vec<String> = sqlx::query_scalar(
        "SELECT status FROM pending_mutations WHERE id IN (?1, ?2) ORDER BY created_at ASC",
    )
    .bind(submitted_run.mutation_id)
    .bind(submitted_suite.mutation_id)
    .fetch_all(harness.db.pool())
    .await?;
    assert_eq!(statuses, vec!["applied".to_string(), "applied".to_string()]);
    Ok(())
}

use std::sync::Arc;

use anyhow::Result;
use desktop_lib::mutations::engine::MutationEngine;
use desktop_lib::mutations::MutationKind;
use wiremock::MockServer;

mod mutations_harness;

#[tokio::test]
async fn cross_family_e2e_reflects_optimistic_then_reconciled_state() -> Result<()> {
    let kinds = [
        MutationKind::AddComment,
        MutationKind::AddLabel,
        MutationKind::SetAssignees,
        MutationKind::ResolveThread,
        MutationKind::UpdatePrTitle,
        MutationKind::MarkFileViewed,
        MutationKind::UpdateBranch,
        MutationKind::ClosePr,
    ];
    let server = MockServer::start().await;
    for kind in kinds {
        mutations_harness::mount_success(&server, kind).await;
    }

    let harness = mutations_harness::build_harness(&server.uri(), "token", "mutation").await?;
    mutations_harness::seed_graph(&harness.db, &harness.account_id).await?;
    let engine = MutationEngine::new(Arc::clone(&harness.db), harness.github.clone());

    for (index, kind) in kinds.into_iter().enumerate() {
        let payload = mutations_harness::payload_for_kind(kind, &format!("e2e-{index}"));
        let _ = engine.submit(&harness.account_id, payload).await?;
    }

    let optimistic_summary = harness
        .db
        .pr_detail_summary(&harness.account_id, mutations_harness::PR_ID)
        .await?
        .expect("summary row");
    assert!(
        optimistic_summary.pending_overlay.is_some(),
        "expected optimistic pending overlay before drain"
    );

    let drain_summary = engine.drain().await?;
    assert_eq!(drain_summary.applied, 8);

    let reconciled_summary = harness
        .db
        .pr_detail_summary(&harness.account_id, mutations_harness::PR_ID)
        .await?
        .expect("summary row");
    assert!(
        reconciled_summary.pending_overlay.is_none(),
        "expected pending overlay to clear after reconcile"
    );
    assert_eq!(reconciled_summary.title, "server title");
    Ok(())
}

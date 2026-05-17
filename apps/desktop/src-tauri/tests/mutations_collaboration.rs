use anyhow::Result;
use desktop_lib::mutations::MutationKind;

mod mutations_harness;

#[tokio::test]
async fn collaboration_happy_paths_round_trip() -> Result<()> {
    for (index, kind) in [
        MutationKind::AddReaction,
        MutationKind::RemoveReaction,
        MutationKind::AddLabel,
        MutationKind::RemoveLabel,
        MutationKind::SetAssignees,
        MutationKind::RequestReview,
        MutationKind::RemoveReviewRequest,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_happy(kind, &format!("collab-happy-{index}")).await?;
    }
    Ok(())
}

#[tokio::test]
async fn collaboration_failure_paths_roll_back() -> Result<()> {
    for (index, kind) in [
        MutationKind::AddReaction,
        MutationKind::RemoveReaction,
        MutationKind::AddLabel,
        MutationKind::RemoveLabel,
        MutationKind::SetAssignees,
        MutationKind::RequestReview,
        MutationKind::RemoveReviewRequest,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_failure(kind, &format!("collab-fail-{index}")).await?;
    }
    Ok(())
}

use anyhow::Result;
use desktop_lib::mutations::MutationKind;

mod mutations_harness;

#[tokio::test]
async fn pr_meta_happy_paths_round_trip() -> Result<()> {
    for (index, kind) in [
        MutationKind::MarkFileViewed,
        MutationKind::UnmarkFileViewed,
        MutationKind::UpdatePrTitle,
        MutationKind::UpdatePrDescription,
        MutationKind::SetMilestone,
        MutationKind::SetProject,
        MutationKind::ConvertToDraft,
        MutationKind::MarkReadyForReview,
        MutationKind::EnableAutoMerge,
        MutationKind::DisableAutoMerge,
        MutationKind::UpdateBranch,
        MutationKind::Merge,
        MutationKind::ClosePr,
        MutationKind::ReopenPr,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_happy(kind, &format!("meta-happy-{index}")).await?;
    }
    Ok(())
}

#[tokio::test]
async fn pr_meta_failure_paths_roll_back() -> Result<()> {
    for (index, kind) in [
        MutationKind::MarkFileViewed,
        MutationKind::UnmarkFileViewed,
        MutationKind::UpdatePrTitle,
        MutationKind::UpdatePrDescription,
        MutationKind::SetMilestone,
        MutationKind::SetProject,
        MutationKind::ConvertToDraft,
        MutationKind::MarkReadyForReview,
        MutationKind::EnableAutoMerge,
        MutationKind::DisableAutoMerge,
        MutationKind::UpdateBranch,
        MutationKind::Merge,
        MutationKind::ClosePr,
        MutationKind::ReopenPr,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_failure(kind, &format!("meta-fail-{index}")).await?;
    }
    Ok(())
}

use anyhow::Result;
use desktop_lib::mutations::MutationKind;

mod mutations_harness;

#[tokio::test]
async fn review_and_thread_happy_paths_round_trip() -> Result<()> {
    for (index, kind) in [
        MutationKind::SubmitReview,
        MutationKind::ResolveThread,
        MutationKind::UnresolveThread,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_happy(kind, &format!("review-happy-{index}")).await?;
    }
    Ok(())
}

#[tokio::test]
async fn review_and_thread_failure_paths_roll_back() -> Result<()> {
    for (index, kind) in [
        MutationKind::SubmitReview,
        MutationKind::ResolveThread,
        MutationKind::UnresolveThread,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_failure(kind, &format!("review-fail-{index}")).await?;
    }
    Ok(())
}

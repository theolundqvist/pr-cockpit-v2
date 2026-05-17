use anyhow::Result;
use desktop_lib::mutations::MutationKind;

mod mutations_harness;

#[tokio::test]
async fn comments_happy_paths_round_trip() -> Result<()> {
    for (index, kind) in [
        MutationKind::AddComment,
        MutationKind::EditComment,
        MutationKind::DeleteComment,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_happy(kind, &format!("comments-happy-{index}")).await?;
    }
    Ok(())
}

#[tokio::test]
async fn comments_failure_paths_roll_back() -> Result<()> {
    for (index, kind) in [
        MutationKind::AddComment,
        MutationKind::EditComment,
        MutationKind::DeleteComment,
    ]
    .into_iter()
    .enumerate()
    {
        mutations_harness::run_failure(kind, &format!("comments-fail-{index}")).await?;
    }
    Ok(())
}

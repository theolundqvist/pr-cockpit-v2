use anyhow::Result;

mod support;

use support::notifications::NotificationHarness;

#[tokio::test(flavor = "current_thread")]
async fn review_requested_trigger_fires() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.trigger_sync().await?;
    harness.add_review_request_for_viewer(2).await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn changes_requested_trigger_fires() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.trigger_sync().await?;
    harness
        .add_review("review_changes", "CHANGES_REQUESTED", 3)
        .await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn approved_trigger_fires() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.trigger_sync().await?;
    harness.add_review("review_approved", "APPROVED", 4).await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn mention_trigger_fires() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.trigger_sync().await?;
    harness
        .add_comment("comment_mention", "hello @fixture-user please review", 5)
        .await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn ci_fail_and_recover_triggers_fire() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.set_check_red(false, 6).await?;
    harness.trigger_sync().await?;
    harness.set_check_red(true, 7).await?;
    harness.trigger_sync().await?;
    harness.set_check_red(false, 8).await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 2);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn merge_conflict_trigger_fires() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.trigger_sync().await?;
    harness.set_mergeable_state("dirty", 9).await?;
    harness.trigger_sync().await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn mutation_failure_trigger_fires_for_non_network() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness
        .send_non_network_mutation_failure("mutation-failed-1")
        .await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

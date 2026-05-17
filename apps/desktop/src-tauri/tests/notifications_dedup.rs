use anyhow::Result;
use desktop_lib::notify::DebugNotificationInput;

mod support;

use support::notifications::NotificationHarness;

#[tokio::test(flavor = "current_thread")]
async fn duplicate_events_dispatch_once() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    let payload = DebugNotificationInput {
        kind: "mention".to_string(),
        repo_id: Some(harness.repo_id.clone()),
        repo_full_name: Some("acme/repo".to_string()),
        pr_id: Some(harness.pr_id.clone()),
        actor_id: Some(harness.other_user_id.clone()),
        server_event_id: Some("comment-dup-1".to_string()),
        title: "Dup mention".to_string(),
        body: "hello".to_string(),
    };

    for _ in 0..100 {
        harness.simulate_debug_event(payload.clone()).await?;
    }

    assert_eq!(harness.sender.sent_count(), 1);
    assert_eq!(harness.notification_event_count().await?, 1);
    Ok(())
}

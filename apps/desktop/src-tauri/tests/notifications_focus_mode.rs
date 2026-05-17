use anyhow::Result;
use desktop_lib::notify::DebugNotificationInput;

mod support;

use support::notifications::NotificationHarness;

#[tokio::test(flavor = "current_thread")]
async fn focus_mode_suppresses_all_os_notifications() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness.set_focus_mode(true).await?;

    for index in 0..3 {
        harness
            .simulate_debug_event(DebugNotificationInput {
                kind: "approved".to_string(),
                repo_id: Some(harness.repo_id.clone()),
                repo_full_name: Some("acme/repo".to_string()),
                pr_id: Some(harness.pr_id.clone()),
                actor_id: Some(harness.other_user_id.clone()),
                server_event_id: Some(format!("focus-on-{index}")),
                title: "Focus on".to_string(),
                body: "suppressed".to_string(),
            })
            .await?;
    }
    assert_eq!(harness.sender.sent_count(), 0);

    harness.set_focus_mode(false).await?;
    for index in 0..3 {
        harness
            .simulate_debug_event(DebugNotificationInput {
                kind: "approved".to_string(),
                repo_id: Some(harness.repo_id.clone()),
                repo_full_name: Some("acme/repo".to_string()),
                pr_id: Some(harness.pr_id.clone()),
                actor_id: Some(harness.other_user_id.clone()),
                server_event_id: Some(format!("focus-off-{index}")),
                title: "Focus off".to_string(),
                body: "dispatched".to_string(),
            })
            .await?;
    }
    assert_eq!(harness.sender.sent_count(), 3);
    Ok(())
}

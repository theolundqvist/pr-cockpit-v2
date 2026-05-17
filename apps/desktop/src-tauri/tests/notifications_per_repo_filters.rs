use anyhow::Result;
use desktop_lib::notify::DebugNotificationInput;

mod support;

use support::notifications::NotificationHarness;

#[tokio::test(flavor = "current_thread")]
async fn allow_list_limits_dispatch_and_deny_takes_precedence() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    harness
        .set_repo_filters(&["acme/repo".to_string()], &[])
        .await?;

    harness
        .simulate_debug_event(DebugNotificationInput {
            kind: "mention".to_string(),
            repo_id: Some(harness.repo_id.clone()),
            repo_full_name: Some("acme/repo".to_string()),
            pr_id: Some(harness.pr_id.clone()),
            actor_id: Some(harness.other_user_id.clone()),
            server_event_id: Some("repo-allow-1".to_string()),
            title: "Allowed".to_string(),
            body: "should send".to_string(),
        })
        .await?;
    harness
        .simulate_debug_event(DebugNotificationInput {
            kind: "mention".to_string(),
            repo_id: Some(harness.repo_id.clone()),
            repo_full_name: Some("other/repo".to_string()),
            pr_id: Some(harness.pr_id.clone()),
            actor_id: Some(harness.other_user_id.clone()),
            server_event_id: Some("repo-denied-1".to_string()),
            title: "Denied by allow".to_string(),
            body: "should suppress".to_string(),
        })
        .await?;
    assert_eq!(harness.sender.sent_count(), 1);

    harness
        .set_repo_filters(
            &["acme/repo".to_string(), "other/repo".to_string()],
            &["acme/repo".to_string()],
        )
        .await?;
    harness
        .simulate_debug_event(DebugNotificationInput {
            kind: "mention".to_string(),
            repo_id: Some(harness.repo_id.clone()),
            repo_full_name: Some("acme/repo".to_string()),
            pr_id: Some(harness.pr_id.clone()),
            actor_id: Some(harness.other_user_id.clone()),
            server_event_id: Some("repo-deny-precedence".to_string()),
            title: "Denied by deny".to_string(),
            body: "deny should win".to_string(),
        })
        .await?;
    assert_eq!(harness.sender.sent_count(), 1);
    Ok(())
}

use anyhow::Result;
use chrono::{Datelike, Duration, Timelike, Utc};
use desktop_lib::notify::DebugNotificationInput;

mod support;

use support::notifications::NotificationHarness;

fn hhmm(value: chrono::DateTime<Utc>) -> String {
    format!("{:02}:{:02}", value.hour(), value.minute())
}

#[tokio::test(flavor = "current_thread")]
async fn quiet_hours_suppress_os_dispatch_but_record_events() -> Result<()> {
    let harness = NotificationHarness::new().await?;
    let now = Utc::now();
    let today = u8::try_from(now.weekday().num_days_from_sunday()).unwrap_or(0);
    let inside_start = hhmm(now - Duration::minutes(30));
    let inside_end = hhmm(now + Duration::minutes(30));
    harness
        .set_quiet_hours(
            &serde_json::json!({
                "start": inside_start,
                "end": inside_end,
                "tz": "UTC",
                "days": [today]
            })
            .to_string(),
        )
        .await?;

    for index in 0..3 {
        harness
            .simulate_debug_event(DebugNotificationInput {
                kind: "mention".to_string(),
                repo_id: Some(harness.repo_id.clone()),
                repo_full_name: Some("acme/repo".to_string()),
                pr_id: Some(harness.pr_id.clone()),
                actor_id: Some(harness.other_user_id.clone()),
                server_event_id: Some(format!("quiet-inside-{index}")),
                title: "Inside quiet hours".to_string(),
                body: "suppressed".to_string(),
            })
            .await?;
    }

    assert_eq!(harness.sender.sent_count(), 0);
    assert_eq!(harness.suppressed_event_count().await?, 3);

    let outside_start = hhmm(now + Duration::hours(2));
    let outside_end = hhmm(now + Duration::hours(3));
    harness
        .set_quiet_hours(
            &serde_json::json!({
                "start": outside_start,
                "end": outside_end,
                "tz": "UTC",
                "days": [today]
            })
            .to_string(),
        )
        .await?;

    for index in 0..3 {
        harness
            .simulate_debug_event(DebugNotificationInput {
                kind: "mention".to_string(),
                repo_id: Some(harness.repo_id.clone()),
                repo_full_name: Some("acme/repo".to_string()),
                pr_id: Some(harness.pr_id.clone()),
                actor_id: Some(harness.other_user_id.clone()),
                server_event_id: Some(format!("quiet-outside-{index}")),
                title: "Outside quiet hours".to_string(),
                body: "should dispatch".to_string(),
            })
            .await?;
    }

    assert_eq!(harness.sender.sent_count(), 3);
    assert_eq!(harness.suppressed_event_count().await?, 3);
    Ok(())
}

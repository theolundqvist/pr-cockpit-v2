use anyhow::{anyhow, Result};
use desktop_lib::ipc::{
    create_saved_reply_impl, delete_saved_reply_impl, list_saved_replies_impl,
    reorder_saved_replies_impl, update_saved_reply_impl, ReorderSavedRepliesInput,
};
use wiremock::MockServer;

#[path = "support/mod.rs"]
mod support;

#[tokio::test]
async fn saved_reply_crud_reorder_and_account_isolation() -> Result<()> {
    let server = MockServer::start().await;
    let harness = support::build_harness(&server.uri(), "token-a", "saved-replies-a").await?;
    let account_a = harness.account_id.clone();
    let account_b = harness
        .db
        .upsert_auth_account("github.com", "saved-replies-b", "pat", "repo", 1)
        .await?
        .id;

    let greeting = create_saved_reply_impl(
        harness.db.as_ref(),
        account_a.clone(),
        "Greeting".to_string(),
        "Thanks for the patch!".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    let follow_up = create_saved_reply_impl(
        harness.db.as_ref(),
        account_a.clone(),
        "Follow up".to_string(),
        "Could you add tests?".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    let _other = create_saved_reply_impl(
        harness.db.as_ref(),
        account_b.clone(),
        "Only B".to_string(),
        "Account scoped".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;

    let account_a_before = list_saved_replies_impl(harness.db.as_ref(), account_a.clone())
        .await
        .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    assert_eq!(account_a_before.len(), 2);
    assert_eq!(account_a_before[0].id, greeting.id);
    assert_eq!(account_a_before[1].id, follow_up.id);

    reorder_saved_replies_impl(
        harness.db.as_ref(),
        ReorderSavedRepliesInput {
            account_id: account_a.clone(),
            ordered_ids: vec![follow_up.id, greeting.id],
        },
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;

    let reordered = list_saved_replies_impl(harness.db.as_ref(), account_a.clone())
        .await
        .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    assert_eq!(reordered[0].id, follow_up.id);
    assert_eq!(reordered[1].id, greeting.id);

    let renamed = update_saved_reply_impl(
        harness.db.as_ref(),
        follow_up.id,
        "Follow up (edited)".to_string(),
        "Could you add regression tests?".to_string(),
    )
    .await
    .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    assert_eq!(renamed.name, "Follow up (edited)");
    assert_eq!(renamed.body, "Could you add regression tests?");

    delete_saved_reply_impl(harness.db.as_ref(), greeting.id)
        .await
        .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;

    let account_a_after = list_saved_replies_impl(harness.db.as_ref(), account_a)
        .await
        .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    assert_eq!(account_a_after.len(), 1);
    assert_eq!(account_a_after[0].id, follow_up.id);

    let account_b_rows = list_saved_replies_impl(harness.db.as_ref(), account_b)
        .await
        .map_err(|error| anyhow!("{}: {}", error.code, error.message))?;
    assert_eq!(account_b_rows.len(), 1);
    assert_eq!(account_b_rows[0].name, "Only B");

    Ok(())
}

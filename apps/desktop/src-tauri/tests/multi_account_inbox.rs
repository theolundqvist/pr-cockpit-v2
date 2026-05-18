use std::collections::HashMap;

use anyhow::{anyhow, Result};
use desktop_lib::db::{AccountRecord, Db, PullRequestRecord, RepoRecord, UserRecord};
use desktop_lib::ipc::{ipc_inbox_list_impl, InboxListInput};

#[tokio::test]
async fn list_inbox_none_returns_all_accounts_sorted_with_denormalized_badges() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Db::open(temp.path()).await?;
    let now = 1_715_000_000_i64;

    let account_a = AccountRecord {
        id: "github.com:account-a".to_string(),
        host: "github.com".to_string(),
        login: "account-a".to_string(),
        token_kind: "pat".to_string(),
        scopes: "repo".to_string(),
        created_at: now,
        updated_at: now,
    };
    let account_b = AccountRecord {
        id: "github.enterprise.test:account-b".to_string(),
        host: "github.enterprise.test".to_string(),
        login: "account-b".to_string(),
        token_kind: "oauth-device".to_string(),
        scopes: "repo,notifications".to_string(),
        created_at: now,
        updated_at: now,
    };

    db.upsert_account(&account_a).await?;
    db.upsert_account(&account_b).await?;

    seed_account_prs(&db, &account_a, 1, now).await?;
    seed_account_prs(&db, &account_b, 6, now).await?;

    let rows = ipc_inbox_list_impl(
        &db,
        InboxListInput {
            account_id_filter: None,
        },
    )
    .await
    .map_err(|err| anyhow!("{err:?}"))?;

    assert_eq!(rows.len(), 10, "expected five PR rows per account");
    assert!(
        rows.windows(2)
            .all(|pair| pair[0].updated_at >= pair[1].updated_at),
        "aggregated inbox should be sorted by updated_at desc"
    );

    let by_account = rows
        .iter()
        .fold(HashMap::<&str, usize>::new(), |mut acc, row| {
            *acc.entry(row.account_id.as_str()).or_default() += 1;
            acc
        });
    assert_eq!(by_account.get(account_a.id.as_str()), Some(&5usize));
    assert_eq!(by_account.get(account_b.id.as_str()), Some(&5usize));

    for row in rows {
        match row.account_id.as_str() {
            "github.com:account-a" => {
                assert_eq!(row.account_login, "account-a");
                assert_eq!(row.account_host, "github.com");
            }
            "github.enterprise.test:account-b" => {
                assert_eq!(row.account_login, "account-b");
                assert_eq!(row.account_host, "github.enterprise.test");
            }
            other => panic!("unexpected account id in inbox row: {other}"),
        }
    }

    Ok(())
}

async fn seed_account_prs(
    db: &Db,
    account: &AccountRecord,
    start_pr_number: i64,
    now: i64,
) -> Result<()> {
    let repo = RepoRecord {
        id: format!("repo-{}", account.login),
        account_id: account.id.clone(),
        owner: "octo".to_string(),
        name: format!("repo-{}", account.login),
        default_branch: Some("main".to_string()),
        description: Some("fixture repo".to_string()),
        html_url: Some(format!(
            "https://{}/octo/repo-{}",
            account.host, account.login
        )),
        is_private: false,
        is_archived: false,
        pushed_at: Some(now),
        created_at: now,
        updated_at: now,
    };
    db.upsert_repo(&repo).await?;

    let author = UserRecord {
        id: format!("user-{}", account.login),
        account_id: account.id.clone(),
        login: account.login.clone(),
        display_name: Some(account.login.clone()),
        avatar_url: None,
        html_url: None,
        created_at: now,
        updated_at: now,
    };
    db.upsert_user(&author).await?;

    for index in 0..5_i64 {
        let number = start_pr_number + index;
        db.upsert_pull_request(&PullRequestRecord {
            id: format!("pr-{}-{number}", account.login),
            account_id: account.id.clone(),
            repo_id: repo.id.clone(),
            number,
            state: "open".to_string(),
            draft: false,
            title: format!("{} PR #{number}", account.login),
            body: "fixture body".to_string(),
            author_id: Some(author.id.clone()),
            base_ref: "main".to_string(),
            base_sha: format!("base-{number:040x}"),
            head_ref: format!("feature/{number}"),
            head_sha: format!("head-{number:040x}"),
            head_repo_id: Some(repo.id.clone()),
            mergeable_state: Some("clean".to_string()),
            merge_state_status: Some("behind".to_string()),
            merge_commit_allowed: None,
            squash_merge_allowed: None,
            rebase_merge_allowed: None,
            delete_branch_on_merge_default: None,
            viewer_can_merge: None,
            viewer_can_enable_auto_merge: None,
            viewer_can_disable_auto_merge: None,
            viewer_can_update_branch: None,
            viewer_can_delete_head_ref: None,
            auto_merge_enabled: None,
            auto_merge_method: None,
            auto_merge_commit_headline: None,
            auto_merge_commit_body: None,
            auto_merge_enabled_by_login: None,
            auto_merge_enabled_at: None,
            merge_queue_entry_id: None,
            merge_queue_entry_position: None,
            merge_queue_entry_state: None,
            merge_queue_entry_estimated_ms: None,
            branch_protection_summary_json: None,
            repo_has_merge_queue: None,
            head_ref_state: None,
            additions: 10,
            deletions: 3,
            changed_files: 2,
            comments_count: 0,
            reviews_count: 0,
            commits_count: 1,
            is_read: false,
            html_url: None,
            created_at: now - 1_000 + number,
            updated_at: now + number,
            closed_at: None,
            merged_at: None,
        })
        .await?;
    }

    Ok(())
}

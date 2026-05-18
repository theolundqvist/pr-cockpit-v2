use anyhow::Result;
use desktop_lib::db::{Db, PullRequestRecord, RepoRecord};
use desktop_lib::stacks::{
    detect_stacks, now_epoch_seconds, upsert_stack_state_for_scope, PullRequestRow,
};
use tempfile::TempDir;

fn pr(
    account_id: &str,
    repo_id: &str,
    id: &str,
    number: i64,
    base_ref: &str,
    head_ref: &str,
) -> PullRequestRow {
    PullRequestRow {
        id: id.to_string(),
        account_id: account_id.to_string(),
        repo_id: repo_id.to_string(),
        number,
        title: format!("PR {number}"),
        state: "open".to_string(),
        base_ref: base_ref.to_string(),
        base_sha: format!("base-{id}"),
        head_ref: head_ref.to_string(),
        head_sha: format!("head-{id}"),
        merge_state_status: Some("CLEAN".to_string()),
        review_decision: Some("APPROVED".to_string()),
        check_rollup_state: Some("SUCCESS".to_string()),
        updated_at: now_epoch_seconds().unwrap_or(1),
    }
}

fn pull_request_record(pr: &PullRequestRow) -> PullRequestRecord {
    PullRequestRecord {
        id: pr.id.clone(),
        account_id: pr.account_id.clone(),
        repo_id: pr.repo_id.clone(),
        number: pr.number,
        state: pr.state.clone(),
        draft: false,
        title: pr.title.clone(),
        body: String::new(),
        author_id: None,
        base_ref: pr.base_ref.clone(),
        base_sha: pr.base_sha.clone(),
        head_ref: pr.head_ref.clone(),
        head_sha: pr.head_sha.clone(),
        head_repo_id: Some(pr.repo_id.clone()),
        mergeable_state: Some("clean".to_string()),
        merge_state_status: pr.merge_state_status.clone(),
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
        additions: 0,
        deletions: 0,
        changed_files: 0,
        comments_count: 0,
        reviews_count: 0,
        commits_count: 0,
        is_read: true,
        html_url: None,
        created_at: now_epoch_seconds().unwrap_or(1),
        updated_at: now_epoch_seconds().unwrap_or(1),
        closed_at: None,
        merged_at: None,
    }
}

async fn seed_repo_and_prs(
    db: &Db,
    account_id: &str,
    repo_id: &str,
    prs: &[PullRequestRow],
) -> Result<()> {
    db.upsert_repo(&RepoRecord {
        id: repo_id.to_string(),
        account_id: account_id.to_string(),
        owner: "octo".to_string(),
        name: "stack-repo".to_string(),
        default_branch: Some("main".to_string()),
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: now_epoch_seconds().unwrap_or(1),
        updated_at: now_epoch_seconds().unwrap_or(1),
    })
    .await?;
    for pr in prs {
        db.upsert_pull_request(&pull_request_record(pr)).await?;
    }
    Ok(())
}

#[tokio::test]
async fn stack_state_round_trip_replaces_obsolete_rows() -> Result<()> {
    let temp = TempDir::new()?;
    let db = Db::open(temp.path()).await?;
    let account = db
        .upsert_auth_account(
            "github.com",
            "stack-sync",
            "pat",
            "repo",
            now_epoch_seconds().unwrap_or(1),
        )
        .await?;
    let account_id = account.id;
    let repo_id = "repo_stack";
    let first_prs = vec![
        pr(&account_id, repo_id, "pr_c", 3, "main", "stack/c"),
        pr(&account_id, repo_id, "pr_b", 2, "stack/c", "stack/b"),
        pr(&account_id, repo_id, "pr_a", 1, "stack/b", "stack/a"),
    ];
    seed_repo_and_prs(&db, &account_id, repo_id, &first_prs).await?;

    let first = detect_stacks(&first_prs, repo_id, &account_id);
    upsert_stack_state_for_scope(&db, &account_id, repo_id, &first).await?;
    let first_stack_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stacks WHERE account_id = ?1 AND repo_id = ?2")
            .bind(&account_id)
            .bind(repo_id)
            .fetch_one(db.pool())
            .await?;
    let first_position_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pr_stack_position WHERE stack_id IN (
            SELECT id FROM stacks WHERE account_id = ?1 AND repo_id = ?2
        )",
    )
    .bind(&account_id)
    .bind(repo_id)
    .fetch_one(db.pool())
    .await?;
    assert_eq!(first_stack_count, 1);
    assert_eq!(first_position_count, 3);

    let second_prs = vec![
        pr(&account_id, repo_id, "pr_root", 20, "main", "stack/root"),
        pr(
            &account_id,
            repo_id,
            "pr_leaf",
            21,
            "stack/root",
            "stack/leaf",
        ),
    ];
    seed_repo_and_prs(&db, &account_id, repo_id, &second_prs).await?;
    let second = detect_stacks(&second_prs, repo_id, &account_id);
    upsert_stack_state_for_scope(&db, &account_id, repo_id, &second).await?;
    let second_stack_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stacks WHERE account_id = ?1 AND repo_id = ?2")
            .bind(&account_id)
            .bind(repo_id)
            .fetch_one(db.pool())
            .await?;
    let second_position_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pr_stack_position WHERE stack_id IN (
            SELECT id FROM stacks WHERE account_id = ?1 AND repo_id = ?2
        )",
    )
    .bind(&account_id)
    .bind(repo_id)
    .fetch_one(db.pool())
    .await?;
    let legacy_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pr_stack_position WHERE pr_id IN ('pr_a','pr_b','pr_c')",
    )
    .fetch_one(db.pool())
    .await?;
    assert_eq!(second_stack_count, 1);
    assert_eq!(second_position_count, 2);
    assert_eq!(legacy_rows, 0);
    Ok(())
}

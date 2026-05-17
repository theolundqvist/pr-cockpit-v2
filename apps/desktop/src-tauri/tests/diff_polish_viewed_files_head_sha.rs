use anyhow::Result;
use desktop_lib::db::{AccountRecord, Db, PrFileRecord, PullRequestRecord, RepoRecord, UserRecord};
use desktop_lib::ipc::{ipc_pr_files_impl, PrFilesInput};

#[tokio::test]
async fn viewed_state_is_scoped_to_current_head_sha() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Db::open(temp.path()).await?;
    let account_id = "acct-viewed";
    let pr_id = "pr-viewed";
    let repo_id = "repo-viewed";
    let head_a = "head-a";
    let head_b = "head-b";

    db.upsert_account(&AccountRecord {
        id: account_id.to_string(),
        host: "github.com".to_string(),
        login: "user-1".to_string(),
        token_kind: "pat".to_string(),
        scopes: "repo".to_string(),
        created_at: 1,
        updated_at: 1,
    })
    .await?;
    db.upsert_repo(&RepoRecord {
        id: repo_id.to_string(),
        account_id: account_id.to_string(),
        owner: "octo".to_string(),
        name: "hello-world".to_string(),
        default_branch: Some("main".to_string()),
        description: None,
        html_url: None,
        is_private: false,
        is_archived: false,
        pushed_at: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;
    db.upsert_user(&UserRecord {
        id: "user-1".to_string(),
        account_id: account_id.to_string(),
        login: "user-1".to_string(),
        display_name: None,
        avatar_url: None,
        html_url: None,
        created_at: 1,
        updated_at: 1,
    })
    .await?;
    db.upsert_pull_request(&PullRequestRecord {
        id: pr_id.to_string(),
        account_id: account_id.to_string(),
        repo_id: repo_id.to_string(),
        number: 1,
        state: "open".to_string(),
        draft: false,
        title: "Viewed test".to_string(),
        body: "body".to_string(),
        author_id: Some("user-1".to_string()),
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feature".to_string(),
        head_sha: head_a.to_string(),
        head_repo_id: Some(repo_id.to_string()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: Some("CLEAN".to_string()),
        additions: 0,
        deletions: 0,
        changed_files: 3,
        comments_count: 0,
        reviews_count: 0,
        commits_count: 0,
        is_read: false,
        html_url: None,
        created_at: 1,
        updated_at: 1,
        closed_at: None,
        merged_at: None,
    })
    .await?;

    for path in ["a.rs", "b.rs", "c.rs"] {
        db.upsert_pr_file(&PrFileRecord {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_a.to_string(),
            path: path.to_string(),
            old_path: None,
            previous_path: None,
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            is_binary: false,
            kind: "text".to_string(),
            rename_similarity: None,
            patch_blob_sha: None,
            viewed_by_account_id: Some(account_id.to_string()),
            viewed_at_head_sha: Some(head_a.to_string()),
        })
        .await?;
        db.upsert_pr_file(&PrFileRecord {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_b.to_string(),
            path: path.to_string(),
            old_path: None,
            previous_path: None,
            status: "modified".to_string(),
            additions: 1,
            deletions: 1,
            is_binary: false,
            kind: "text".to_string(),
            rename_similarity: None,
            patch_blob_sha: None,
            viewed_by_account_id: Some(account_id.to_string()),
            viewed_at_head_sha: Some(head_a.to_string()),
        })
        .await?;
    }

    let viewed_at_a = ipc_pr_files_impl(
        &db,
        PrFilesInput {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_a.to_string(),
        },
    )
    .await
    .map_err(|error| anyhow::anyhow!("ipc_pr_files_impl failed: {error:?}"))?;
    assert_eq!(
        viewed_at_a
            .files
            .iter()
            .filter(|file| file.is_viewed)
            .count(),
        3
    );

    sqlx::query("UPDATE pull_requests SET head_sha = ?1 WHERE account_id = ?2 AND id = ?3")
        .bind(head_b)
        .bind(account_id)
        .bind(pr_id)
        .execute(db.pool())
        .await?;

    let viewed_at_b = ipc_pr_files_impl(
        &db,
        PrFilesInput {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_b.to_string(),
        },
    )
    .await
    .map_err(|error| anyhow::anyhow!("ipc_pr_files_impl failed: {error:?}"))?;
    assert_eq!(
        viewed_at_b
            .files
            .iter()
            .filter(|file| file.is_viewed)
            .count(),
        0
    );

    Ok(())
}

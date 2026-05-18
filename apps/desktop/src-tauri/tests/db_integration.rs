use std::time::{Duration, Instant};

use anyhow::Result;
use desktop_lib::db::{
    AccountRecord, BlobKind, Db, PrFileRecord, PullRequestRecord, RepoRecord, UserRecord,
};
use desktop_lib::ipc::{ipc_pr_files_impl, PrFilesInput};
use tempfile::TempDir;

const ACCOUNT_ID: &str = "acct_demo";

#[tokio::test]
async fn migrations_apply_cleanly_to_empty_file() -> Result<()> {
    let temp = TempDir::new()?;
    let db = Db::open(temp.path()).await?;

    let required_tables = [
        "accounts",
        "repos",
        "repo_subscriptions",
        "users",
        "orgs",
        "pull_requests",
        "pr_labels",
        "pr_assignees",
        "pr_reviewers",
        "pr_projects",
        "pr_milestones",
        "commits",
        "pr_commits",
        "comments",
        "review_threads",
        "reviews",
        "check_suites",
        "check_runs",
        "check_annotations",
        "pr_files",
        "pr_patches",
        "notifications",
        "pr_pushes",
        "worktrees",
        "pending_mutations",
        "mutation_attempts",
        "id_mappings",
        "sync_cursors",
        "rate_limit_buckets",
        "blob_refs",
        "search_documents",
    ];

    for table in required_tables {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind(table)
        .fetch_one(db.pool())
        .await?;
        assert_eq!(count, 1, "table `{table}` should exist");
    }

    let required_views = [
        "pr_inbox_rows",
        "pr_detail_summary",
        "unread_counts",
        "file_tree_summary",
        "account_rate_limits",
        "pr_force_push_pairs",
    ];
    for view in required_views {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'view' AND name = ?1",
        )
        .bind(view)
        .fetch_one(db.pool())
        .await?;
        assert_eq!(count, 1, "view `{view}` should exist");
    }

    Ok(())
}

#[tokio::test]
async fn read_models_match_hand_rolled_selects_on_fixture() -> Result<()> {
    let db = Db::open_fixture().await?;

    assert_query_sets_equal(
        db.pool(),
        "SELECT account_id, account_login, account_host, pr_id, repo_id, repo_owner, repo_name, pr_number, title, state, draft, head_sha, base_sha, mergeable_state, merge_state_status, updated_at, author_login, unread_notification_count, latest_notification_at FROM pr_inbox_rows WHERE account_id = ?1",
        "SELECT
            pr.account_id,
            acc.login AS account_login,
            acc.host AS account_host,
            pr.id AS pr_id,
            pr.repo_id,
            r.owner AS repo_owner,
            r.name AS repo_name,
            pr.number AS pr_number,
            pr.title,
            pr.state,
            pr.draft,
            pr.head_sha,
            pr.base_sha,
            pr.mergeable_state,
            pr.merge_state_status,
            pr.updated_at,
            COALESCE(u.login, 'unknown') AS author_login,
            COALESCE((SELECT COUNT(*) FROM notifications n WHERE n.account_id = pr.account_id AND n.pr_id = pr.id AND n.unread = 1), 0) AS unread_notification_count,
            (SELECT MAX(updated_at) FROM notifications n WHERE n.account_id = pr.account_id AND n.pr_id = pr.id AND n.unread = 1) AS latest_notification_at
         FROM pull_requests pr
         JOIN repos r ON r.id = pr.repo_id
         JOIN accounts acc ON acc.id = pr.account_id
         LEFT JOIN users u ON u.id = pr.author_id
         WHERE pr.state = 'open' AND pr.account_id = ?1",
        ACCOUNT_ID,
    )
    .await?;

    assert_query_sets_equal(
        db.pool(),
        "SELECT account_id, pr_id, repo_id, pr_number, title, body, state, draft, base_ref, base_sha, head_ref, head_sha, mergeable_state, merge_state_status, additions, deletions, changed_files, comment_count, review_count, thread_count, check_run_count, file_count, updated_at FROM pr_detail_summary WHERE account_id = ?1",
        "SELECT
            pr.account_id,
            pr.id AS pr_id,
            pr.repo_id,
            pr.number AS pr_number,
            pr.title,
            pr.body,
            pr.state,
            pr.draft,
            pr.base_ref,
            pr.base_sha,
            pr.head_ref,
            pr.head_sha,
            pr.mergeable_state,
            pr.merge_state_status,
            pr.additions,
            pr.deletions,
            pr.changed_files,
            (SELECT COUNT(*) FROM comments c WHERE c.account_id = pr.account_id AND c.pr_id = pr.id) AS comment_count,
            (SELECT COUNT(*) FROM reviews rv WHERE rv.account_id = pr.account_id AND rv.pr_id = pr.id) AS review_count,
            (SELECT COUNT(*) FROM review_threads rt WHERE rt.account_id = pr.account_id AND rt.pr_id = pr.id) AS thread_count,
            (SELECT COUNT(*) FROM check_runs cr WHERE cr.account_id = pr.account_id AND cr.pr_id = pr.id) AS check_run_count,
            (SELECT COUNT(*) FROM pr_files pf WHERE pf.account_id = pr.account_id AND pf.pr_id = pr.id AND pf.head_sha = pr.head_sha) AS file_count,
            pr.updated_at
         FROM pull_requests pr
         WHERE pr.account_id = ?1",
        ACCOUNT_ID,
    )
    .await?;

    assert_query_sets_equal(
        db.pool(),
        "SELECT account_id, total_notifications, unread_notifications, prs_with_unread FROM unread_counts WHERE account_id = ?1",
        "SELECT
            account_id,
            COUNT(*) AS total_notifications,
            SUM(CASE WHEN unread = 1 THEN 1 ELSE 0 END) AS unread_notifications,
            COUNT(DISTINCT CASE WHEN unread = 1 THEN pr_id END) AS prs_with_unread
         FROM notifications
         WHERE account_id = ?1
         GROUP BY account_id",
        ACCOUNT_ID,
    )
    .await?;

    assert_query_sets_equal(
        db.pool(),
        "SELECT account_id, pr_id, head_sha, directory, file_count, viewed_file_count, additions, deletions
         FROM file_tree_summary
         WHERE account_id = ?1 AND pr_id = 'pr_1'",
        "SELECT
            pf.account_id,
            pf.pr_id,
            pf.head_sha,
            CASE
                WHEN instr(pf.path, '/') > 0 THEN substr(pf.path, 1, instr(pf.path, '/') - 1)
                ELSE '.'
            END AS directory,
            COUNT(*) AS file_count,
            SUM(
                CASE
                    WHEN pf.viewed_by_account_id IS NOT NULL AND pf.viewed_at_head_sha = pr.head_sha THEN 1
                    ELSE 0
                END
            ) AS viewed_file_count,
            SUM(pf.additions) AS additions,
            SUM(pf.deletions) AS deletions
         FROM pr_files pf
         JOIN pull_requests pr ON pr.account_id = pf.account_id AND pr.id = pf.pr_id
         WHERE pf.account_id = ?1 AND pf.pr_id = 'pr_1'
         GROUP BY pf.account_id, pf.pr_id, pf.head_sha, directory",
        ACCOUNT_ID,
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn fts_search_returns_expected_hits() -> Result<()> {
    let db = Db::open_fixture().await?;
    let falcon_hits = db.search(ACCOUNT_ID, "falcon", 10).await?;
    assert!(
        falcon_hits
            .iter()
            .any(|hit| hit.doc_type == "pr" && hit.doc_ref == "pr_1"),
        "expected PR hit for falcon query"
    );

    let nebula_hits = db.search(ACCOUNT_ID, "nebula", 10).await?;
    assert!(
        nebula_hits.iter().any(|hit| hit.doc_type == "comment"),
        "expected comment hit for nebula query"
    );

    let file_hits = db.search(ACCOUNT_ID, "huge_fixture", 10).await?;
    assert!(
        file_hits
            .iter()
            .any(|hit| hit.doc_type == "file" && hit.filename.contains("huge_fixture.rs")),
        "expected filename hit for huge_fixture query"
    );
    Ok(())
}

#[tokio::test]
async fn blob_store_round_trip_and_ref_counts_work() -> Result<()> {
    let temp = TempDir::new()?;
    let db = Db::open(temp.path()).await?;

    let blob_sha = db
        .blob_store()
        .put(b"plain blob payload", BlobKind::Asset)
        .await?;
    let blob_sha_second = db
        .blob_store()
        .put(b"plain blob payload", BlobKind::Asset)
        .await?;
    assert_eq!(
        blob_sha, blob_sha_second,
        "same payload should hash-identically"
    );

    let raw = db.blob_store().get(&blob_sha).await?;
    assert_eq!(raw.as_deref(), Some(&b"plain blob payload"[..]));

    let ref_count: i64 = sqlx::query_scalar("SELECT ref_count FROM blob_refs WHERE sha256 = ?1")
        .bind(&blob_sha)
        .fetch_one(db.pool())
        .await?;
    assert_eq!(ref_count, 2, "duplicate puts should increment ref_count");

    let patch_sha = db
        .blob_store()
        .put_patch(b"diff --git a/a b/a\n@@ -1 +1 @@\n-old\n+new\n")
        .await?;
    let patch = db.blob_store().get(&patch_sha).await?;
    assert!(
        patch
            .as_deref()
            .unwrap_or_default()
            .starts_with(b"diff --git a/a b/a"),
        "patch get should transparently decompress"
    );

    let eviction = db.blob_store().evict_lru(0).await?;
    assert!(
        eviction.bytes_before >= eviction.bytes_after,
        "eviction should not increase blob size"
    );
    Ok(())
}

#[tokio::test]
async fn open_fixture_loads_inbox_under_timing_budget_best_effort() -> Result<()> {
    let start = Instant::now();
    let db = Db::open_fixture().await?;
    let rows = db.list_inbox(ACCOUNT_ID).await?;
    let elapsed = start.elapsed();
    eprintln!("fixture_inbox_cold_load_ms={}", elapsed.as_millis());
    assert_eq!(rows.len(), 200, "fixture should expose 200 inbox rows");

    if elapsed > Duration::from_millis(50) {
        eprintln!(
            "best-effort budget warning: fixture inbox cold load exceeded 50ms ({}ms)",
            elapsed.as_millis()
        );
    }
    assert!(
        elapsed < Duration::from_millis(500),
        "fixture inbox cold load should remain below shared-runner regression threshold"
    );
    Ok(())
}

#[tokio::test]
async fn pr_file_rename_round_trip_surfaces_previous_path_and_similarity() -> Result<()> {
    let temp = TempDir::new()?;
    let db = Db::open(temp.path()).await?;
    let account_id = "acct-rename";
    let repo_id = "repo-rename";
    let pr_id = "pr-rename";
    let head_sha = "head-rename";

    db.upsert_account(&AccountRecord {
        id: account_id.to_string(),
        host: "github.com".to_string(),
        login: "rename-user".to_string(),
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
        name: "rename-demo".to_string(),
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
        id: "user-rename".to_string(),
        account_id: account_id.to_string(),
        login: "rename-user".to_string(),
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
        title: "rename fixture".to_string(),
        body: "body".to_string(),
        author_id: Some("user-rename".to_string()),
        base_ref: "main".to_string(),
        base_sha: "base".to_string(),
        head_ref: "feature".to_string(),
        head_sha: head_sha.to_string(),
        head_repo_id: Some(repo_id.to_string()),
        mergeable_state: Some("MERGEABLE".to_string()),
        merge_state_status: Some("CLEAN".to_string()),
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
        additions: 3,
        deletions: 1,
        changed_files: 1,
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
    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        path: "src/new_name.rs".to_string(),
        old_path: Some("src/old_name.rs".to_string()),
        previous_path: Some("src/old_name.rs".to_string()),
        status: "renamed".to_string(),
        additions: 3,
        deletions: 1,
        is_binary: false,
        kind: "text".to_string(),
        rename_similarity: Some(0.92),
        patch_blob_sha: None,
        viewed_by_account_id: None,
        viewed_at_head_sha: None,
    })
    .await?;

    let files = ipc_pr_files_impl(
        &db,
        PrFilesInput {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_sha.to_string(),
        },
    )
    .await
    .map_err(|error| anyhow::anyhow!("ipc_pr_files_impl failed: {error:?}"))?;
    let renamed = files
        .files
        .iter()
        .find(|file| file.path == "src/new_name.rs")
        .expect("renamed file row");
    assert_eq!(renamed.status, "renamed");
    assert_eq!(renamed.previous_path.as_deref(), Some("src/old_name.rs"));
    assert_eq!(renamed.rename_similarity, Some(92));

    Ok(())
}

async fn assert_query_sets_equal(
    pool: &sqlx::SqlitePool,
    lhs: &str,
    rhs: &str,
    account_id: &str,
) -> Result<()> {
    let account_literal = format!("'{}'", account_id.replace('\'', "''"));
    let lhs_sql = lhs.replace("?1", &account_literal);
    let rhs_sql = rhs.replace("?1", &account_literal);

    let lhs_minus_rhs: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM ({lhs_sql} EXCEPT {rhs_sql})"
    ))
    .fetch_one(pool)
    .await?;

    let rhs_minus_lhs: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM ({rhs_sql} EXCEPT {lhs_sql})"
    ))
    .fetch_one(pool)
    .await?;

    assert_eq!(
        lhs_minus_rhs, 0,
        "left query had rows not present in right query"
    );
    assert_eq!(
        rhs_minus_lhs, 0,
        "right query had rows not present in left query"
    );
    Ok(())
}

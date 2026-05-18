use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use desktop_lib::db::{
    AccountRecord, BlobKind, CheckAnnotationRecord, CheckRunRecord, CheckSuiteRecord,
    CommentRecord, CommitRecord, Db, IdMappingRecord, NotificationRecord, OrgRecord,
    PendingMutationRecord, PrCommitRecord, PrFileRecord, PrPatchRecord, PullRequestRecord,
    RateLimitBucketUpdate, RepoRecord, RepoSubscriptionRecord, ReviewRecord, ReviewThreadRecord,
    SyncCursorUpdate, UserRecord, WorktreeRecord,
};

#[tokio::main]
async fn main() -> Result<()> {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    std::fs::create_dir_all(&fixture_dir)
        .with_context(|| format!("creating fixture directory {}", fixture_dir.display()))?;

    let build_dir = tempfile::Builder::new()
        .prefix("fixture-build-")
        .tempdir()
        .context("creating fixture build tempdir")?;

    let db = Db::open(build_dir.path()).await?;
    seed_fixture(&db).await?;
    db.rebuild_search_index().await?;

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .context("checkpointing WAL before fixture copy")?;

    let seed_sql = render_seed_sql();
    std::fs::write(fixture_dir.join("seed.sql"), seed_sql).context("writing seed.sql")?;

    db.pool().close().await;

    let fixture_db_path = fixture_dir.join("cockpit_fixture.db");
    if fixture_db_path.exists() {
        std::fs::remove_file(&fixture_db_path)
            .with_context(|| format!("removing {}", fixture_db_path.display()))?;
    }
    std::fs::copy(build_dir.path().join("cockpit.db"), &fixture_db_path).with_context(|| {
        format!(
            "copying fixture sqlite {} -> {}",
            build_dir.path().join("cockpit.db").display(),
            fixture_db_path.display()
        )
    })?;

    let fixture_blobs = fixture_dir.join("blobs");
    if fixture_blobs.exists() {
        std::fs::remove_dir_all(&fixture_blobs)
            .with_context(|| format!("removing stale {}", fixture_blobs.display()))?;
    }
    copy_dir_recursive(&build_dir.path().join("blobs"), &fixture_blobs)?;

    Ok(())
}

async fn seed_fixture(db: &Db) -> Result<()> {
    const ACCOUNT_ID: &str = "acct_demo";
    let base_ts = 1_715_000_000_i64;

    db.upsert_account(&AccountRecord {
        id: ACCOUNT_ID.to_string(),
        host: "github.com".to_string(),
        login: "fixture-user".to_string(),
        token_kind: "pat".to_string(),
        scopes: "repo,read:org,notifications".to_string(),
        created_at: base_ts,
        updated_at: base_ts,
    })
    .await?;

    db.upsert_org(&OrgRecord {
        id: "org_demo".to_string(),
        account_id: ACCOUNT_ID.to_string(),
        login: "fixture-org".to_string(),
        display_name: Some("Fixture Org".to_string()),
        avatar_url: Some("https://example.test/org.png".to_string()),
        html_url: Some("https://example.test/fixture-org".to_string()),
        created_at: base_ts,
        updated_at: base_ts,
    })
    .await?;

    for idx in 1..=3_i64 {
        let repo_id = format!("repo_{idx}");
        db.upsert_repo(&RepoRecord {
            id: repo_id.clone(),
            account_id: ACCOUNT_ID.to_string(),
            owner: "fixture-org".to_string(),
            name: format!("repo-{idx}"),
            default_branch: Some("main".to_string()),
            description: Some(format!("Fixture repository #{idx}")),
            html_url: Some(format!("https://example.test/fixture-org/repo-{idx}")),
            is_private: false,
            is_archived: false,
            pushed_at: Some(base_ts + idx),
            created_at: base_ts - 5_000,
            updated_at: base_ts + idx,
        })
        .await?;

        db.upsert_repo_subscription(&RepoSubscriptionRecord {
            repo_id,
            account_id: ACCOUNT_ID.to_string(),
            watch_tier: if idx == 1 {
                "hot".to_string()
            } else if idx == 2 {
                "warm".to_string()
            } else {
                "cool".to_string()
            },
            last_full_sync_at: Some(base_ts - 60),
            created_at: base_ts - 60,
            updated_at: base_ts - 60,
        })
        .await?;
    }

    for idx in 1..=40_i64 {
        db.upsert_user(&UserRecord {
            id: format!("user_{idx}"),
            account_id: ACCOUNT_ID.to_string(),
            login: format!("fixture-user-{idx:02}"),
            display_name: Some(format!("Fixture User {idx:02}")),
            avatar_url: Some(format!("https://example.test/u/{idx}.png")),
            html_url: Some(format!("https://example.test/fixture-user-{idx:02}")),
            created_at: base_ts - 10_000 + idx,
            updated_at: base_ts - 10_000 + idx,
        })
        .await?;
    }

    for idx in 1..=200_i64 {
        let repo_id = format!("repo_{}", (idx % 3) + 1);
        let pr_id = format!("pr_{idx}");
        let is_active = idx == 1;
        let head_sha = if is_active {
            "active_head_sha_000000000000000000000000000000000001".to_string()
        } else {
            format!("head_sha_{idx:040x}")
        };

        db.upsert_pull_request(&PullRequestRecord {
            id: pr_id.clone(),
            account_id: ACCOUNT_ID.to_string(),
            repo_id: repo_id.clone(),
            number: idx,
            state: "open".to_string(),
            draft: idx % 11 == 0,
            title: if is_active {
                "Active Fixture PR Falcon Diff Stress".to_string()
            } else {
                format!("Fixture Inbox PR #{idx}")
            },
            body: if is_active {
                "This active fixture PR drives read-only cockpit validation with synthetic diff and timeline."
                    .to_string()
            } else {
                format!("Seeded PR body {idx} for local-first inbox tests.")
            },
            author_id: Some(format!("user_{}", (idx % 40) + 1)),
            base_ref: "main".to_string(),
            base_sha: format!("base_sha_{idx:040x}"),
            head_ref: format!("feature/pr-{idx}"),
            head_sha: head_sha.clone(),
            head_repo_id: Some(repo_id),
            mergeable_state: Some("clean".to_string()),
            merge_state_status: Some("behind".to_string()),
            additions: if is_active { 5_200 } else { 30 + idx },
            deletions: if is_active { 250 } else { 5 + idx },
            changed_files: if is_active { 30 } else { 3 + (idx % 5) },
            comments_count: if is_active { 55 } else { idx % 4 },
            reviews_count: if is_active { 10 } else { idx % 3 },
            commits_count: if is_active { 8 } else { 1 + (idx % 4) },
            is_read: idx % 7 == 0,
            html_url: Some(format!("https://example.test/fixture-org/repo-{}/pull/{idx}", (idx % 3) + 1)),
            created_at: base_ts - 20_000 + idx,
            updated_at: base_ts + idx,
            closed_at: None,
            merged_at: None,
        })
        .await?;

        db.upsert_notification(&NotificationRecord {
            id: format!("notif_{idx}"),
            account_id: ACCOUNT_ID.to_string(),
            repo_id: format!("repo_{}", (idx % 3) + 1),
            pr_id: Some(pr_id.clone()),
            reason: if idx % 2 == 0 {
                "review_requested".to_string()
            } else {
                "mention".to_string()
            },
            subject_type: "PullRequest".to_string(),
            subject_id: pr_id.clone(),
            title: format!("Notification for {pr_id}"),
            unread: idx <= 150,
            updated_at: base_ts + idx,
            last_read_at: if idx <= 150 {
                None
            } else {
                Some(base_ts + idx - 60)
            },
            url: Some(format!("https://example.test/notifs/{idx}")),
        })
        .await?;
    }

    seed_active_pr_details(db, ACCOUNT_ID, base_ts).await?;

    db.upsert_worktree(&WorktreeRecord {
        id: "wt_1".to_string(),
        account_id: ACCOUNT_ID.to_string(),
        repo_id: "repo_1".to_string(),
        path: "/workspace/demo/repo-1".to_string(),
        head_sha: "active_head_sha_000000000000000000000000000000000001".to_string(),
        branch: "feature/pr-1".to_string(),
        dirty: false,
        ahead: 1,
        behind: 0,
        mapped_pr_id: Some("pr_1".to_string()),
        mapping_confidence: Some(0.97),
        mapping_source: Some("head_sha".to_string()),
        is_app_managed: false,
        manual_override_pr_id: None,
        manual_override_at: None,
        last_cleanup_snapshot_id: None,
        untracked_count: 0,
        staged_count: 0,
        modified_count: 0,
        created_at: base_ts,
        updated_at: base_ts + 5,
    })
    .await?;

    db.apply_pending_mutation(&PendingMutationRecord {
        id: "mut_1".to_string(),
        account_id: ACCOUNT_ID.to_string(),
        kind: "addComment".to_string(),
        target_type: "pull_request".to_string(),
        target_id: "pr_1".to_string(),
        idempotency_key: "fixture-mut-1".to_string(),
        input_json: r#"{"body":"offline comment"}"#.to_string(),
        optimistic_patch_json: r#"{"comments_count_delta":1}"#.to_string(),
        inverse_patch_json: r#"{"comments_count_delta":-1}"#.to_string(),
        status: "pending".to_string(),
        retries: 0,
        created_at: base_ts + 10,
        updated_at: base_ts + 10,
        last_error: None,
        requires_connection_confirmation: false,
    })
    .await?;

    db.record_id_mapping(&IdMappingRecord {
        account_id: ACCOUNT_ID.to_string(),
        kind: "comment".to_string(),
        local_id: "local_comment_1".to_string(),
        server_id: "comment_server_1".to_string(),
        created_at: base_ts + 10,
    })
    .await?;

    let _ = db
        .update_sync_cursor(&SyncCursorUpdate {
            account_id: ACCOUNT_ID.to_string(),
            resource: "notifications".to_string(),
            cursor: Some("cursor_200".to_string()),
            etag: Some("W/\"fixture-etag-1\"".to_string()),
            expected_previous_etag: None,
            fetched_at: base_ts + 200,
        })
        .await?;

    db.update_rate_limit_bucket(&RateLimitBucketUpdate {
        account_id: ACCOUNT_ID.to_string(),
        resource: "graphql".to_string(),
        remaining: 4_800,
        used: Some(200),
        limit_total: 5_000,
        reset_at: base_ts + 3_600,
        updated_at: base_ts + 200,
    })
    .await?;

    Ok(())
}

async fn seed_active_pr_details(db: &Db, account_id: &str, base_ts: i64) -> Result<()> {
    let pr_id = "pr_1";
    let head_sha = "active_head_sha_000000000000000000000000000000000001";
    let diff = synthesize_large_diff();
    let patch_sha = db.blob_store().put_patch(diff.as_bytes()).await?;
    let image_blob_sha = db
        .blob_store()
        .put(test_pattern_png(), BlobKind::Asset)
        .await?;
    let binary_blob_sha = db
        .blob_store()
        .put(binary_header_bytes(), BlobKind::Asset)
        .await?;

    db.upsert_pr_patch(&PrPatchRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        patch_blob_sha: patch_sha.clone(),
        fetched_at: base_ts + 100,
    })
    .await?;

    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        path: "src/generated/huge_fixture.rs".to_string(),
        old_path: Some("src/generated/huge_fixture.rs".to_string()),
        previous_path: Some("src/generated/huge_fixture.rs".to_string()),
        status: "modified".to_string(),
        additions: 5_000,
        deletions: 0,
        is_binary: false,
        kind: "text".to_string(),
        rename_similarity: None,
        patch_blob_sha: Some(patch_sha),
        viewed_by_account_id: Some(account_id.to_string()),
        viewed_at_head_sha: Some(head_sha.to_string()),
    })
    .await?;

    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        path: "assets/test-pattern.png".to_string(),
        old_path: Some("assets/test-pattern.png".to_string()),
        previous_path: Some("assets/test-pattern.png".to_string()),
        status: "modified".to_string(),
        additions: 1,
        deletions: 1,
        is_binary: true,
        kind: "image".to_string(),
        rename_similarity: None,
        patch_blob_sha: Some(image_blob_sha),
        viewed_by_account_id: None,
        viewed_at_head_sha: None,
    })
    .await?;

    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        path: "assets/header.bin".to_string(),
        old_path: None,
        previous_path: None,
        status: "added".to_string(),
        additions: 0,
        deletions: 0,
        is_binary: true,
        kind: "binary".to_string(),
        rename_similarity: None,
        patch_blob_sha: Some(binary_blob_sha),
        viewed_by_account_id: None,
        viewed_at_head_sha: None,
    })
    .await?;

    db.upsert_pr_file(&PrFileRecord {
        account_id: account_id.to_string(),
        pr_id: pr_id.to_string(),
        head_sha: head_sha.to_string(),
        path: "src/renamed/new_name.txt".to_string(),
        old_path: Some("src/renamed/old_name.txt".to_string()),
        previous_path: Some("src/renamed/old_name.txt".to_string()),
        status: "renamed".to_string(),
        additions: 2,
        deletions: 2,
        is_binary: false,
        kind: "text".to_string(),
        rename_similarity: Some(0.95),
        patch_blob_sha: None,
        viewed_by_account_id: None,
        viewed_at_head_sha: None,
    })
    .await?;

    for idx in 1..=29_i64 {
        db.upsert_pr_file(&PrFileRecord {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_sha.to_string(),
            path: format!("src/module_{idx:02}/file_{idx:02}.ts"),
            old_path: None,
            previous_path: None,
            status: "added".to_string(),
            additions: 12 + idx,
            deletions: idx % 3,
            is_binary: false,
            kind: "text".to_string(),
            rename_similarity: None,
            patch_blob_sha: None,
            viewed_by_account_id: None,
            viewed_at_head_sha: None,
        })
        .await?;
    }

    for idx in 1..=8_i64 {
        let commit_id = format!("commit_{idx}");
        db.upsert_commit(&CommitRecord {
            id: commit_id.clone(),
            account_id: account_id.to_string(),
            repo_id: "repo_1".to_string(),
            author_id: Some(format!("user_{}", idx + 2)),
            message_headline: format!("Commit headline {idx}"),
            message_body: format!("Commit body {idx} for fixture timeline."),
            committed_at: base_ts + idx,
            parents_json: if idx == 1 {
                "[]".to_string()
            } else {
                format!(r#"["commit_{}"]"#, idx - 1)
            },
        })
        .await?;
        db.upsert_pr_commit(&PrCommitRecord {
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            commit_id,
            commit_order: idx,
        })
        .await?;
    }

    for idx in 1..=5_i64 {
        db.upsert_review_thread(&ReviewThreadRecord {
            id: format!("thread_{idx}"),
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            path: "src/generated/huge_fixture.rs".to_string(),
            line: Some(100 * idx),
            side: Some("RIGHT".to_string()),
            start_line: Some((100 * idx) - 2),
            start_side: Some("RIGHT".to_string()),
            original_commit_sha: Some(
                "base_sha_0000000000000000000000000000000000000001".to_string(),
            ),
            original_path: Some("src/generated/huge_fixture.rs".to_string()),
            original_position: Some(100 * idx),
            original_line: Some(100 * idx),
            is_outdated: idx % 2 == 0,
            is_resolved: idx == 5,
            resolved_by_id: if idx == 5 {
                Some("user_2".to_string())
            } else {
                None
            },
            created_at: base_ts + 300 + idx,
            updated_at: base_ts + 350 + idx,
        })
        .await?;
    }

    for idx in 1..=10_i64 {
        db.upsert_review(&ReviewRecord {
            id: format!("review_{idx}"),
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            author_id: format!("user_{}", idx + 5),
            state: if idx % 3 == 0 {
                "CHANGES_REQUESTED".to_string()
            } else {
                "COMMENTED".to_string()
            },
            body: format!("Review {idx} body mentions saturn gate and fixture quality."),
            commit_sha: Some(format!("commit_{}", ((idx - 1) % 8) + 1)),
            submitted_at: Some(base_ts + 400 + idx),
            created_at: base_ts + 400 + idx,
            updated_at: base_ts + 400 + idx,
        })
        .await?;
    }

    for idx in 1..=55_i64 {
        let kind = if idx <= 25 {
            "issue"
        } else if idx <= 45 {
            "review"
        } else {
            "review_thread_reply"
        };
        db.upsert_comment(&CommentRecord {
            id: format!("comment_{idx}"),
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            kind: kind.to_string(),
            author_id: format!("user_{}", ((idx + 10) % 40) + 1),
            body: format!("Comment {idx} references nebula token and deterministic fixture flows."),
            created_at: base_ts + 500 + idx,
            updated_at: base_ts + 500 + idx,
            deleted_at: None,
            in_reply_to_id: if idx > 45 {
                Some(format!("comment_{}", idx - 5))
            } else {
                None
            },
            review_id: if kind == "review" {
                Some(format!("review_{}", ((idx - 26) % 10) + 1))
            } else {
                None
            },
            thread_id: if kind == "review_thread_reply" {
                Some(format!("thread_{}", ((idx - 46) % 5) + 1))
            } else {
                None
            },
            path: Some("src/generated/huge_fixture.rs".to_string()),
            line: Some(40 + idx),
            side: Some("RIGHT".to_string()),
            start_line: None,
            start_side: None,
            original_commit_sha: Some(
                "base_sha_0000000000000000000000000000000000000001".to_string(),
            ),
        })
        .await?;
    }

    for suite in 1..=2_i64 {
        let suite_id = format!("suite_{suite}");
        db.upsert_check_suite(&CheckSuiteRecord {
            id: suite_id.clone(),
            account_id: account_id.to_string(),
            pr_id: pr_id.to_string(),
            head_sha: head_sha.to_string(),
            app_name: if suite == 1 {
                "ci-linux".to_string()
            } else {
                "ci-macos".to_string()
            },
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
            details_url: Some(format!("https://example.test/check-suites/{suite}")),
            created_at: base_ts + 700 + suite,
            updated_at: base_ts + 710 + suite,
        })
        .await?;

        for run in 1..=5_i64 {
            let run_id = format!("run_{}_{}", suite, run);
            db.upsert_check_run(&CheckRunRecord {
                id: run_id.clone(),
                account_id: account_id.to_string(),
                check_suite_id: suite_id.clone(),
                pr_id: pr_id.to_string(),
                name: format!("suite-{suite}-run-{run}"),
                status: "completed".to_string(),
                conclusion: Some("success".to_string()),
                details_url: Some(format!("https://example.test/check-runs/{suite}/{run}")),
                output_title: Some("Fixture check output".to_string()),
                output_summary: Some("Everything passed in deterministic fixture.".to_string()),
                started_at: Some(base_ts + 720 + run),
                completed_at: Some(base_ts + 730 + run),
                created_at: base_ts + 720 + run,
                updated_at: base_ts + 730 + run,
            })
            .await?;

            if run <= 2 {
                db.upsert_check_annotation(&CheckAnnotationRecord {
                    id: format!("ann_{}_{}", suite, run),
                    account_id: account_id.to_string(),
                    check_run_id: run_id,
                    pr_id: pr_id.to_string(),
                    path: "src/generated/huge_fixture.rs".to_string(),
                    start_line: 20 * run,
                    end_line: 20 * run,
                    start_column: Some(1),
                    end_column: Some(10),
                    annotation_level: "notice".to_string(),
                    title: Some("Fixture annotation".to_string()),
                    message: "Deterministic annotation message".to_string(),
                    raw_details: None,
                })
                .await?;
            }
        }
    }

    let mut tx = db.pool().begin().await?;
    sqlx::query("INSERT INTO pr_labels(account_id, pr_id, label_name, label_color, description) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(account_id)
        .bind(pr_id)
        .bind("needs-review")
        .bind("fbca04")
        .bind("Needs review before merge")
        .execute(tx.as_mut())
        .await?;
    sqlx::query(
        "INSERT INTO pr_assignees(account_id, pr_id, user_id, assigned_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(account_id)
    .bind(pr_id)
    .bind("user_3")
    .bind(base_ts + 810)
    .execute(tx.as_mut())
    .await?;
    sqlx::query("INSERT INTO pr_reviewers(account_id, pr_id, user_id, reviewer_type, reviewer_state, requested_at) VALUES (?1, ?2, ?3, 'user', 'requested', ?4)")
        .bind(account_id)
        .bind(pr_id)
        .bind("user_4")
        .bind(base_ts + 811)
        .execute(tx.as_mut())
        .await?;
    sqlx::query("INSERT INTO pr_projects(account_id, pr_id, project_id, project_title, item_id, status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
        .bind(account_id)
        .bind(pr_id)
        .bind("proj_1")
        .bind("Roadmap")
        .bind("item_1")
        .bind("In Review")
        .bind(base_ts + 812)
        .execute(tx.as_mut())
        .await?;
    sqlx::query("INSERT INTO pr_milestones(account_id, pr_id, milestone_id, title, state, due_on, description) VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6)")
        .bind(account_id)
        .bind(pr_id)
        .bind("mile_1")
        .bind("M1 cockpit")
        .bind(base_ts + 86_400)
        .bind("Milestone for read-only cockpit")
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;

    Ok(())
}

fn synthesize_large_diff() -> String {
    let mut diff = String::new();
    diff.push_str("diff --git a/src/generated/huge_fixture.rs b/src/generated/huge_fixture.rs\n");
    diff.push_str("index 0000000..1111111 100644\n");
    diff.push_str("--- a/src/generated/huge_fixture.rs\n");
    diff.push_str("+++ b/src/generated/huge_fixture.rs\n");
    diff.push_str("@@ -0,0 +1,5000 @@\n");
    for line in 1..=5_000_i64 {
        let _ = writeln!(
            diff,
            "+pub const LINE_{line:04}: &str = \"synthetic fixture line {line:04}\";"
        );
    }
    diff
}

fn test_pattern_png() -> &'static [u8] {
    &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, b'I', b'D', b'A', b'T', 0x08, 0x99, 0x63, 0xF8,
        0x0F, 0x04, 0x00, 0x09, 0xFB, 0x03, 0xFD, 0xA7, 0x8A, 0xA2, 0x25, 0x00, 0x00, 0x00, 0x00,
        b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn binary_header_bytes() -> &'static [u8] {
    &[
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D,
        0x1E, 0x1F,
    ]
}

fn render_seed_sql() -> String {
    let mut sql = String::new();
    sql.push_str("-- Generated by `cargo run -p desktop --bin fixture-build`\n");
    sql.push_str("BEGIN;\n");
    sql.push_str("INSERT INTO accounts(id, host, login, token_kind, scopes, created_at, updated_at) VALUES ('acct_demo', 'github.com', 'fixture-user', 'pat', 'repo,read:org,notifications', 1715000000, 1715000000);\n");
    sql.push_str("INSERT INTO orgs(id, account_id, login, display_name, avatar_url, html_url, created_at, updated_at) VALUES ('org_demo', 'acct_demo', 'fixture-org', 'Fixture Org', 'https://example.test/org.png', 'https://example.test/fixture-org', 1715000000, 1715000000);\n");

    for idx in 1..=3_i64 {
        let _ = writeln!(
            sql,
            "INSERT INTO repos(id, account_id, owner, name, default_branch, description, html_url, is_private, is_archived, pushed_at, created_at, updated_at) VALUES ('repo_{idx}', 'acct_demo', 'fixture-org', 'repo-{idx}', 'main', 'Fixture repository #{idx}', 'https://example.test/fixture-org/repo-{idx}', 0, 0, {}, 1714995000, {});",
            1715000000 + idx,
            1715000000 + idx
        );
    }

    for idx in 1..=40_i64 {
        let _ = writeln!(
            sql,
            "INSERT INTO users(id, account_id, login, display_name, avatar_url, html_url, created_at, updated_at) VALUES ('user_{idx}', 'acct_demo', 'fixture-user-{idx:02}', 'Fixture User {idx:02}', 'https://example.test/u/{idx}.png', 'https://example.test/fixture-user-{idx:02}', {}, {});",
            1714990000 + idx,
            1714990000 + idx
        );
    }

    for idx in 1..=200_i64 {
        let repo = (idx % 3) + 1;
        let title = if idx == 1 {
            "Active Fixture PR Falcon Diff Stress".to_string()
        } else {
            format!("Fixture Inbox PR #{idx}")
        };
        let _ = writeln!(
            sql,
            "INSERT INTO pull_requests(id, account_id, repo_id, number, state, draft, title, body, author_id, base_ref, base_sha, head_ref, head_sha, head_repo_id, mergeable_state, merge_state_status, additions, deletions, changed_files, comments_count, reviews_count, commits_count, is_read, html_url, created_at, updated_at) VALUES ('pr_{idx}', 'acct_demo', 'repo_{repo}', {idx}, 'open', {}, '{}', 'Seeded PR body {idx} for local-first inbox tests.', 'user_{}', 'main', 'base_sha_{idx:040x}', 'feature/pr-{idx}', '{}', 'repo_{repo}', 'clean', 'behind', {}, {}, {}, {}, {}, {}, {}, 'https://example.test/fixture-org/repo-{repo}/pull/{idx}', {}, {});",
            if idx % 11 == 0 { 1 } else { 0 },
            title.replace('\'', "''"),
            (idx % 40) + 1,
            if idx == 1 {
                "active_head_sha_000000000000000000000000000000000001".to_string()
            } else {
                format!("head_sha_{idx:040x}")
            },
            if idx == 1 { 5200 } else { 30 + idx },
            if idx == 1 { 250 } else { 5 + idx },
            if idx == 1 { 30 } else { 3 + (idx % 5) },
            if idx == 1 { 55 } else { idx % 4 },
            if idx == 1 { 10 } else { idx % 3 },
            if idx == 1 { 8 } else { 1 + (idx % 4) },
            if idx % 7 == 0 { 1 } else { 0 },
            1714980000 + idx,
            1715000000 + idx
        );

        let _ = writeln!(
            sql,
            "INSERT INTO notifications(id, account_id, repo_id, pr_id, reason, subject_type, subject_id, title, unread, updated_at, last_read_at, url) VALUES ('notif_{idx}', 'acct_demo', 'repo_{repo}', 'pr_{idx}', '{}', 'PullRequest', 'pr_{idx}', 'Notification for pr_{idx}', {}, {}, {}, 'https://example.test/notifs/{idx}');",
            if idx % 2 == 0 { "review_requested" } else { "mention" },
            if idx <= 150 { 1 } else { 0 },
            1715000000 + idx,
            if idx <= 150 {
                "NULL".to_string()
            } else {
                (1715000000 + idx - 60).to_string()
            }
        );
    }

    sql.push_str("COMMIT;\n");
    sql
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("creating destination dir {}", dst.display()))?;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        std::fs::create_dir_all(&to)
            .with_context(|| format!("creating destination dir {}", to.display()))?;
        for entry in std::fs::read_dir(&from)
            .with_context(|| format!("reading source dir {}", from.display()))?
        {
            let entry = entry?;
            let source_path = entry.path();
            let target_path = to.join(entry.file_name());
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push((source_path, target_path));
            } else if file_type.is_file() {
                std::fs::copy(&source_path, &target_path).with_context(|| {
                    format!(
                        "copying fixture blob {} -> {}",
                        source_path.display(),
                        target_path.display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

use anyhow::{anyhow, Context, Result};

use crate::db::{
    CommentRecord, PrAssigneeRecord, PrLabelRecord, PrReviewerRecord, PullRequestRecord,
    ReviewRecord, ReviewThreadRecord,
};
use crate::sync::reconcile::RefetchTarget;

use super::{IdMappingDraft, MarkdownOverlayTarget, ReconcileCtx, ServerNode, ServerResponse};

pub async fn reconcile(ctx: &ReconcileCtx<'_>, response: ServerResponse) -> Result<()> {
    {
        let mut tx = ctx.db.pool().begin().await?;
        for mapping in &response.id_mappings {
            apply_id_mapping(ctx, tx.as_mut(), mapping).await?;
        }
        tx.commit().await?;
    }

    upsert_nodes(ctx, &response.upserts).await?;

    {
        let mut tx = ctx.db.pool().begin().await?;
        apply_markdown_overlays(tx.as_mut(), &response).await?;
        tx.commit().await?;
    }

    schedule_refetches(ctx, &response).await?;

    if let Some(emitter) = ctx.cache_invalidation_emitter {
        for pr_id in &response.refetch_pr_ids {
            emitter.emit_pr_changed(pr_id);
        }
    }

    Ok(())
}

async fn upsert_nodes(ctx: &ReconcileCtx<'_>, nodes: &[ServerNode]) -> Result<()> {
    for node in nodes {
        match node {
            ServerNode::PullRequest(record) => {
                ctx.db.upsert_pull_request(record).await?;
            }
            ServerNode::Review(record) => {
                ctx.db.upsert_review(record).await?;
            }
            ServerNode::ReviewThread(record) => {
                ctx.db.upsert_review_thread(record).await?;
            }
            ServerNode::PrLabels {
                account_id,
                pr_id,
                labels,
            } => {
                ctx.db.replace_pr_labels(account_id, pr_id, labels).await?;
            }
            ServerNode::PrAssignees {
                account_id,
                pr_id,
                assignees,
            } => {
                ctx.db
                    .replace_pr_assignees(account_id, pr_id, assignees)
                    .await?;
            }
            ServerNode::PrReviewers {
                account_id,
                pr_id,
                reviewers,
            } => {
                ctx.db
                    .replace_pr_reviewers(account_id, pr_id, reviewers)
                    .await?;
            }
            ServerNode::Comment(_) => {}
        }
    }
    for node in nodes {
        if let ServerNode::Comment(record) = node {
            ctx.db.upsert_comment(record).await?;
        }
    }
    Ok(())
}

async fn apply_id_mapping(
    ctx: &ReconcileCtx<'_>,
    tx: &mut sqlx::SqliteConnection,
    mapping: &IdMappingDraft,
) -> Result<()> {
    let existing_server_id = sqlx::query_scalar::<_, String>(
        "SELECT server_id FROM id_mappings WHERE account_id = ?1 AND kind = ?2 AND local_id = ?3",
    )
    .bind(ctx.account_id)
    .bind(&mapping.kind)
    .bind(&mapping.local_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(existing_server_id) = existing_server_id {
        if existing_server_id != mapping.server_id {
            return Err(anyhow!(
                "id_mappings monotonicity violated for {}:{} ({})",
                mapping.kind,
                mapping.local_id,
                existing_server_id
            ));
        }
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO id_mappings(account_id, kind, local_id, server_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(ctx.account_id)
    .bind(&mapping.kind)
    .bind(&mapping.local_id)
    .bind(&mapping.server_id)
    .bind(now_epoch_seconds()?)
    .execute(&mut *tx)
    .await?;

    cascade_id_swap(tx, mapping).await?;
    Ok(())
}

async fn cascade_id_swap(tx: &mut sqlx::SqliteConnection, mapping: &IdMappingDraft) -> Result<()> {
    let kind = mapping.kind.as_str();

    if kind == "comment" {
        sqlx::query("UPDATE comments SET in_reply_to_id = ?1 WHERE in_reply_to_id = ?2")
            .bind(&mapping.server_id)
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE comments SET id = ?1 WHERE id = ?2")
            .bind(&mapping.server_id)
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
    }

    if kind == "review" {
        sqlx::query("UPDATE comments SET review_id = ?1 WHERE review_id = ?2")
            .bind(&mapping.server_id)
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE reviews SET id = ?1 WHERE id = ?2")
            .bind(&mapping.server_id)
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
    }

    if kind == "thread" {
        sqlx::query(
            "INSERT OR IGNORE INTO review_threads(
               id, account_id, pr_id, path, line, side, start_line, start_side,
               original_commit_sha, original_path, original_position, original_line,
               is_outdated, is_resolved, resolved_by_id, created_at, updated_at, pending_state
             )
             SELECT
               ?1, account_id, pr_id, path, line, side, start_line, start_side,
               original_commit_sha, original_path, original_position, original_line,
               is_outdated, is_resolved, resolved_by_id, created_at, updated_at, pending_state
             FROM review_threads
             WHERE id = ?2",
        )
        .bind(&mapping.server_id)
        .bind(&mapping.local_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE comments SET thread_id = ?1 WHERE thread_id = ?2")
            .bind(&mapping.server_id)
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM review_threads WHERE id = ?1")
            .bind(&mapping.local_id)
            .execute(&mut *tx)
            .await?;
    }

    if kind == "pull_request" {
        for sql in [
            "UPDATE comments SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE reviews SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE review_threads SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_labels SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_assignees SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_reviewers SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_projects SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_milestones SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_files SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pr_patches SET pr_id = ?1 WHERE pr_id = ?2",
            "UPDATE pull_requests SET id = ?1 WHERE id = ?2",
            "UPDATE pending_mutations SET target_id = ?1 WHERE target_id = ?2 AND target_type = 'pull_request'",
        ] {
            sqlx::query(sql)
                .bind(&mapping.server_id)
                .bind(&mapping.local_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    Ok(())
}

async fn apply_markdown_overlays(
    tx: &mut sqlx::SqliteConnection,
    response: &ServerResponse,
) -> Result<()> {
    for overlay in &response.markdown_overlays {
        let server_adjusted = if overlay.server_body != overlay.predicted_body {
            1_i64
        } else {
            0_i64
        };
        let adjusted_at = if server_adjusted == 1 {
            Some(now_epoch_seconds()?)
        } else {
            None
        };

        match &overlay.target {
            MarkdownOverlayTarget::Comment { comment_id } => {
                sqlx::query(
                    "UPDATE comments
                     SET body = ?1,
                         body_server_adjusted = ?2,
                         server_adjusted_at = ?3
                     WHERE id = ?4",
                )
                .bind(&overlay.server_body)
                .bind(server_adjusted)
                .bind(adjusted_at)
                .bind(comment_id)
                .execute(&mut *tx)
                .await?;
            }
            MarkdownOverlayTarget::Review { review_id } => {
                sqlx::query(
                    "UPDATE reviews
                     SET body = ?1,
                         body_server_adjusted = ?2,
                         server_adjusted_at = ?3
                     WHERE id = ?4",
                )
                .bind(&overlay.server_body)
                .bind(server_adjusted)
                .bind(adjusted_at)
                .bind(review_id)
                .execute(&mut *tx)
                .await?;
            }
            MarkdownOverlayTarget::PullRequest { pr_id } => {
                sqlx::query(
                    "UPDATE pull_requests
                     SET body = ?1,
                         body_server_adjusted = ?2,
                         server_adjusted_at = ?3
                     WHERE id = ?4",
                )
                .bind(&overlay.server_body)
                .bind(server_adjusted)
                .bind(adjusted_at)
                .bind(pr_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    Ok(())
}

async fn schedule_refetches(ctx: &ReconcileCtx<'_>, response: &ServerResponse) -> Result<()> {
    let Some(sync_handle) = ctx.sync_handle else {
        return Ok(());
    };

    let mut targets = Vec::new();
    for pr_id in &response.refetch_pr_ids {
        let row = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT r.owner, r.name, pr.number
             FROM pull_requests pr
             JOIN repos r ON r.id = pr.repo_id
             WHERE pr.account_id = ?1 AND pr.id = ?2
             LIMIT 1",
        )
        .bind(ctx.account_id)
        .bind(pr_id)
        .fetch_optional(ctx.db.pool())
        .await?;

        if let Some((owner, repo, number)) = row {
            targets.push(RefetchTarget {
                owner,
                repo,
                number,
            });
        }
    }

    if !targets.is_empty() {
        sync_handle.enqueue_refetch(targets).await?;
    }

    Ok(())
}

pub fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}

#[allow(dead_code)]
fn _assert_types(
    _pull_request: PullRequestRecord,
    _comment: CommentRecord,
    _review: ReviewRecord,
    _thread: ReviewThreadRecord,
    _label: PrLabelRecord,
    _assignee: PrAssigneeRecord,
    _reviewer: PrReviewerRecord,
) {
}

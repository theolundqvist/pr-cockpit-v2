use std::hash::{Hash, Hasher};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::Row;

use crate::db::Db;

use super::triggers::{NotificationCandidate, NotificationKind};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationEventRow {
    pub id: String,
    pub account_id: String,
    pub repo_id: Option<String>,
    pub pr_id: Option<String>,
    pub event_type: String,
    pub actor_id: String,
    pub server_event_id: String,
    pub title: String,
    pub body: String,
    pub fired_at: i64,
    pub deduped: bool,
    pub seen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct EventPageInput {
    pub account_id: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub since: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertResult {
    pub inserted: bool,
    pub event_id: String,
}

pub async fn insert_or_ignore(
    db: &Db,
    candidate: &NotificationCandidate,
    fired_at: i64,
) -> Result<InsertResult> {
    let event_id = event_id(candidate);
    let result = sqlx::query(
        "INSERT OR IGNORE INTO notification_events(
           id,
           account_id,
           repo_id,
           pr_id,
           event_type,
           actor_id,
           server_event_id,
           title,
           body,
           fired_at,
           deduped
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
    )
    .bind(&event_id)
    .bind(&candidate.account_id)
    .bind(&candidate.repo_id)
    .bind(&candidate.pr_id)
    .bind(candidate.kind.as_str())
    .bind(&candidate.actor_id)
    .bind(&candidate.server_event_id)
    .bind(&candidate.title)
    .bind(&candidate.body)
    .bind(fired_at)
    .execute(db.pool())
    .await?;
    Ok(InsertResult {
        inserted: result.rows_affected() == 1,
        event_id,
    })
}

pub async fn mark_suppressed(db: &Db, event_id: &str) -> Result<()> {
    sqlx::query("UPDATE notification_events SET deduped = 1 WHERE id = ?1")
        .bind(event_id)
        .execute(db.pool())
        .await?;
    Ok(())
}

pub async fn mark_seen(db: &Db, event_id: &str, seen_at: i64) -> Result<()> {
    sqlx::query("UPDATE notification_events SET seen_at = ?2 WHERE id = ?1")
        .bind(event_id)
        .bind(seen_at)
        .execute(db.pool())
        .await?;
    Ok(())
}

pub async fn list_events(db: &Db, input: EventPageInput) -> Result<Vec<NotificationEventRow>> {
    let limit = input.limit.unwrap_or(50).clamp(1, 200);
    let offset = input.offset.unwrap_or(0).max(0);
    let rows = sqlx::query(
        "SELECT
           id,
           account_id,
           repo_id,
           pr_id,
           event_type,
           actor_id,
           server_event_id,
           title,
           body,
           fired_at,
           deduped,
           seen_at
         FROM notification_events
         WHERE account_id = ?1
           AND (?2 IS NULL OR fired_at >= ?2)
         ORDER BY fired_at DESC, id DESC
         LIMIT ?3 OFFSET ?4",
    )
    .bind(&input.account_id)
    .bind(input.since)
    .bind(limit)
    .bind(offset)
    .fetch_all(db.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(NotificationEventRow {
                id: row.try_get("id")?,
                account_id: row.try_get("account_id")?,
                repo_id: row.try_get("repo_id")?,
                pr_id: row.try_get("pr_id")?,
                event_type: row.try_get("event_type")?,
                actor_id: row.try_get("actor_id")?,
                server_event_id: row.try_get("server_event_id")?,
                title: row.try_get("title")?,
                body: row.try_get("body")?,
                fired_at: row.try_get("fired_at")?,
                deduped: row.try_get::<i64, _>("deduped")? == 1,
                seen: row.try_get::<Option<i64>, _>("seen_at")?.is_some(),
            })
        })
        .collect()
}

pub fn event_type_from_kind(kind: NotificationKind) -> String {
    kind.as_str().to_string()
}

fn event_id(candidate: &NotificationCandidate) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    candidate.account_id.hash(&mut hasher);
    candidate.repo_id.hash(&mut hasher);
    candidate.pr_id.hash(&mut hasher);
    candidate.kind.as_str().hash(&mut hasher);
    candidate.actor_id.hash(&mut hasher);
    candidate.server_event_id.hash(&mut hasher);
    format!("notif-{:016x}", hasher.finish())
}

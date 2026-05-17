use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::patch::{
    bind_patch_value, build_where_clause, collect_row_columns, ensure_pk_in_after,
    required_pk_columns, sql_placeholders, Patch, PatchValue, RowMutation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    OptimisticPrediction { mutation_id: String },
    Rollback { mutation_id: String },
    ServerReconcile { mutation_id: String },
}

pub async fn apply_patch(
    tx: &mut Transaction<'_, Sqlite>,
    patch: &Patch,
    source: PatchSource,
) -> Result<()> {
    for operation in &patch.operations {
        apply_row_mutation(tx, operation, patch, &source).await?;
    }

    match source {
        PatchSource::Rollback { mutation_id } | PatchSource::ServerReconcile { mutation_id } => {
            clear_pending_state(tx, &mutation_id).await?;
        }
        PatchSource::OptimisticPrediction { .. } => {}
    }

    Ok(())
}

async fn apply_row_mutation(
    tx: &mut Transaction<'_, Sqlite>,
    operation: &RowMutation,
    patch: &Patch,
    source: &PatchSource,
) -> Result<()> {
    let table = sanitize_identifier(&operation.table)?;
    let pk_columns = required_pk_columns(&operation.pk)?;

    match &operation.after {
        Some(after) => {
            ensure_pk_in_after(&operation.pk, after)?;
            let row = with_pending_overlay(operation.table.as_str(), after, patch, source)?;
            upsert_row(tx, table.as_str(), &pk_columns, &row).await?;
        }
        None => {
            delete_row(tx, table.as_str(), &pk_columns, &operation.pk).await?;
        }
    }

    Ok(())
}

fn with_pending_overlay(
    table: &str,
    row: &BTreeMap<String, PatchValue>,
    patch: &Patch,
    source: &PatchSource,
) -> Result<BTreeMap<String, PatchValue>> {
    let mut row = row.clone();
    if !table_has_pending_state(table) {
        return Ok(row);
    }

    let PatchSource::OptimisticPrediction { mutation_id } = source else {
        return Ok(row);
    };

    let overlay_kind = patch
        .pending_overlay_kind
        .as_deref()
        .unwrap_or("full")
        .to_string();
    let overlay = json!({
        "mutation_id": mutation_id,
        "kind": overlay_kind,
    });
    row.insert("pending_state".to_string(), PatchValue::Json(overlay));
    Ok(row)
}

async fn upsert_row(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    pk_columns: &[String],
    row: &BTreeMap<String, PatchValue>,
) -> Result<()> {
    let columns = collect_row_columns(row);
    let placeholders = sql_placeholders(columns.len());
    let mut sql = format!(
        "INSERT INTO {table} ({}) VALUES ({placeholders})",
        columns.join(", ")
    );

    let pk_set = pk_columns.iter().cloned().collect::<HashSet<_>>();
    let update_columns = columns
        .iter()
        .filter(|column| !pk_set.contains(column.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if update_columns.is_empty() {
        sql.push_str(&format!(
            " ON CONFLICT ({}) DO NOTHING",
            pk_columns.join(", ")
        ));
    } else {
        let assignments = update_columns
            .iter()
            .map(|column| format!("{column} = excluded.{column}"))
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!(
            " ON CONFLICT ({}) DO UPDATE SET {assignments}",
            pk_columns.join(", ")
        ));
    }

    let mut query = sqlx::query(&sql);
    for column in &columns {
        let value = row
            .get(column)
            .with_context(|| format!("missing patch value for column `{column}`"))?;
        query = bind_patch_value(query, value)?;
    }
    query.execute(tx.as_mut()).await?;
    Ok(())
}

async fn delete_row(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    pk_columns: &[String],
    pk: &BTreeMap<String, PatchValue>,
) -> Result<()> {
    let where_clause = build_where_clause(pk_columns);
    let sql = format!("DELETE FROM {table} WHERE {where_clause}");
    let mut query = sqlx::query(&sql);
    for column in pk_columns {
        let value = pk
            .get(column)
            .with_context(|| format!("missing pk value for column `{column}`"))?;
        query = bind_patch_value(query, value)?;
    }
    query.execute(tx.as_mut()).await?;
    Ok(())
}

async fn clear_pending_state(tx: &mut Transaction<'_, Sqlite>, mutation_id: &str) -> Result<()> {
    for table in pending_state_tables() {
        if !table_exists(tx, table).await? {
            continue;
        }
        let sql = format!(
            "UPDATE {table} SET pending_state = NULL WHERE json_extract(pending_state, '$.mutation_id') = ?1"
        );
        sqlx::query(&sql)
            .bind(mutation_id)
            .execute(tx.as_mut())
            .await?;
    }
    Ok(())
}

async fn table_exists(tx: &mut Transaction<'_, Sqlite>, table: &str) -> Result<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(table)
    .fetch_one(tx.as_mut())
    .await?;
    Ok(exists > 0)
}

fn table_has_pending_state(table: &str) -> bool {
    pending_state_tables().contains(&table)
}

fn pending_state_tables() -> &'static [&'static str] {
    &[
        "pull_requests",
        "comments",
        "reviews",
        "review_threads",
        "pr_labels",
        "pr_assignees",
        "pr_reviewers",
        "pr_projects",
        "pr_milestones",
        "pr_files",
    ]
}

fn sanitize_identifier(value: &str) -> Result<String> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        anyhow::bail!("invalid sql identifier `{value}` in patch")
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Result;
    use sqlx::Row;

    use super::*;

    #[tokio::test]
    async fn apply_forward_then_inverse_is_noop_for_single_row() -> Result<()> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query(
            "CREATE TABLE comments (
                id TEXT PRIMARY KEY,
                body TEXT NOT NULL,
                pending_state TEXT,
                body_server_adjusted INTEGER NOT NULL DEFAULT 0,
                server_adjusted_at INTEGER
             )",
        )
        .execute(&pool)
        .await?;

        let mut after = BTreeMap::new();
        after.insert("id".to_string(), PatchValue::from("comment-1"));
        after.insert("body".to_string(), PatchValue::from("body"));

        let patch = Patch {
            operations: vec![RowMutation {
                table: "comments".to_string(),
                pk: BTreeMap::from([(String::from("id"), PatchValue::from("comment-1"))]),
                before: None,
                after: Some(after),
            }],
            pending_overlay_kind: Some("full".to_string()),
        };

        let inverse = patch.inverse();

        let mut tx = pool.begin().await?;
        apply_patch(
            &mut tx,
            &patch,
            PatchSource::OptimisticPrediction {
                mutation_id: "m1".to_string(),
            },
        )
        .await?;
        apply_patch(
            &mut tx,
            &inverse,
            PatchSource::Rollback {
                mutation_id: "m1".to_string(),
            },
        )
        .await?;
        tx.commit().await?;

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM comments")
            .fetch_one(&pool)
            .await?;
        assert_eq!(count, 0);

        let pending_rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE pending_state IS NOT NULL")
                .fetch_one(&pool)
                .await?;
        assert_eq!(pending_rows, 0);

        Ok(())
    }

    #[tokio::test]
    async fn optimistic_prediction_stamps_pending_overlay() -> Result<()> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query(
            "CREATE TABLE pull_requests (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                pending_state TEXT,
                body_server_adjusted INTEGER NOT NULL DEFAULT 0,
                server_adjusted_at INTEGER
             )",
        )
        .execute(&pool)
        .await?;

        let mut row = BTreeMap::new();
        row.insert("id".to_string(), PatchValue::from("pr-1"));
        row.insert("title".to_string(), PatchValue::from("Draft title"));
        let patch = Patch {
            operations: vec![RowMutation {
                table: "pull_requests".to_string(),
                pk: BTreeMap::from([(String::from("id"), PatchValue::from("pr-1"))]),
                before: None,
                after: Some(row),
            }],
            pending_overlay_kind: Some("cautious".to_string()),
        };

        let mut tx = pool.begin().await?;
        apply_patch(
            &mut tx,
            &patch,
            PatchSource::OptimisticPrediction {
                mutation_id: "mut-7".to_string(),
            },
        )
        .await?;
        tx.commit().await?;

        let overlay = sqlx::query("SELECT pending_state FROM pull_requests WHERE id = 'pr-1'")
            .fetch_one(&pool)
            .await?
            .try_get::<Option<String>, _>("pending_state")?;
        assert!(overlay
            .as_deref()
            .is_some_and(|value| value.contains("\"mutation_id\":\"mut-7\"")));
        assert!(overlay
            .as_deref()
            .is_some_and(|value| value.contains("\"kind\":\"cautious\"")));

        Ok(())
    }
}

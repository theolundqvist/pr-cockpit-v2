use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteArguments;
use sqlx::{query::Query, Sqlite};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Patch {
    #[serde(default)]
    pub operations: Vec<RowMutation>,
    #[serde(default)]
    pub pending_overlay_kind: Option<String>,
}

impl Patch {
    pub fn empty() -> Self {
        Self {
            operations: Vec::new(),
            pending_overlay_kind: None,
        }
    }

    pub fn inverse(&self) -> Self {
        let mut operations = self.operations.clone();
        operations.reverse();
        for operation in &mut operations {
            std::mem::swap(&mut operation.before, &mut operation.after);
        }
        Self {
            operations,
            pending_overlay_kind: None,
        }
    }

    pub fn merge_inverses(forward: &Patch) -> Patch {
        forward.inverse()
    }

    pub fn with_overlay_kind(mut self, kind: impl Into<String>) -> Self {
        self.pending_overlay_kind = Some(kind.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RowMutation {
    pub table: String,
    pub pk: BTreeMap<String, PatchValue>,
    pub before: Option<BTreeMap<String, PatchValue>>,
    pub after: Option<BTreeMap<String, PatchValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PatchValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Bool(bool),
    Json(serde_json::Value),
}

impl From<&str> for PatchValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

impl From<String> for PatchValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<i64> for PatchValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<bool> for PatchValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl PatchValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

pub fn bind_patch_value<'q>(
    query: Query<'q, Sqlite, SqliteArguments<'q>>,
    value: &'q PatchValue,
) -> Result<Query<'q, Sqlite, SqliteArguments<'q>>> {
    let query = match value {
        PatchValue::Null => query.bind(Option::<String>::None),
        PatchValue::Integer(value) => query.bind(*value),
        PatchValue::Real(value) => query.bind(*value),
        PatchValue::Text(value) => query.bind(value.clone()),
        PatchValue::Bool(value) => query.bind(if *value { 1_i64 } else { 0_i64 }),
        PatchValue::Json(value) => query.bind(serde_json::to_string(value)?),
    };
    Ok(query)
}

pub fn required_pk_columns(pk: &BTreeMap<String, PatchValue>) -> Result<Vec<String>> {
    if pk.is_empty() {
        bail!("patch row mutation must include at least one pk column");
    }
    Ok(pk.keys().cloned().collect())
}

pub fn collect_row_columns(row: &BTreeMap<String, PatchValue>) -> Vec<String> {
    row.keys().cloned().collect()
}

pub fn sql_placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

pub fn build_where_clause(columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| format!("{column} = ?"))
        .collect::<Vec<_>>()
        .join(" AND ")
}

pub fn ensure_pk_in_after(
    pk: &BTreeMap<String, PatchValue>,
    after: &BTreeMap<String, PatchValue>,
) -> Result<()> {
    for (column, pk_value) in pk {
        let after_value = after
            .get(column)
            .with_context(|| format!("missing pk column `{column}` in patch after-row"))?;
        if after_value != pk_value {
            bail!("pk column `{column}` changed inside patch row mutation");
        }
    }
    Ok(())
}

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::Row;

use crate::db::Db;

use super::triggers::NotificationKind;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationRule {
    pub id: String,
    pub account_id: String,
    pub kind: String,
    pub enabled: bool,
    pub config_json: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
pub struct QuietHoursConfig {
    pub start: String,
    pub end: String,
    pub tz: String,
    pub days: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
pub struct RepoFilters {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct NotificationSettings {
    pub account_id: String,
    pub quiet_hours_json: Option<String>,
    pub focus_mode: bool,
    pub per_repo_filters_json: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveSettings {
    pub quiet_hours: Option<QuietHoursConfig>,
    pub focus_mode: bool,
    pub repo_filters: RepoFilters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionReason {
    FocusMode,
    QuietHours,
    RepoFilter,
}

pub async fn ensure_defaults(db: &Db, account_id: &str) -> Result<()> {
    let now = now_epoch_seconds()?;
    sqlx::query(
        "INSERT OR IGNORE INTO notification_settings(
           account_id,
           quiet_hours_json,
           focus_mode,
           per_repo_filters_json,
           updated_at
         )
         VALUES (?1, NULL, 0, '{\"allow\":[],\"deny\":[]}', ?2)",
    )
    .bind(account_id)
    .bind(now)
    .execute(db.pool())
    .await?;

    for kind in NotificationKind::all() {
        let kind_str = kind.as_str();
        let rule_id = format!("{account_id}:{kind_str}");
        sqlx::query(
            "INSERT OR IGNORE INTO notification_rules(
               id,
               account_id,
               kind,
               enabled,
               config_json,
               updated_at
             )
             VALUES (?1, ?2, ?3, 1, '{}', ?4)",
        )
        .bind(rule_id)
        .bind(account_id)
        .bind(kind_str)
        .bind(now)
        .execute(db.pool())
        .await?;
    }
    Ok(())
}

pub async fn list_rules(db: &Db, account_id: &str) -> Result<Vec<NotificationRule>> {
    ensure_defaults(db, account_id).await?;
    let rows = sqlx::query(
        "SELECT id, account_id, kind, enabled, config_json, updated_at
         FROM notification_rules
         WHERE account_id = ?1
         ORDER BY kind ASC",
    )
    .bind(account_id)
    .fetch_all(db.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(NotificationRule {
                id: row.try_get("id")?,
                account_id: row.try_get("account_id")?,
                kind: row.try_get("kind")?,
                enabled: row.try_get::<i64, _>("enabled")? == 1,
                config_json: row.try_get("config_json")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

pub async fn set_rule(
    db: &Db,
    account_id: &str,
    kind: NotificationKind,
    enabled: bool,
    config_json: &str,
) -> Result<()> {
    ensure_defaults(db, account_id).await?;
    let now = now_epoch_seconds()?;
    let kind_str = kind.as_str();
    let rule_id = format!("{account_id}:{kind_str}");
    sqlx::query(
        "INSERT INTO notification_rules(id, account_id, kind, enabled, config_json, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(account_id, kind) DO UPDATE SET
           enabled = excluded.enabled,
           config_json = excluded.config_json,
           updated_at = excluded.updated_at",
    )
    .bind(rule_id)
    .bind(account_id)
    .bind(kind_str)
    .bind(if enabled { 1_i64 } else { 0_i64 })
    .bind(config_json)
    .bind(now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn set_quiet_hours(
    db: &Db,
    account_id: &str,
    quiet_hours_json: Option<&str>,
) -> Result<()> {
    ensure_defaults(db, account_id).await?;
    let now = now_epoch_seconds()?;
    sqlx::query(
        "UPDATE notification_settings
         SET quiet_hours_json = ?2, updated_at = ?3
         WHERE account_id = ?1",
    )
    .bind(account_id)
    .bind(quiet_hours_json)
    .bind(now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn set_focus_mode(db: &Db, account_id: &str, on: bool) -> Result<()> {
    ensure_defaults(db, account_id).await?;
    let now = now_epoch_seconds()?;
    sqlx::query(
        "UPDATE notification_settings
         SET focus_mode = ?2, updated_at = ?3
         WHERE account_id = ?1",
    )
    .bind(account_id)
    .bind(if on { 1_i64 } else { 0_i64 })
    .bind(now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn set_per_repo_filters(
    db: &Db,
    account_id: &str,
    allow: &[String],
    deny: &[String],
) -> Result<()> {
    ensure_defaults(db, account_id).await?;
    let now = now_epoch_seconds()?;
    let payload = serde_json::to_string(&RepoFilters {
        allow: allow.to_vec(),
        deny: deny.to_vec(),
    })?;
    sqlx::query(
        "UPDATE notification_settings
         SET per_repo_filters_json = ?2, updated_at = ?3
         WHERE account_id = ?1",
    )
    .bind(account_id)
    .bind(payload)
    .bind(now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn load_settings(db: &Db, account_id: &str) -> Result<NotificationSettings> {
    ensure_defaults(db, account_id).await?;
    let row = sqlx::query(
        "SELECT account_id, quiet_hours_json, focus_mode, per_repo_filters_json, updated_at
         FROM notification_settings
         WHERE account_id = ?1
         LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(db.pool())
    .await?;
    Ok(NotificationSettings {
        account_id: row.try_get("account_id")?,
        quiet_hours_json: row.try_get("quiet_hours_json")?,
        focus_mode: row.try_get::<i64, _>("focus_mode")? == 1,
        per_repo_filters_json: row.try_get("per_repo_filters_json")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn load_effective_settings(db: &Db, account_id: &str) -> Result<EffectiveSettings> {
    let settings = load_settings(db, account_id).await?;
    let quiet_hours = settings
        .quiet_hours_json
        .as_deref()
        .map(serde_json::from_str::<QuietHoursConfig>)
        .transpose()
        .context("parsing quiet hours json")?;
    let repo_filters = serde_json::from_str::<RepoFilters>(&settings.per_repo_filters_json)
        .context("parsing repo filters json")?;
    Ok(EffectiveSettings {
        quiet_hours,
        focus_mode: settings.focus_mode,
        repo_filters,
    })
}

pub async fn rule_enabled(db: &Db, account_id: &str, kind: NotificationKind) -> Result<bool> {
    ensure_defaults(db, account_id).await?;
    let enabled = sqlx::query_scalar::<_, i64>(
        "SELECT enabled
         FROM notification_rules
         WHERE account_id = ?1 AND kind = ?2
         LIMIT 1",
    )
    .bind(account_id)
    .bind(kind.as_str())
    .fetch_optional(db.pool())
    .await?
    .unwrap_or(1_i64);
    Ok(enabled == 1)
}

pub fn suppression_reason(
    settings: &EffectiveSettings,
    repo_full_name: Option<&str>,
    now_utc: chrono::DateTime<Utc>,
) -> Option<SuppressionReason> {
    if settings.focus_mode {
        return Some(SuppressionReason::FocusMode);
    }
    if should_suppress_repo(repo_full_name, &settings.repo_filters) {
        return Some(SuppressionReason::RepoFilter);
    }
    if in_quiet_hours(settings.quiet_hours.as_ref(), now_utc) {
        return Some(SuppressionReason::QuietHours);
    }
    None
}

fn should_suppress_repo(repo_full_name: Option<&str>, filters: &RepoFilters) -> bool {
    let Some(repo_full_name) = repo_full_name else {
        return false;
    };
    if filters
        .deny
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(repo_full_name))
    {
        return true;
    }
    if filters.allow.is_empty() {
        return false;
    }
    !filters
        .allow
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(repo_full_name))
}

fn in_quiet_hours(config: Option<&QuietHoursConfig>, now_utc: chrono::DateTime<Utc>) -> bool {
    let Some(config) = config else {
        return false;
    };
    if config.days.is_empty() {
        return false;
    }
    let Ok(tz) = config.tz.parse::<Tz>() else {
        return false;
    };
    let start = parse_time(&config.start);
    let end = parse_time(&config.end);
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };

    let local = tz.from_utc_datetime(&now_utc.naive_utc());
    let now = NaiveTime::from_hms_opt(local.hour(), local.minute(), 0);
    let Some(now) = now else {
        return false;
    };
    let day = u8::try_from(local.weekday().num_days_from_sunday()).unwrap_or(0);

    if start <= end {
        config.days.contains(&day) && now >= start && now < end
    } else if now >= start {
        config.days.contains(&day)
    } else {
        let prev_day = (day + 6) % 7;
        config.days.contains(&prev_day) && now < end
    }
}

fn parse_time(raw: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(raw, "%H:%M").ok()
}

fn now_epoch_seconds() -> Result<i64> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before unix epoch")?;
    i64::try_from(elapsed.as_secs()).context("unix timestamp exceeds i64")
}

use std::sync::{Arc, Mutex};

use anyhow::Result;
use desktop_lib::api::{ApiResource, RateLimitSnapshot};
use desktop_lib::db::Db;
use desktop_lib::sync::{
    CacheInvalidationEmitter, Priority, RateLimitBudgetSnapshot, RateLimitBudgeter,
};

#[tokio::test]
async fn budgeter_scopes_throttle_by_account_and_emits_pressure_and_bypass() -> Result<()> {
    let temp = tempfile::TempDir::new()?;
    let db = Arc::new(Db::open(temp.path()).await?);
    let account_a = db
        .upsert_auth_account("github.com", "account-a", "pat", "repo", 1)
        .await?;
    let account_b = db
        .upsert_auth_account("github.com", "account-b", "pat", "repo", 1)
        .await?;

    let emitter = Arc::new(CaptureEmitter::default());
    let budgeter = RateLimitBudgeter::start_with_emitter(Arc::clone(&db), emitter.clone());

    budgeter
        .record(
            &account_a.id,
            ApiResource::Graphql,
            RateLimitSnapshot {
                remaining: 100,
                limit_total: 5_000,
                used: Some(4_900),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    budgeter
        .record(
            &account_a.id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 5_000,
                limit_total: 5_000,
                used: Some(0),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    budgeter
        .record(
            &account_b.id,
            ApiResource::Graphql,
            RateLimitSnapshot {
                remaining: 5_000,
                limit_total: 5_000,
                used: Some(0),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;
    budgeter
        .record(
            &account_b.id,
            ApiResource::Core,
            RateLimitSnapshot {
                remaining: 5_000,
                limit_total: 5_000,
                used: Some(0),
                reset_at_epoch: 9_999_999_999,
            },
        )
        .await?;

    assert!(!budgeter.allow(&account_a.id, Priority::Background).await?);
    assert!(budgeter.allow(&account_b.id, Priority::Background).await?);
    assert!(budgeter.allow(&account_a.id, Priority::Foreground).await?);

    let pressure_events = emitter.pressure_events();
    assert_eq!(pressure_events.len(), 1);
    assert_eq!(pressure_events[0].0, account_a.id);
    assert!(
        pressure_events[0]
            .1
            .iter()
            .any(|row| row.account_id == account_b.id),
        "pressure snapshot should include all account rows for UI meter updates"
    );

    let bypass_events = emitter.bypass_events();
    assert_eq!(bypass_events.len(), 1);
    assert_eq!(bypass_events[0].0, account_a.id);

    assert!(
        pressure_events
            .iter()
            .all(|(account_id, _)| account_id != &account_b.id),
        "background allowance for account B must not emit pressure"
    );
    assert!(
        bypass_events
            .iter()
            .all(|(account_id, _)| account_id != &account_b.id),
        "foreground bypass should only emit for account A"
    );

    Ok(())
}

#[derive(Default)]
struct CaptureEmitter {
    pressure: Mutex<Vec<(String, Vec<RateLimitBudgetSnapshot>)>>,
    bypass: Mutex<Vec<(String, Vec<RateLimitBudgetSnapshot>)>>,
}

impl CaptureEmitter {
    fn pressure_events(&self) -> Vec<(String, Vec<RateLimitBudgetSnapshot>)> {
        self.pressure
            .lock()
            .expect("pressure lock poisoned")
            .clone()
    }

    fn bypass_events(&self) -> Vec<(String, Vec<RateLimitBudgetSnapshot>)> {
        self.bypass.lock().expect("bypass lock poisoned").clone()
    }
}

impl CacheInvalidationEmitter for CaptureEmitter {
    fn emit_rate_limit_pressure(&self, account_id: &str, snapshot: &[RateLimitBudgetSnapshot]) {
        self.pressure
            .lock()
            .expect("pressure lock poisoned")
            .push((account_id.to_string(), snapshot.to_vec()));
    }

    fn emit_rate_limit_bypass(&self, account_id: &str, snapshot: &[RateLimitBudgetSnapshot]) {
        self.bypass
            .lock()
            .expect("bypass lock poisoned")
            .push((account_id.to_string(), snapshot.to_vec()));
    }
}

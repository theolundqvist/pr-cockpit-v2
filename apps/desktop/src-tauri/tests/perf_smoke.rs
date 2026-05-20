use std::time::{Duration, Instant};

use anyhow::Result;
use desktop_lib::db::Db;

const ACCOUNT_ID: &str = "acct_demo";

#[tokio::test]
async fn inbox_cold_paint_fixture_budget_smoke() -> Result<()> {
    let started = Instant::now();
    let db = Db::open_fixture().await?;
    let rows = db.list_inbox(ACCOUNT_ID).await?;
    let elapsed = started.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1_000.0;

    println!("## Measurements");
    println!("- inbox_cold_paint_ms: {:.2} <= 250.00", elapsed_ms);
    println!("- inbox_row_count: {} == 200", rows.len());

    assert_eq!(rows.len(), 200, "fixture should keep 200 inbox rows");
    assert!(
        elapsed < Duration::from_millis(250),
        "fixture inbox cold paint exceeded 250ms: {:.2}ms",
        elapsed_ms
    );

    Ok(())
}

use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use desktop_lib::db::Db;
use tokio::runtime::Runtime;

const ACCOUNT_ID: &str = "acct_demo";

fn inbox_first_paint(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    c.bench_function("inbox_first_paint", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let started = Instant::now();
                runtime.block_on(async {
                    let db = Db::open_fixture().await.expect("open fixture db");
                    let rows = db.list_inbox(ACCOUNT_ID).await.expect("list inbox");
                    assert_eq!(rows.len(), 200, "fixture should keep 200 inbox rows");
                });
                total += started.elapsed();
            }
            total
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(2));
    targets = inbox_first_paint
}
criterion_main!(benches);

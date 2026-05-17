use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use desktop_lib::db::Db;
use tokio::runtime::Runtime;

const ACCOUNT_ID: &str = "acct_demo";
const PR_ID: &str = "pr_1";

fn pr_detail_open(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let db = runtime.block_on(Db::open_fixture()).expect("open fixture");

    c.bench_function("pr_detail_open/preloaded", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let started = Instant::now();
                runtime.block_on(async {
                    let summary = db
                        .pr_detail_summary(ACCOUNT_ID, PR_ID)
                        .await
                        .expect("summary query")
                        .expect("pr summary in fixture");
                    let files = db
                        .pr_files(ACCOUNT_ID, PR_ID, &summary.head_sha)
                        .await
                        .expect("file query");
                    let patch = db
                        .pr_patch(ACCOUNT_ID, PR_ID, &summary.head_sha)
                        .await
                        .expect("patch query")
                        .expect("fixture patch");
                    black_box((summary, files.len(), patch.len()));
                });
                total += started.elapsed();
            }
            total
        });
    });

    c.bench_function("pr_detail_open/cold_cache", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let started = Instant::now();
                runtime.block_on(async {
                    let db = Db::open_fixture().await.expect("open fixture");
                    let summary = db
                        .pr_detail_summary(ACCOUNT_ID, PR_ID)
                        .await
                        .expect("summary query")
                        .expect("pr summary in fixture");
                    let files = db
                        .pr_files(ACCOUNT_ID, PR_ID, &summary.head_sha)
                        .await
                        .expect("file query");
                    let patch = db
                        .pr_patch(ACCOUNT_ID, PR_ID, &summary.head_sha)
                        .await
                        .expect("patch query")
                        .expect("fixture patch");
                    black_box((summary, files.len(), patch.len()));
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
    targets = pr_detail_open
}
criterion_main!(benches);

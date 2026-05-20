use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use desktop_lib::db::Db;
use tokio::runtime::Runtime;

const ACCOUNT_ID: &str = "acct_demo";
const PR_ID: &str = "pr_1";
const TARGET_FILE: &str = "src/generated/huge_fixture.rs";

fn file_open_in_diff_cached(c: &mut Criterion) {
    let runtime = Runtime::new().expect("runtime");
    let db = runtime.block_on(Db::open_fixture()).expect("open fixture");
    let head_sha = runtime.block_on(async {
        db.pr_detail_summary(ACCOUNT_ID, PR_ID)
            .await
            .expect("summary query")
            .expect("fixture summary")
            .head_sha
    });

    runtime.block_on(async {
        let _ = db
            .pr_patch(ACCOUNT_ID, PR_ID, &head_sha)
            .await
            .expect("warm patch query")
            .expect("fixture patch");
    });

    c.bench_function("file_open_in_diff_cached", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let started = Instant::now();
                runtime.block_on(async {
                    let files = db
                        .pr_files(ACCOUNT_ID, PR_ID, &head_sha)
                        .await
                        .expect("file query");
                    let target = files
                        .iter()
                        .find(|file| file.path == TARGET_FILE)
                        .expect("fixture target file");
                    let patch = db
                        .pr_patch(ACCOUNT_ID, PR_ID, &head_sha)
                        .await
                        .expect("patch query")
                        .expect("fixture patch");
                    let patch_text = String::from_utf8(patch).expect("utf8 patch");
                    let hit = patch_text.contains(&format!("+++ b/{}", target.path));
                    black_box((target.additions, hit));
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
    targets = file_open_in_diff_cached
}
criterion_main!(benches);

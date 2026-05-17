use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};

fn placeholder_bench(c: &mut Criterion) {
    c.bench_function("scaffold_noop", |b| {
        b.iter(|| {
            black_box(1usize + 1usize);
        })
    });
}

criterion_group!(benches, placeholder_bench);
criterion_main!(benches);

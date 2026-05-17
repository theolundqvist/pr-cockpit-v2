use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use desktop_lib::render::{render_comment, RenderCtx};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct CorpusEntry {
    body: String,
    repo: Option<String>,
}

fn load_corpus() -> Vec<CorpusEntry> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let corpus_path = root.join("../../tools/markdown-corpus/corpus.json");
    let raw = fs::read_to_string(&corpus_path).unwrap_or_default();
    if raw.is_empty() {
        return fallback_corpus();
    }
    serde_json::from_str::<Vec<CorpusEntry>>(&raw).unwrap_or_else(|_| fallback_corpus())
}

fn fallback_corpus() -> Vec<CorpusEntry> {
    vec![
        CorpusEntry {
            body: "cc @octocat closes #7 :rocket:".to_string(),
            repo: Some("cli/cli".to_string()),
        },
        CorpusEntry {
            body: "> [!WARNING]\n> Do not merge without checks".to_string(),
            repo: None,
        },
        CorpusEntry {
            body: "```suggestion\nlet ok = true;\n```".to_string(),
            repo: None,
        },
    ]
}

fn comrak_render_throughput(c: &mut Criterion) {
    let entries = load_corpus();
    let mut index = 0_usize;

    let mut group = c.benchmark_group("comrak_render_throughput");
    group.throughput(Throughput::Elements(1));
    group.bench_function("comrak_render_throughput", |b| {
        b.iter(|| {
            let entry = &entries[index % entries.len()];
            index += 1;
            let rendered = render_comment(
                &entry.body,
                &RenderCtx {
                    repo: entry.repo.as_deref(),
                    cache: None,
                },
            );
            black_box(rendered.html.len());
        });
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(3));
    targets = comrak_render_throughput
}
criterion_main!(benches);

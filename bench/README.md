# Performance and corpus bench harness

This directory contains the M1 perf budgets (`bench/budgets.json`) and output artifacts (`bench/results/*.json`).

## What runs in `pnpm bench`

`pnpm bench` executes `tools/perf-bench/run-all.mjs` in this order:

1. `pnpm --filter desktop build`
2. Rust Criterion benches (`tools/perf-bench/run-rust-benches.mjs`)
3. Frontend Playwright perf harness (`tools/perf-bench/run-frontend-bench.mjs`)
4. Budget gate (`tools/perf-bench/compare-budgets.mjs`)

A run fails if any hard budget in `bench/budgets.json` is violated, or if a metric regresses more than the global `tolerance_pct` (10%) versus its baseline.

## Rust benches

Criterion benches live under `apps/desktop/src-tauri/benches/`:

- `inbox_first_paint`
- `pr_detail_open` (`preloaded` and `cold_cache`)
- `file_open_in_diff_cached`
- `comrak_render_throughput`

Run them directly:

```bash
cargo bench -p desktop --bench inbox_first_paint --bench pr_detail_open --bench file_open_in_diff_cached --bench comrak_render_throughput -- --noplot
```

Criterion JSON estimates are read from `target/criterion/**/new/estimates.json` and normalized into `bench/results/rust.json`.

## Frontend harness

Run only the frontend harness:

```bash
node tools/perf-bench/run-frontend-bench.mjs
```

Environment knobs:

- `PERF_BROWSER=webkit|chromium|firefox` (default `webkit`)
- `PERF_TARGET=preview|tauri` (current CI path uses `preview`)
- `PERF_BASE_URL=http://127.0.0.1:4173`

The harness records:

- Inbox first paint (`performance` paint entries)
- PR detail open preloaded
- PR detail open cold cache
- File open in diff cached
- 5k-line diff scroll FPS and frame p95

Artifacts:

- `bench/results/frontend.json`
- `bench/results/frontend-perf.png`
- `bench/results/chromium-trace.json` (when `PERF_BROWSER=chromium`)

## Corpus gate

`pnpm corpus` runs the markdown corpus scorer:

```bash
node tools/markdown-corpus/score.mjs
```

It fails when weighted corpus diff exceeds `0.02` (2%).

Optional online refresh stays non-gating and requires network access:

```bash
pnpm corpus:fetch
```

## Refreshing budgets

1. Run `pnpm bench` on the CI-like runner class.
2. Inspect `bench/results/rust.json` and `bench/results/frontend.json`.
3. Update each metric `baseline` in `bench/budgets.json` to the new stable value.
4. Keep hard `budget` values aligned with PLAN §10; do not raise them.

## Interpreting regressions

When budget comparison fails, the script prints each offending metric with either:

- hard budget violation (`value > budget` for max metrics, `value < budget` for min metrics), or
- 10% regression violation against baseline.

A passing run prints one line per metric with the measured value and budget comparator.

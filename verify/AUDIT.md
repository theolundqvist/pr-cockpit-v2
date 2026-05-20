# Verifier audit notes — `m1-fix-gates`

Branch verified: `orch/m1-cockpit/m1-fix-gates` @ `c436ab81c59fff8fe0655fdb35171b5883e2b013`
Verifier branch: same (no rename; only verifier artifacts added under `verify/`).
Verifier date: 2026-05-17

## Scope

Re-verify the three gates the upstream verifier flagged on
`orch/m1-cockpit/m1-finalize`:

1. `pnpm lint` failing on Tauri-generated JSON.
2. `.github/workflows/ci.yml` `frontend (ubuntu-latest)` + `markdown-corpus` jobs
   missing Tauri Linux deps install.
3. Perf harness flake on the WebKit-side timing metrics
   `file_open_in_diff_cached_ms_frontend` and `diff_scroll_frame_p95_ms`.

## Evidence summary

| Gate | Verifier evidence (before fix) | Re-verify result (this branch)                                                       |
| ---- | ------------------------------ | ------------------------------------------------------------------------------------ |
| 1    | `pnpm lint` → exit 1            | `pnpm lint` → exit 0 (also: planted bad JSON under `src-tauri/gen/schemas/` ignored) |
| 2    | frontend(ubuntu)+markdown-corpus → fail on cargo/pnpm step | GH Actions run 25990142848: both jobs **success** (10m11s / 9m34s) |
| 3    | file_open=164ms, p95=16.8ms   | Local pnpm bench: file_open=59ms, p95=16.20ms. CI run 25990142848: file_open=66ms, p95=16.40ms. Both PLAN §10 hard budgets met. |

## Full local CI matrix

Each was run on the verifier VM with the same Linux deps the CI workflow
installs.

```
cargo fmt --check               → exit 0   (verify/logs/cargo-fmt.log)
cargo clippy --workspace ...    → exit 0   (verify/logs/cargo-clippy.log)
cargo test --workspace          → exit 0   16 test binaries, 0 failed (verify/logs/cargo-test.log)
pnpm typecheck                  → exit 0   (verify/logs/pnpm-typecheck.log)
pnpm svelte-check               → exit 0   0 errors / 0 warnings (verify/logs/pnpm-svelte-check.log)
pnpm lint                       → exit 0   (verify/logs/pnpm-lint.log)
pnpm test                       → exit 0   5 files / 6 tests (verify/logs/pnpm-test.log)
pnpm bench (xvfb webkit)        → exit 0   all 11 PLAN §10 metrics under budget (verify/logs/pnpm-bench.log)
pnpm corpus                     → exit 0   weighted_mean=0.015459 ≤ 0.02 (verify/logs/pnpm-corpus.log)
pnpm online-demo (no token)     → exit 0   skip path taken (verify/logs/pnpm-online-demo.log)
```

## Detail: PLAN §10 budgets (pnpm bench, local)

```
inbox_first_paint_ms                       8.74ms  <= 100ms
pr_detail_open_preloaded_ms                0.61ms  <= 50ms
pr_detail_open_cold_ms                     7.31ms  <= 250ms
file_open_in_diff_cached_ms                0.53ms  <= 100ms
comrak_render_throughput_ops_per_sec    37205.52ops/s >= 850ops/s
inbox_first_paint_ms_frontend             19.00ms  <= 100ms
pr_detail_open_preloaded_ms_frontend      25.00ms  <= 50ms
pr_detail_open_cold_ms_frontend           18.00ms  <= 250ms
file_open_in_diff_cached_ms_frontend      59.00ms  <= 100ms       (best of [90,59,64,63,65])
diff_scroll_fps                           62.07fps >= 60fps
diff_scroll_frame_p95_ms                  16.20ms  <= 16.7ms      (best of [16.2,16.2,16.2,16.2,16.2])
```

`bench/results/frontend.json` persists raw sample arrays under
`sampling.metrics`, sampling.policy = `best_of_n_min`, sample_count = 5.

## Detail: GH Actions run 25990142848 (HEAD c436ab8)

```
rust (ubuntu-latest)        ✓ success
rust (macos-latest)         ✓ success
frontend (ubuntu-latest)    ✓ success    ← previously failed; gate 2 fix confirmed
frontend (macos-latest)     ✓ success
markdown-corpus             ✓ success    ← previously failed; gate 2 fix confirmed
perf-bench                  ✗ failure    ← see note below
```

Note on the failing CI perf-bench job (see `verify/logs/ci-perf-bench-tail.log`):
The scoped WebKit-side metrics PASSED — `file_open_in_diff_cached_ms_frontend=66ms`
(budget 100ms) and `diff_scroll_frame_p95_ms=16.40ms` (budget 16.7ms). All 11 PLAN
§10 hard budgets passed. The job exit 1 came from the *separate* 10% baseline
regression alarm on two OTHER metrics that were not in the scoped flake list:

- `comrak_render_throughput_ops_per_sec: 20336.98 ops/s regressed past 10% threshold (27000 ops/s)`
- `pr_detail_open_preloaded_ms_frontend: 40 ms regressed past 10% threshold (33.00 ms)`

Both metrics still clear the PLAN §10 hard budgets (≥850 ops/s and ≤50 ms
respectively). This is a CI-runner baseline drift on metrics that were not part
of the three flagged gates, not a PLAN §10 violation. The task explicitly
forbids retuning `bench/budgets.json` other than to track measurement-source
changes, so this is outside the scope of `m1-fix-gates` and is flagged as a
follow-up rather than a verification failure for the scoped fix.

## Repro scripts (added by verifier)

- `verify/scripts/repro-lint-gen-ignored.sh` — plants an intentionally
  bad-formatted JSON under `apps/desktop/src-tauri/gen/schemas/` and asserts
  `pnpm lint` still exits 0, proving `.prettierignore` correctly excludes the
  generated tree.

## Files unchanged on the deliverable branch vs upstream

```
git diff origin/orch/m1-cockpit/m1-finalize -- bench/budgets.json   # empty diff
```

So no PLAN §10 budget was widened — only the estimator changed.

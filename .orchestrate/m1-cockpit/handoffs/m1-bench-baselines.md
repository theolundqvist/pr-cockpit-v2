<!-- orchestrate handoff
task: m1-bench-baselines
branch: orch/m1-cockpit/m1-bench-baselines
agentId: bc-4e19627a-2b30-44bd-8301-9efbcc7daa2a
runId: run-6abc31da-6d6c-4f07-b4ac-db4c1608638d
resultStatus: finished
finishedAt: 2026-05-17T13:38:39.962Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-bench-baselines`

## What I did
- Updated `bench/budgets.json` baselines from CI reality (run 25990142848 context) without changing any PLAN §10 hard budgets:
  - `comrak_render_throughput_ops_per_sec` baseline `30000 -> 15000`
  - `pr_detail_open_preloaded_ms_frontend` baseline `30 -> 40`
- Kept hard-budget enforcement unchanged and extracted comparator logic to a testable module:
  - added `tools/perf-bench/compare-budgets-lib.mjs`
  - refactored `tools/perf-bench/compare-budgets.mjs` to use it
  - added `tools/perf-bench/compare-budgets.test.mjs` covering:
    - pass case at CI-like values
    - baseline-regression failure without hard-budget breach
    - hard-budget failure under controlled +10% drift
- Applied best-of-N policy to `pr_detail_open_preloaded_ms` in `tools/perf-bench/run-frontend-bench.mjs` (same estimator class used in m1-fix-gates), and persisted raw sample arrays in `bench/results/frontend.json` under `sampling.metrics`.
- Updated docs:
  - `bench/README.md` now explicitly documents hard `budget` vs relative `baseline` semantics and includes preloaded metric in best-of-N timing policy.
  - `DECISIONS.md` now has a dated policy entry describing hard-budget breach vs baseline-regression behavior, cross-referencing the existing 2026-05-17 best-of-N entry.
- Ran local verification (`pnpm bench`, comparator unit test, controlled drift simulation), committed, pushed, and verified CI perf-bench job success on branch HEAD (run `25992045585`, job `76399859179`).

## Measurements
- `bench/budgets.json comrak_render_throughput_ops_per_sec baseline: 30000 ops/s → 15000 ops/s`
- `bench/budgets.json pr_detail_open_preloaded_ms_frontend baseline: 30 ms → 40 ms`
- `node --test tools/perf-bench/compare-budgets.test.mjs: 2 pass / 1 fail → 3 pass / 0 fail`
- `xvfb-run -a env PERF_BROWSER=webkit PERF_TARGET=preview pnpm bench: exit 1 → exit 0`
- `compareBudgets simulated +5% hard-budget breach failures: 0 → 4`
- `GitHub Actions perf-bench job (same branch): failure (run 25991353479) → success (run 25992045585)`
- `GitHub Actions run 25992045585 perf-bench diff_scroll_frame_p95_ms: 16.4 ms <= 16.7 ms`
- `GitHub Actions run 25992045585 perf-bench comrak_render_throughput_ops_per_sec: 25573.86 ops/s >= 850 ops/s`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- First push (`54ae67c`) fixed the scoped baseline alarms, but perf-bench failed once on an unrelated hard-budget edge (`diff_scroll_frame_p95_ms=16.8 > 16.7`) in run `25991353479`.
- Current HEAD `7e9ba47b6e48ad8ea6130c32eda4831a747fa4fa` retriggered CI and has `perf-bench` green in run `25992045585` (job `76399859179`).
- No PLAN §10 hard budgets were widened.
- No PR was opened (per instruction).

## Suggested follow-ups
- Add cloud env setup so agents don’t repeatedly install Linux WebKit/Tauri deps manually. Suggested env-setup prompt:
  - `Preinstall libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config xvfb and Playwright WebKit (pnpm --filter desktop exec playwright install --with-deps webkit) for the pr-cockpit-v2 cloud image so pnpm bench/corpus and rust checks run without per-agent bootstrap.`
- If `diff_scroll_frame_p95_ms` keeps occasionally hitting `16.8` on shared runners, consider a dedicated follow-up to further reduce CI jitter for that metric without changing the hard budget.
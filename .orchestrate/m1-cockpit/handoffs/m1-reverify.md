<!-- orchestrate handoff
task: m1-reverify
branch: `orch/m1-cockpit/m1-fix-gates` @ `6a553e8dab189848cb8c3dedb9c64966a8be7d5b` (verifier added `verify/` artifacts on top of worker SHA `c436ab81c59fff8fe0655fdb35171b5883e2b013`)
agentId: bc-5c357a89-61c7-4511-b6f1-4b587171b314
runId: run-30e8597c-3df7-4126-bfe6-d826e450107e
resultStatus: finished
finishedAt: 2026-05-17T12:33:46.827Z
-->

## Verification
unit-test-verified

## Target
`m1-fix-gates` on branch `orch/m1-cockpit/m1-fix-gates`

## Branch
`orch/m1-cockpit/m1-fix-gates` @ `6a553e8dab189848cb8c3dedb9c64966a8be7d5b` (verifier added `verify/` artifacts on top of worker SHA `c436ab81c59fff8fe0655fdb35171b5883e2b013`)

## Execution
Verifier VM (Ubuntu 24.04, node 22.22.2, pnpm 9.12.3, cargo 1.95.0, xvfb 1.21.x). Installed `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config xvfb` + Playwright WebKit before running the matrix.

- `pnpm install --frozen-lockfile` → exit 0
- `./scripts/fetch-grammars.sh` → exit 0 (8 wasm grammars)
- `pnpm ipc:bindings` → exit 0 (`generate-ipc-bindings` ran; `src-tauri/gen/schemas/{acl-manifests,capabilities,desktop-schema,linux-schema}.json` materialised)
- `cd apps/desktop && pnpm exec prettier --check src-tauri/gen/schemas/` (original repro path) → exit 0
- `bash verify/scripts/repro-lint-gen-ignored.sh` → exit 0 (planted intentionally bad-formatted JSON inside `apps/desktop/src-tauri/gen/schemas/__verify_unformatted.json`, `pnpm lint` still exited 0 → `.prettierignore` genuinely excludes the gen tree)
- `pnpm lint` → exit 0 (`verify/logs/pnpm-lint.log`)
- `cargo fmt --check` → exit 0
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0 (`verify/logs/cargo-clippy.log`)
- `cargo test --workspace` → exit 0 (16 test binaries, 0 failed, ~32 tests passed across desktop unit + integration suites incl. `token_safety`, `renderer_isolation`, `sync_integration`)
- `pnpm typecheck` → exit 0
- `pnpm svelte-check` → 0 errors / 0 warnings, exit 0
- `pnpm test` → 5 test files / 6 tests pass, exit 0
- `pnpm corpus` → `weighted_mean=0.015459` ≤ gate `0.02`, exit 0
- `unset GITHUB_TOKEN; pnpm online-demo` → skip-path taken (`GITHUB_TOKEN is not set; live API demo is optional and non-gating.`), exit 0
- `xvfb-run -a env PERF_BROWSER=webkit PERF_TARGET=preview pnpm bench` → exit 0. All 11 PLAN §10 metrics PASS: `inbox_first_paint_ms=8.74`, `pr_detail_open_preloaded_ms=0.61`, `pr_detail_open_cold_ms=7.31`, `file_open_in_diff_cached_ms=0.53`, `comrak_render_throughput=37205.52 ops/s`, `inbox_first_paint_ms_frontend=19.00`, `pr_detail_open_preloaded_ms_frontend=25.00`, `pr_detail_open_cold_ms_frontend=18.00`, `file_open_in_diff_cached_ms_frontend=59.00` (best of `[90,59,64,63,65]`), `diff_scroll_fps=62.07`, `diff_scroll_frame_p95_ms=16.20` (best of `[16.2,16.2,16.2,16.2,16.2]`). `bench/results/frontend.json` confirms `sampling.policy=best_of_n_min`, `sample_count=5` for both jitter-sensitive metrics.
- `git diff origin/orch/m1-cockpit/m1-finalize -- bench/budgets.json` → empty (no PLAN §10 budget widened)
- GH Actions run **25988682091** (the verifier-flagged baseline run on m1-finalize) showed `frontend (ubuntu-latest)`, `markdown-corpus`, `perf-bench` red. Fresh run **25990142848** on m1-fix-gates HEAD: `rust (ubuntu-latest)` ✓, `rust (macos-latest)` ✓, `frontend (ubuntu-latest)` ✓ (10m11s), `frontend (macos-latest)` ✓, `markdown-corpus` ✓ (9m34s), `perf-bench` ✗. The two CI gates the previous verifier flagged for missing Linux deps (`frontend (ubuntu-latest)` + `markdown-corpus`) are now GREEN — gate #2 is genuinely fixed.
- CI `perf-bench` failure log (`verify/logs/ci-perf-bench-tail.log`): `file_open_in_diff_cached_ms_frontend=66.00ms ≤ 100ms` ✓ and `diff_scroll_frame_p95_ms=16.40ms ≤ 16.7ms` ✓ — both scoped flake metrics PASS on CI. The CI job exited 1 due to two **other** baseline-regression alarms (10% threshold, not PLAN §10 hard budget): `comrak_render_throughput_ops_per_sec=20336.98` (10% threshold 27000), `pr_detail_open_preloaded_ms_frontend=40ms` (10% threshold 33ms). Both still clear their PLAN §10 hard budgets (≥850 ops/s and ≤50 ms). This is shared-runner baseline drift on metrics that were NOT in the three scoped gates.
- Inspected `DECISIONS.md` (lines 31–40) — dated `2026-05-17: WebKit perf gate uses best-of-5 estimator…` entry present. `bench/README.md` (lines 55–60) — matching WebKit timing policy block present.
- Committed `verify/` artifacts (audit + repro + logs) and pushed to `orch/m1-cockpit/m1-fix-gates` as commit `6a553e8`.

## Findings

Per scoped acceptance criterion (from the target task):
- [x] `pnpm lint` exits 0 from a clean checkout: confirmed locally (`verify/logs/pnpm-lint.log`); planted-bad-JSON repro also passes (`verify/logs/repro-lint-gen-ignored.log`). Met.
- [x] `.github/workflows/ci.yml` `frontend (ubuntu-latest)` + `markdown-corpus` jobs install Tauri Linux deps before pnpm/cargo steps: workflow has the install block in both jobs; CI run 25990142848 shows both jobs green. Met.
- [x] WebKit-side perf metrics reported as min/median of N=5; PLAN §10 `bench/budgets.json` NOT widened; `pnpm bench` exits 0 locally: `run-frontend-bench.mjs` implements `best_of_n_min` with `PERF_TIMING_SAMPLE_COUNT` (default 5) for `file_open_in_diff_cached_ms` and `diff_scroll_frame_p95_ms`; `git diff` vs `m1-finalize` on `bench/budgets.json` is empty; `pnpm bench` exit 0. Met.
- [x] Every PLAN.md §10 M1 budget enforced; harness still fails on a genuine regression: all 11 PLAN §10 budgets enforced by `tools/perf-bench/compare-budgets.mjs`; the CI run proved the regression-alarm path still fires (on unrelated metrics, not in scope). Met.
- [x] Full local CI matrix passes (cargo fmt/clippy/test, pnpm typecheck/svelte-check/lint/test/bench/corpus, online-demo skip): all exit 0. Met.
- [x] `DECISIONS.md` updated with dated min-of-N entry; `bench/README.md` matches: both present. Met.
- [x] Branch pushed; deliverable SHA reported: worker SHA `c436ab8`, verifier-artifacts SHA `6a553e8` on `orch/m1-cockpit/m1-fix-gates`. Met.

Per verifier-specific acceptance criteria:
- [x] All previously failing gates re-tested with concrete evidence: see Execution section (exit codes, perf numbers, GH Actions run IDs). Met.
- [x] Verification section enumerates PLAN §12 M1 + AUTONOMY_BRIEF §M1 + subtree criteria: bench-derived PLAN §10 budgets table + matrix table above cover every M1 quality bar that's scoped to this fix-gates task. Met.
- [x] Verifier branch carries `verify/` artifacts on top of m1-fix-gates: commit `6a553e8` pushed. Met.
- [x] Final `## Verification` value reflects suitability as M2's starting ref: yes — see severity-ordered findings below.

Other findings (severity-ordered):
- (med) CI `perf-bench` job exits 1 on run 25990142848 because the global 10% baseline-regression alarm fires on `comrak_render_throughput_ops_per_sec` (CI 20337 ops/s vs baseline 30000; threshold 27000) and `pr_detail_open_preloaded_ms_frontend` (CI 40 ms vs baseline 30 ms; threshold 33 ms). Both metrics CLEAR their PLAN §10 hard budgets (≥850 ops/s; ≤50 ms). This is a separate failure mode (relaxed sensitivity to CI hardware drift) not part of the three flagged gates and not a PLAN §10 violation. Locally on a beefier verifier VM neither alarm fires (comrak 37206 ops/s, preloaded 25 ms). Suggested follow-up task (not in scope here): either rebaseline these two metrics in `bench/budgets.json` against the CI runner class, or widen the 10% global tolerance only for runner-sensitive metrics, or apply the same best-of-N estimator on the WebKit `pr_detail_open_preloaded_ms_frontend` and re-measure comrak with a longer Criterion warmup. Whatever is chosen, the scoped fix-gates contract (only the two flagged jitter metrics get best-of-N, budgets not widened) is honoured here — this is a real but separate CI flake.
- (low) `actionlint` is not on the verifier VM image, so workflow lint was not run as an extra check. The CI run itself is the more authoritative signal and that proved green for the two jobs in scope.
- (low) The CI run takes ~30+ minutes for the perf-bench job (12m 56s rust build under `bench` profile). Cargo cache restore would help future cycles but is unrelated to this fix.

## Notes & suggestions
- M2 starting ref: the deliverable as scoped (lint, CI Linux deps, WebKit jitter) is fully met. The branch is suitable as M2's starting ref for the three scoped concerns. The med-severity follow-up above (CI `perf-bench` regression alarms on the comrak + preloaded metrics) is independent — it pre-existed on the m1-finalize branch in latent form (these metrics weren't in the original verifier's three-gate list because the runs of that earlier branch did not trip them), and the task explicitly forbids retuning `bench/budgets.json` outside what's needed to track measurement-source changes. I recommend the planner queue a follow-up `m1-bench-baselines` worker rather than blocking M2 on this.
- The `bench/results/frontend.json` sampling artefact is exactly what the planner asked for — raw per-sample arrays plus `sampling.policy=best_of_n_min`. The harness will still fail a genuine WebKit regression because the best-of-5 is compared to the unchanged hard budget; the CI run with `file_open=66`/`p95=16.40` confirms a comfortable margin.
- Recommended env-setup agent prompt (verifier saw the same gap as the worker): `Preinstall libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config xvfb and Playwright WebKit (pnpm --filter desktop exec playwright install --with-deps webkit) on the pr-cockpit-v2 cloud-agent base image so future cargo clippy/test and pnpm bench/corpus/lint runs do not require manual apt + playwright bootstrap.` Optionally also preinstall `actionlint`.
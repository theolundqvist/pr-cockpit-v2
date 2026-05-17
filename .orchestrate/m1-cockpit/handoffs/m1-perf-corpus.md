<!-- orchestrate handoff
task: m1-perf-corpus
branch: orch/m1-cockpit/m1-perf-corpus
agentId: bc-f1a2a30c-1418-4ed2-b30b-432f1b7abd3e
runId: run-a3e64bfd-137b-4bac-a854-4331e2b21c0f
resultStatus: finished
finishedAt: 2026-05-17T09:59:43.614Z
-->

I’m taking on the perf/corpus scope now and will first inspect the current M1 tree to see what harnesses/CI placeholders already exist, then I’ll implement the Rust + frontend benchmark gates and wire them into CI as hard failures with the required budgets.I found this branch currently contains only planning/docs, so I’m now fetching the existing implementation branches from origin and checking out `orch/m1-cockpit/m1-perf-corpus` to layer the perf/corpus work on top of real app code.I’m on the requested branch now with full app code. Next I’m mapping current bench/corpus placeholders and CI wiring so I can replace them with real gates without breaking existing scripts.I’ve located the placeholder bench/corpus scripts and the core Rust DB/render entry points. I’m now reading the existing DB integration/perf-ish tests and CI workflow to align new harnesses with current data fixtures and command structure.I’ve confirmed the scaffold still has bench/corpus no-ops. I’m now pulling in the markdown-pipeline branch state so the real corpus harness exists locally, then I’ll wire perf budgets and CI gates on top of it.I’m now drafting the concrete implementation plan (bench files, frontend harness scripts, CI wiring, and docs) and then I’ll execute it in one pass with budget checks baked into scripts so regressions fail deterministically.Plan I’m executing now:
1. Import the markdown corpus harness assets/scripts from `m1-markdown-pipeline`, then adapt `pnpm corpus` so CI gates on `<=2%` regression and supports optional online refresh behind `GITHUB_TOKEN`.
2. Replace Rust bench placeholder with four real Criterion benches + JSON budget checker against `bench/budgets.json` (10% regression tolerance), and add `perf_smoke.rs` headless timing output in `## Measurements` format.
3. Add `tools/perf-bench` Playwright harness that drives the built desktop UI, records inbox/PR/diff metrics (including 5k scroll FPS), and compares against PLAN §10 M1 thresholds.
4. Wire `pnpm bench` to run both Rust + frontend harnesses and update `.github/workflows/ci.yml` Linux perf and corpus jobs to be hard gates.
5. Document everything in `bench/README.md` and append perf/corpus decisions in `DECISIONS.md`, then run full required verification commands, commit, and push `orch/m1-cockpit/m1-perf-corpus`.I’ve imported the markdown-corpus assets and confirmed they need integration fixes (package naming/scripts). I’m now writing the actual bench harness files (Rust + Playwright + budget checker) and replacing placeholder scripts/CI jobs in one editing pass.Core code edits are in place now (criterion benches, Playwright harness, budget comparator, CI/job rewiring, corpus runner integration, and perf smoke test). I’m moving into compile/test/debug mode to fix any path/runtime issues and lock the budgets against real measurements.I’m at the verification phase now and will run the required command matrix (`cargo bench --no-run`, `cargo test`, `pnpm corpus`, `pnpm bench`) to shake out path/benchmark issues, then I’ll tune any failing script paths and finalize budgets from measured outputs.The corpus gate now behaves as required (fails on a single controlled mutation and passes again after revert). I’m running the full `pnpm bench` path now to validate criterion JSON parsing, Playwright metrics capture, and strict budget enforcement end-to-end.I’ve reached a green verification state for all required commands and am now staging everything into a single perf/corpus deliverable commit, then pushing `orch/m1-cockpit/m1-perf-corpus` to origin for handoff.I’m validating the current branch/commit state first, then I’ll run the required verification commands and push the deliverable branch `orch/m1-cockpit/m1-perf-corpus` before producing the final handoff.The deliverable commit is present locally on `orch/m1-cockpit/m1-perf-corpus`; I’m pushing it now to the matching remote branch and setting upstream tracking correctly.I’ve pushed the correct branch; next I’m re-running the required gate commands (`cargo bench --workspace --no-run`, `cargo test --workspace`, `pnpm corpus`, `pnpm bench`) so the handoff reflects fresh verification on this branch.Bench suite finished successfully; I’m extracting the key measured values from the run output now so the handoff includes concrete budget numbers.I’m now re-validating the corpus gate’s failure mode by mutating one corpus entry locally, running `pnpm corpus` to confirm it fails, and then restoring the file and confirming it passes again.I confirmed a GitHub Actions run was triggered by the push; I’m polling it to capture final CI status for the perf and corpus gate jobs before handoff.I found the newly triggered CI run failing early in `pnpm/action-setup`; I’m waiting for the run to finish so I can pull complete logs and verify whether this is a pre-existing CI config issue or something introduced in this branch.I’m creating a concrete CI screenshot artifact by capturing the GitHub Actions run page with Playwright so the handoff can include an actual image path.I identified a real CI blocker: `pnpm/action-setup@v4` is failing because the workflow specifies a pnpm version while `package.json` also pins one, which now hard-errors in Actions. I’m patching the workflow to use a single source of truth so perf/corpus jobs can execute.I’m editing `.github/workflows/ci.yml` now to remove duplicate pnpm version pinning from all `pnpm/action-setup` steps, which resolves the “Multiple versions of pnpm specified” CI hard failure.I committed and pushed the CI setup fix. I’m grabbing the new Actions run metadata so the handoff can point to the corrected run and the screenshot artifact path.## Status
success

## Branch
`orch/m1-cockpit/m1-perf-corpus`

## What I did
- Completed and pushed the perf/corpus gate implementation on the required branch, including:
  - Rust Criterion benches under `apps/desktop/src-tauri/benches/`:
    - `inbox_first_paint`
    - `pr_detail_open` (preloaded + cold)
    - `file_open_in_diff_cached`
    - `comrak_render_throughput`
  - Frontend perf harness under `tools/perf-bench/` with `pnpm bench` orchestration and budget enforcement via `bench/budgets.json`.
  - Markdown corpus gate wired into CI (`pnpm corpus`) with offline-capable scoring flow.
  - Synthetic headless perf smoke test in `apps/desktop/src-tauri/tests/perf_smoke.rs` with `## Measurements`-compatible output.
  - Documentation updates in `bench/README.md` and decision log updates in `DECISIONS.md`.
- Re-ran required local verification gates on this branch:
  - `cargo bench --workspace --no-run`
  - `cargo test --workspace`
  - `pnpm corpus`
  - `pnpm bench`
- Re-verified corpus failure/recovery behavior by controlled mutation:
  - Mutated one `oracle_html_sha` in `tools/markdown-corpus/corpus.json` and confirmed `pnpm corpus` failed.
  - Reverted mutation and confirmed `pnpm corpus` passed.
- Found and fixed a CI blocker unrelated to perf logic but preventing gates from running:
  - `.github/workflows/ci.yml` used `pnpm/action-setup@v4` with `version: 9` while `package.json` also pins pnpm via `packageManager`; GitHub Actions now fails with “Multiple versions of pnpm specified”.
  - Removed explicit `version` from all `pnpm/action-setup` steps so workflows use the `packageManager` pin only.
  - Commit pushed: `91dc52b fix(ci): use packageManager pnpm version only`.
- Captured CI run screenshots for handoff:
  - `/workspace/bench/artifacts/ci-run-25987568540.png`
  - `/workspace/bench/artifacts/ci-run-25987718408.png`

## Measurements
- `git commit 12925b5 files changed: 0 → 226`
- `git commit 12925b5 insertions: 0 → 3890`
- `git commit 12925b5 deletions: 0 → 23`
- `git commit 91dc52b files changed: 0 → 1`
- `git commit 91dc52b deletions: 0 → 6`
- `cargo bench --workspace --no-run exit_code: 0 == 0`
- `cargo test --workspace passing tests: 29 == 29`
- `pnpm corpus weighted_mean: 0.015459 <= 0.02`
- `pnpm corpus (controlled mutation) exit_code: 0 → 1`
- `pnpm corpus (after revert) exit_code: 1 → 0`
- `inbox_first_paint_ms (rust): 10.14 <= 100`
- `pr_detail_open_preloaded_ms (rust): 0.78 <= 50`
- `pr_detail_open_cold_ms (rust): 8.33 <= 250`
- `file_open_in_diff_cached_ms (rust): 0.74 <= 100`
- `comrak_render_throughput_ops_per_sec (rust): 36736.73 >= 850`
- `inbox_first_paint_ms_frontend: 81 <= 100`
- `pr_detail_open_preloaded_ms_frontend: 23 <= 50`
- `pr_detail_open_cold_ms_frontend: 17 <= 250`
- `file_open_in_diff_cached_ms_frontend: 70 <= 100`
- `diff_scroll_fps: 62.12 >= 60`
- `diff_scroll_frame_p95_ms: 16.2 <= 16.7`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- I did **not** merge/rebase/open a PR.
- Branch pushed exactly as requested: `orch/m1-cockpit/m1-perf-corpus` (tracking `origin/orch/m1-cockpit/m1-perf-corpus`).
- New CI run after the pnpm setup fix is active: `25987718408` (all jobs started/in-progress instead of immediate pnpm setup failure).
- Prior runs on this repo were failing early at `pnpm/action-setup` with:
  - “Multiple versions of pnpm specified … remove one of these versions …”
  - This is now addressed in `.github/workflows/ci.yml`.
- `bench/artifacts/` contains untracked screenshot artifacts for handoff evidence and was intentionally not committed.

## Suggested follow-ups
- Let CI run `25987718408` complete and confirm final pass/fail status of `perf-bench` and `markdown-corpus` on GitHub runners.
- If desired, add/ignore `bench/artifacts/` explicitly in repo policy (currently left untracked for handoff evidence only).
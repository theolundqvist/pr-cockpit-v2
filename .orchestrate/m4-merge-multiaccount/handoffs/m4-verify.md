<!-- orchestrate handoff
task: m4-verify
branch: `orch/m4-merge-multiaccount/m4-finalize` (verifier commit `a95f504` pushed with `verify/m4-finalize/` repro logs + report)
agentId: bc-d079e6bf-3ff8-4c76-89a7-14375cb47422
runId: run-35c1a62b-a257-4f79-9c4d-545c0be67701
resultStatus: finished
finishedAt: 2026-05-18T06:40:58.490Z
-->

## Verification
live-ui-verified

## Target
`m4-finalize` on branch `orch/m4-merge-multiaccount/m4-finalize`

## Branch
`orch/m4-merge-multiaccount/m4-finalize` (verifier commit `a95f504` pushed with `verify/m4-finalize/` repro logs + report)

## Execution
- Step 0 chain integration sanity: `git log --oneline origin/orch/m4-merge-multiaccount/{m4-merge-surface,m4-force-push-range-diff,m4-multi-account} ^HEAD` → all three empty (no integration commits needed).
- `cargo fmt --check` → clean.
- `cargo clippy --workspace --all-targets -- -D warnings` → clean (`Finished dev profile target(s) in 4m 55s`).
- `cargo test --workspace --no-fail-fast` → 44 binaries, 90 tests, 0 failed. M4-specific binaries that ran: `airplane_drill`, `composer_posting_identity`, `ghe_endpoint_derivation`, `ghe_round_trip`, `ghe_token_storage`, `multi_account_inbox`, `multi_account_rate_limit`, `mutations_harness`, full `notifications_*` suite, `push_history_reconcile`, `range_diff_fallback`, `range_diff_local_git`, `range_diff_rest_compare`, full `worktree_*` suite. One known flake (`db_integration::open_fixture_loads_inbox_under_timing_budget_best_effort`) tripped under heavy concurrent load (567 ms vs 500 ms shared-runner ceiling); passed in isolation and in the less-loaded full rerun.
- `pnpm typecheck` (incl. `cargo run --bin generate-ipc-bindings` → `tsc --noEmit`) → clean.
- `pnpm svelte-check` → `0 errors and 0 warnings`.
- `pnpm lint` → ESLint + Prettier clean.
- `pnpm test` (vitest) → 9 files / 11 tests passed.
- `pnpm corpus` → `weighted_mean = 0.000115 ≤ gate 0.015` (M3-tightened 1.5% bar).
- `pnpm bench` → every PLAN §10 metric within budget; key numbers: `inbox_first_paint_ms_frontend=98 ≤ 100`, `pr_detail_open_preloaded_ms=0.77 ≤ 50`, `pr_detail_open_cold_ms=28.14 ≤ 250`, `file_open_in_diff_cached_ms_frontend=48 ≤ 100`, `mutation_submit_visible_ms=0.39 ≤ 16`, `diff_scroll_fps=62.14 ≥ 60`, `diff_scroll_frame_p95_ms=16.39 ≤ 16.7`, `comrak_render_throughput=36548 ops/s ≥ 850`.
- Playwright sweep under `xvfb-run -a` (clean run, no concurrent jobs): `airplane.spec.ts`, `m2-smoke.spec.ts`, `m3-smoke.spec.ts`, `diff-polish.spec.ts`, `worktree.spec.ts`, `notifications.spec.ts`, `m4-merge-surface.spec.ts`, `m4-range-diff.spec.ts`, `m4-multi-account.spec.ts`, `m4-ghe.spec.ts` → **18 passed / 18** in 31.5s. Two flakes observed only when interleaved with `pnpm bench` (which holds port 4173 for its preview-server step); the clean run is fully green.
- Spot-read of `apps/desktop/src-tauri/src/mutations/mod.rs` → 3 new MutationKind variants `EnqueueMergeQueue`/`DequeueMergeQueue`/`ReorderMergeQueue` plus `EnableAutoMerge`/`DisableAutoMerge`/`UpdateBranch`/`Merge`/`DeleteHeadRef`.
- Spot-read of `apps/desktop/src-tauri/src/api/queries/PrDetail.graphql` → `mergeCommitAllowed/squashMergeAllowed/rebaseMergeAllowed/deleteBranchOnMerge/mergeQueue/defaultBranchRef.branchProtectionRule.{requiresApprovingReviews,requiredApprovingReviewCount,requiresStatusChecks,requiredStatusCheckContexts,requiresStrictStatusChecks,restrictsPushes,restrictsReviewDismissals}` all queried.
- Spot-read of `apps/desktop/src-tauri/migrations/0010_merge_surface.sql` → all 22 merge-surface columns added + `pr_detail_summary` view rebuilt to expose them.
- Spot-read of `apps/desktop/src/lib/components/merge/NoOptimismButton.svelte` → confirm modal → spinner → `Promise.all([listenEventPayload('mutation:reconciled',…), listenEventPayload('mutation:failed',…)])` before resolving; no optimistic DB projection in the merge family.
- Spot-read of `MergeBox.svelte` → `submitDeleteAfterMerge` runs inside the merge `NoOptimismButton.onComplete` callback (which only fires after the merge mutation is reconciled), and the delete itself awaits its own `mutation:reconciled` — sequencing is server-confirmed, not optimistic.
- Spot-read of `apps/desktop/src-tauri/src/sync/mod.rs::run_mergeable_backoff` → schedule `[2,5,15,45,120]s + loop 300s`; bundled `mergeable_backoff_emits_tick_schedule` MockClock test asserts `observed_sleeps == [2,5,15,45,120,300]`.
- Spot-read of `apps/desktop/src-tauri/src/sync/mod.rs` budgeter → `HashMap::<(String, ApiResource), BudgetState>::new()`; foreground bypass emits `rate_limit_bypass:<account>` per-account.
- Spot-read of `apps/desktop/src-tauri/src/auth/mod.rs::derive_endpoint_config` → dotcom → `api.github.com`; other host → `<host>/api/v3` + `<host>/api/graphql`, with `PR_COCKPIT_HOSTS_TOML` and platform configs.
- Spot-read of `apps/desktop/src-tauri/tests/ghe_round_trip.rs` → wiremock host registered as `ghe.local`, `github_api_origin` set to dead loopback, asserts dotcom account inbox stays empty after the GHE round-trip (no `api.github.com` leakage).
- Token leak audit: SQL migrations contain no `token`/`secret`/`bearer` columns; `accounts` table has only `token_kind`/`scopes`; `rate_limit_buckets` carries `account_id/resource/remaining/used/limit_total/reset_at`; only `SavePatTokenInput` carries token bytes (user-input PAT, routed straight to `KeyringTokenStore`) and only `Bearer …` Authorization headers ever embed `secret.access_token`, with `set_sensitive(true)` and the `ipc_token_safety` test passing.
- Verified all M4 artifact PNGs are real and large (RGBA, 1280×N, generated by the Playwright specs themselves) in `artifacts/m4-merge-surface/`, `artifacts/m4-mergeable-backoff/`, `artifacts/m4-range-diff/`, `artifacts/m4-multi-account/`, `artifacts/m4-foreground-bypass/`, `artifacts/m4-ghe/`. Smoke summary at `artifacts/m4-smoke/README.md` cross-links them.
- Migrations `0010_merge_surface.sql`, `0011_push_history.sql`, `0012_multi_account_rate_limits.sql` present in order (with matching `.down.sql` siblings).
- `git log --oneline -10` on the deliverable branch shows a clean linear history (`98bac50` "finalize smoke docs and gate polish" tip before my verify commit).
- Verifier commit `a95f504` pushed adding `verify/m4-finalize/{REPORT.md, cargo-clippy.log, cargo-test.log, pnpm-typecheck.log, pnpm-svelte-check.log, pnpm-lint.log, pnpm-test.log, pnpm-bench.log, pnpm-corpus.log, playwright-sweep.log}`.

## Findings
Per acceptance criterion:
- [x] Full local CI matrix passes (cargo fmt/clippy/test, pnpm typecheck/svelte-check/lint/test/bench/corpus, all M4 Playwright specs): met — see Execution.
- [x] End-to-end smoke documented under `artifacts/m4-smoke/`: met — `README.md` cross-links all six prior-worker artifact folders with scenario-by-scenario evidence.
- [x] DECISIONS.md M4 section consolidated and enumerates non-obvious M4 calls: met — top-of-section "M4 contract decisions (promoted for M5+)" summary plus four detailed dated entries cover queue reorder mutation choice (`reorderMergeQueueEntry` with `TOP`/`BOTTOM`), `NoOptimismButton` lifecycle, branch-protection JSON shape, mergeable backoff event schema, delete-branch sequencing, range-diff pairing + intra-line + worktree fallback, per-account budgeter, posting-identity contract, meter design + bypass observability, GHE endpoint derivation + device-flow gap.
- [x] README.md reflects M4 completion with the four new feature bullets and test pointers: met — status now reads `M4 merge surface + force-push range-diff + multi-account complete`, with merge-surface / force-push range-diff / multi-account / GHE bullets and direct test-file pointers.
- [x] Deliverable branch has coherent history; migrations 0010/0011/0012 in order: met.
- [x] All PLAN.md §10 perf budgets still green incl. `mutation_submit_visible_ms < 16 ms`; markdown corpus ≤ 1.5%: met.
- [x] Branch-protection-aware merge UI verified across three protection variations: met — `m4-merge-surface.spec.ts:34` exercises none/soft/hard with explicit `mergeEnabled`/`autoMergeEnabled`/`updateVisible`/`rebaseRadio` assertions and produces `branch-protection-{none,soft,hard}.png`. Data comes from the GraphQL query + `0010_merge_surface.sql` columns, not a frontend hardcode.
- [x] Merge queue enqueue/dequeue/reorder + updateBranch + delete-branch-on-merge verified: met — three new MutationKind variants + matching GraphQL files; Playwright `merge queue controls enqueue, reorder, and dequeue` and `update branch transitions through updating and clean` pass; `merge and delete branch runs delete only after merge reconcile` passes; `MergeBox.svelte` sequences `delete_head_ref` inside the merge button's `onComplete` (server-confirmed) callback.
- [x] `mergeable: null` backoff schedule verified (2/5/15/45/120/300s, max 5 min loop): met — `run_mergeable_backoff` schedule + `mergeable_backoff_emits_tick_schedule` MockClock unit test (`assert_eq!(observed_sleeps, vec![2,5,15,45,120,300])`) + recorded ticks 001–007 under `artifacts/m4-mergeable-backoff/` + Playwright `mergeable null backoff emits expected sequence screenshots` spec.
- [x] Force-push range-diff verified for both local-git path AND REST `/compare` path: met — three Rust integration tests (`range_diff_local_git`, `range_diff_rest_compare`, `range_diff_fallback`) pass; Playwright `local-git mode renders markers and intra-line highlights` + `rest mode renders the same range-diff structure` pass; recordings under `artifacts/m4-range-diff/`. The REST provider does not shell out (verified by grep — only `git range-diff` in `LocalGitRangeDiff::compute`).
- [x] Multi-account aggregated inbox + per-row badge + composer posting identity quick-switch + per-account rate-limit meter + foreground bypass: met — `multi_account_inbox`, `multi_account_rate_limit`, `composer_posting_identity` Rust tests + `m4-multi-account.spec.ts` (`switcher, badges, composer identity, and rate-limit meter`) pass; `artifacts/m4-foreground-bypass/01-03` shows red meter → background throttled → bypass chip.
- [x] GHE schema readiness — add-account modal + wiremock GHE round-trip + no api.github.com leakage: met — `ghe_round_trip.rs` registers `ghe.local` against wiremock with a deliberately-dead `github_api_origin`, asserts dotcom inbox stays empty; `ghe_endpoint_derivation.rs` + `ghe_token_storage.rs` green; `m4-ghe.spec.ts` end-to-end pass; recordings at `artifacts/m4-ghe/01-add-account.png`, `02-ghe-inbox.png`, `03-ghe-pr-detail.png`.
- [x] All prior milestone gates remain green; markdown corpus ≤ 1.5%; PLAN §10 perf budgets green on cloud-agent runner: met.
- [x] Token-leak audit rerun on M4 surface: met — see Execution; no token bytes in `pending_mutations`, `accounts`, `rate_limit_buckets`, `account_rate_limits`, or event payloads. `ipc_token_safety.rs` tracing-leak assertion green.

Other findings (severity-ordered):
- (low) The merge-surface migration adds 22 columns via `ALTER TABLE` and rebuilds `pr_detail_summary` as a SQL view, matching DECISIONS §M4. No down-migration concerns surfaced.
- (low) Playwright is flaky against the `pnpm bench` frontend benchmark when run concurrently (port 4173 collision because both spin up `vite preview`). Not a product bug; suggest the planner have CI run bench and Playwright sequentially (or pin different ports). The clean sequential run is fully green.
- (low) The `db_integration::open_fixture_loads_inbox_under_timing_budget_best_effort` test asserts `< 500 ms` shared-runner ceiling on a fixture inbox load that normally takes <100 ms; under high concurrent CPU load it can drift past 500 ms. Suggest the planner consider widening the ceiling or marking the test `#[ignore]` outside dedicated perf jobs.
- (low) `inbox_first_paint_ms_frontend` clocked 98 ms vs the 100 ms hard budget — only 2 ms of headroom on the cloud-agent runner; M5 work should be mindful before adding inbox-paint cost.

## Notes & suggestions
- Logs from every command I ran are committed at `verify/m4-finalize/*.log` plus a consolidated `verify/m4-finalize/REPORT.md` so the planner can re-audit without re-running.
- Playwright + bench port-4173 collision is worth a cleanup task: either bind the frontend bench to a different port or sequence them in the CI matrix.
- The shared-runner `db_integration` regression test is the only test that can plausibly fail under load; consider gating it on a `PERF_GATE=1` env var so non-perf runs don't trip on runner noise.
- All M4 deliverables (merge surface, mergeable backoff, force-push range-diff, multi-account, GHE) are present in code, Rust tests, Playwright specs, and screen recordings. The branch is in a clean state to pin as M5's starting ref.
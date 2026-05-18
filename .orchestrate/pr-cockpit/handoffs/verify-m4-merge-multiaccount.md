<!-- orchestrate handoff
task: verify-m4-merge-multiaccount
branch: `orch/pr-cockpit/m4-merge-multiaccount` (verifier-rerun commit `8e9ba92` on top of inherited tip `a95f504`)
agentId: bc-fc39074f-1761-4408-b6ce-8b9793258f5e
runId: run-71d21ada-8908-46c9-9a05-c466f45bb031
resultStatus: finished
finishedAt: 2026-05-18T07:19:08.292Z
-->

Clippy passed. Now let's run the rest in parallel.90 passed, 0 failed - matching the prior report. Now check on corpus and start playwright + bench.Corpus weighted_mean=0.000115 ≤ 0.015 (well under the 1.5% bar). Now let's run pnpm bench and Playwright sequentially (to avoid the port 4173 collision noted by the prior verifier).All perf budgets green. Now Playwright sweep under xvfb.All 20 Playwright tests pass (18 from M2/M3/M4 specs + offline smoke + a11y). Now let me do my own spot checks on the M4-specific code and acceptance.Backoff sequence `[2,5,15,45,120,300]` matches spec. Now confirm key paths exist.Everything lines up with the M4 deliverables. Let me check one final thing — that the verifier tip matches `a95f504` mentioned in the handoff.HEAD = `a95f504` matches the handoff. Now commit verifier artifacts to the branch.Pushed. Verifier rerun is committed at `8e9ba92`. Now my final handoff.

## Verification
live-ui-verified

## Target
`m4-merge-multiaccount` on branch `orch/pr-cockpit/m4-merge-multiaccount`

## Branch
`orch/pr-cockpit/m4-merge-multiaccount` (verifier-rerun commit `8e9ba92` on top of inherited tip `a95f504`)

## Execution
Independent rerun on a clean checkout of the inherited verifier tip (`a95f504`), full evidence logs committed at `verify/m4-verifier-rerun/`:

- `apt-get install` Tauri Linux deps (libgtk-3-dev, libwebkit2gtk-4.1-dev, libayatana-appindicator3-dev, librsvg2-dev, libevent-2.1-7t64, libgstreamer-plugins-bad1.0-0, libavif16, libwoff1, patchelf) → ok
- `pnpm install --frozen-lockfile` → ok
- `pnpm ipc:bindings` (rebuilds Specta bindings via `cargo run -p desktop --bin generate-ipc-bindings`) → ok
- `cargo fmt --check` → exit 0
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0 (`verify/m4-verifier-rerun/clippy.log`)
- `cargo test --workspace --no-fail-fast` → **90 passed / 0 failed** across **43 test binaries** (`cargo-test.log`); explicitly includes M4 binaries `composer_posting_identity`, `ghe_endpoint_derivation`, `ghe_round_trip`, `ghe_token_storage`, `multi_account_inbox`, `multi_account_rate_limit`, `range_diff_local_git`, `range_diff_rest_compare`, `range_diff_fallback`, `push_history_reconcile`, `mutations_harness`, `sync_integration::mergeable_null_recovery_uses_expected_virtual_clock_schedule`, `sync_integration::rate_limit_budgeter_throttles_background_only`
- `pnpm typecheck` → exit 0
- `pnpm svelte-check` → `0 errors and 0 warnings`
- `pnpm lint` → exit 0
- `pnpm test` (vitest) → 9 files / 11 tests passed (includes `Composer.quick-switch`)
- `pnpm corpus` → `weighted_mean=0.000115` ≤ 0.015 (well under both the 1.5% M3 bar and the 1% M6 target)
- `pnpm bench` → every PLAN §10 budget green (inbox_first_paint_ms_frontend=23 ≤ 100; diff_scroll_fps=62.17 ≥ 60; diff_scroll_frame_p95_ms=16.20 ≤ 16.7; mutation_submit_visible_ms=0.62 ≤ 16; comrak=36596 ops/s ≥ 850; full table in REPORT.md)
- `xvfb-run pnpm exec playwright test` (after `playwright install --with-deps` for webkit/firefox/chromium) → **20/20 passed** including all four M4 specs:
  - `m4-merge-surface.spec.ts` (8 cases: branch protection variations none/soft/hard, merge confirmation reconcile, merge+delete-branch ordering, auto-merge toggle, merge queue enqueue/reorder/dequeue, update-branch transitions, mergeable-null backoff screenshot sequence)
  - `m4-range-diff.spec.ts` (local-git mode, REST mode, force-push banner)
  - `m4-multi-account.spec.ts` (switcher, badges, composer identity, rate-limit meter)
  - `m4-ghe.spec.ts` (add account, switch, inbox/detail, endpoint routing proof)
- Spot-read source: `sync/mod.rs::run_mergeable_backoff` MockClock unit test asserts `observed_sleeps == [2, 5, 15, 45, 120, 300]`; `mutations/mod.rs::MutationKind` carries `EnableAutoMerge/DisableAutoMerge/UpdateBranch/Merge/DeleteHeadRef/EnqueueMergeQueue/DequeueMergeQueue/ReorderMergeQueue`; `mutations/handlers/{merge_controls,merge_queue}.rs` declare `OptimismLevel::None` for merge / auto-merge / delete-head-ref / all three merge-queue ops; `ipc/mod.rs` registers `list_pr_pushes` + `compute_range_diff`; `range_diff/mod.rs` exports both `LocalGitRangeDiff` and `RestCompareRangeDiff` with worktree-missing fallback; `auth/mod.rs::derive_endpoint_config` + `ghe_round_trip.rs` (dotcom-dead-loopback + wiremock ghe.local) proves no api.github.com leakage.
- Recorded artifacts present: `artifacts/m4-mergeable-backoff/tick-001..007*.png`, `artifacts/m4-range-diff/{local-git,rest-compare}.png`, `artifacts/m4-multi-account/{inbox-aggregated,composer-identity-dropdown-open}.png`, `artifacts/m4-foreground-bypass/01..03.png`, `artifacts/m4-ghe/01..03.png`.
- Committed verifier evidence + REPORT to branch and pushed (`8e9ba92`).

## Findings
Per acceptance criterion:
- [x] Merge / squash / rebase respect repo branch-protection + allowed methods; auto-merge enable/disable + merge queue + delete-branch all live: `m4-merge-surface.spec.ts` (8 cases) green; merge-controls + merge-queue handlers in place with `OptimismLevel::None`; branch-protection-aware UI proven by the three protection-variation Playwright cases. (met)
- [x] Force-push range-diff: local-git path when worktree present, else REST `/compare` + Rust-side range-diff; both paths render correctly: Rust tests `range_diff_local_git`, `range_diff_rest_compare`, `range_diff_fallback` all green; `m4-range-diff.spec.ts` exercises both modes + force-push banner. (met)
- [x] Multi-account UI: account switcher + multi-account inbox + posting-identity quick-switch + per-account rate-limit meter live: `m4-multi-account.spec.ts` green; Rust tests `multi_account_inbox`, `multi_account_rate_limit`, `composer_posting_identity` green; foreground-bypass screenshots show low-budget meter + background throttle + foreground bypass; `RateLimitBudgeter` keyed by `(account_id, ApiResource)`. (met)
- [x] `mergeable: null` backoff verified live with a recorded session (2/5/15/45/120/300 s, max 5 min): `sync_integration::mergeable_null_recovery_uses_expected_virtual_clock_schedule` + `sync/mod.rs::tests::mergeable_backoff_emits_tick_schedule` both assert the exact schedule against MockClock; 7-frame `tick-001..007` screenshots recorded; Playwright case `mergeable null backoff emits expected sequence screenshots` green. (met)
- [x] GHE schema + per-host endpoint config exercised end-to-end against a stubbed GHE host: `ghe_endpoint_derivation`, `ghe_round_trip` (dotcom-leakage assertion), and `ghe_token_storage` all green; `m4-ghe.spec.ts` flow green with 3 recorded screenshots. (met)
- [x] All prior milestone gates + perf budgets still green; markdown corpus ≤ 1.5%: 20/20 Playwright + full bench matrix green; corpus weighted_mean = 0.000115 ≤ 0.015 (and ≤ 0.01 M6 target). (met)

Other findings:
- (low) `UpdateBranch` handler returns `OptimismLevel::Cautious`, not `None`. The M4 task description groups it under "All as no-optimism", but PLAN.md §3.2 explicitly places `update branch` in the Cautious bucket; the implementation matches the contract source (PLAN.md), not the looser task-description language. No defect, but worth noting.
- (low) Playwright + `pnpm bench` collide on port 4173 when run concurrently (both spin up `vite preview`). Sequential run is clean. CI should sequence them or pin distinct ports.
- (low) `db_integration::open_fixture_loads_inbox_under_timing_budget_best_effort` asserts a <500 ms shared-runner ceiling and can drift past it under heavy concurrent CPU load (it passes cleanly here in `cargo test --workspace`). Consider gating on `PERF_GATE=1`.
- (low) `inbox_first_paint_ms_frontend` is 23 ms in this run (was 98 ms in m4-finalize's run; budget = 100 ms). Real run-to-run variance — M5 inbox work should be careful not to regress.
- (low) Playwright specs still 404 on `/fixture-user.png` and `src/assets/grammars/tree-sitter-rust.wasm` (currently filtered as allowed). Adding the assets would silence log noise. Carried forward from M3 verifier.

## Notes & suggestions
- Independently confirmed every M4 acceptance criterion via real binary + real DOM + recorded screenshots; nothing was "diff-read verified". Tip-of-branch verifier evidence is at `verify/m4-verifier-rerun/` (additive to the existing `verify/m4-finalize/`).
- Cloud-agent env setup cost is real: ~10 min for cargo to compile dependencies first time, ~10 min for `cargo clippy`, ~25 s for Playwright browser download. Strongly recommend the env-setup agent prompt from the upstream handoff (preinstall Tauri Linux deps + Playwright browsers + `pnpm install` on startup) before M5 spawns multiple workers — that alone saves ~25–30 min/worker.
- For M5 sequencing: the `m4-merge-multiaccount → m4-merge-surface / m4-force-push-range-diff / m4-multi-account / m4-ghe-readiness / m4-finalize / m4-verify` chain pattern (downstream worker explicitly merges siblings before doing its work, finalize/verify run on the integrated branch) worked well and should be replicated. Avoid the sibling-only star topology, which was the cause of one M4 worker only getting `unit-test-verified`.
- M3 quiet-hours flake (`notifications_quiet_hours::quiet_hours_suppress_os_dispatch_but_record_events`, UTC-midnight edge) is still latent in the tree; not re-tripped by M4 verification, but worth fixing before M5 touches notifications.
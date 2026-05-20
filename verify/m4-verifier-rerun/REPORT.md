# M4 verifier rerun (independent verification)

Verifier branch: `orch/pr-cockpit/m4-merge-multiaccount` @ `a95f504`
(= upstream-declared verifier tip; equivalent to
`orch/m4-merge-multiaccount/m4-finalize`).

This directory holds an independent re-run of the full M4 gate matrix on a
clean clone, performed after the handoff. It augments (does not replace)
the `verify/m4-finalize/` evidence already on the branch.

## Gate matrix

| Check                                   | Result                              | Log                  |
| --------------------------------------- | ----------------------------------- | -------------------- |
| `cargo fmt --check`                     | clean (exit 0)                      | (inline)             |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (exit 0)      | `clippy.log`         |
| `cargo test --workspace --no-fail-fast` | 90 passed / 0 failed, 43 binaries   | `cargo-test.log`     |
| `pnpm typecheck`                        | exit 0                              | `typecheck.log`      |
| `pnpm svelte-check`                     | 0 errors, 0 warnings                | `svelte-check.log`   |
| `pnpm lint`                             | exit 0                              | `lint.log`           |
| `pnpm test` (vitest)                    | 11 / 11 passed (9 files)            | `vitest.log`         |
| `pnpm bench`                            | every PLAN §10 budget green         | `bench.log`          |
| `pnpm corpus`                           | weighted_mean = 0.000115 ≤ 0.015    | `corpus.log`         |
| `xvfb-run pnpm exec playwright test`    | 20 / 20 passed                      | `playwright.log`     |

### Cargo tests (43 binaries, 90 tests)

Including all M4 binaries:
`composer_posting_identity`, `ghe_endpoint_derivation`, `ghe_round_trip`,
`ghe_token_storage`, `multi_account_inbox`, `multi_account_rate_limit`,
`mutations_harness` (covers all merge handlers), `push_history_reconcile`,
`range_diff_local_git`, `range_diff_rest_compare`, `range_diff_fallback`,
`sync_integration::mergeable_null_recovery_uses_expected_virtual_clock_schedule`,
`sync_integration::rate_limit_budgeter_throttles_background_only`.

### Bench results

| Metric                                | Observed     | Budget  |
| ------------------------------------- | ------------ | ------- |
| inbox_first_paint_ms                  | 27.71        | ≤ 100   |
| pr_detail_open_preloaded_ms           | 0.81         | ≤ 50    |
| pr_detail_open_cold_ms                | 27.92        | ≤ 250   |
| file_open_in_diff_cached_ms           | 0.75         | ≤ 100   |
| mutation_submit_visible_ms            | 0.62         | ≤ 16    |
| mutation_submit_visible_online_ms     | 0.61         | ≤ 16    |
| mutation_submit_visible_offline_ms    | 0.62         | ≤ 16    |
| comrak_render_throughput_ops_per_sec  | 36596        | ≥ 850   |
| inbox_first_paint_ms_frontend         | 23.00        | ≤ 100   |
| pr_detail_open_preloaded_ms_frontend  | 22.00        | ≤ 50    |
| pr_detail_open_cold_ms_frontend       | 56.00        | ≤ 250   |
| file_open_in_diff_cached_ms_frontend  | 71.00        | ≤ 100   |
| diff_scroll_fps                       | 62.17        | ≥ 60    |
| diff_scroll_frame_p95_ms              | 16.20        | ≤ 16.7  |

(Note: this run measured `inbox_first_paint_ms_frontend` at 23 ms — much
healthier headroom than the 98 ms reported by m4-finalize. Likely an
under-loaded run; the 2 ms-headroom concern from the previous verifier
is still worth tracking as an M5 risk but is not currently tripping.)

### Playwright sweep (20/20)

```
20 passed (1.1m)
```

All four M4 specs (`m4-merge-surface`, `m4-range-diff`, `m4-multi-account`,
`m4-ghe`) + prior M2/M3 specs + the offline-fixture smoke flow + a11y
spec all green under xvfb in a single sequential run.

## Targeted M4 spot-reads (re-confirmed)

- `apps/desktop/src-tauri/src/sync/mod.rs::run_mergeable_backoff` schedule
  asserted by unit test against MockClock:
  `vec![2, 5, 15, 45, 120, 300]` seconds (matches PLAN §M4: 2/5/15/45/120
  then loop at 300 s, max 5 min).
- `apps/desktop/src-tauri/src/mutations/mod.rs::MutationKind` includes
  `EnableAutoMerge`, `DisableAutoMerge`, `UpdateBranch`, `Merge`,
  `DeleteHeadRef`, `EnqueueMergeQueue`, `DequeueMergeQueue`,
  `ReorderMergeQueue`.
- `apps/desktop/src-tauri/src/mutations/handlers/merge_controls.rs` +
  `merge_queue.rs`: `Merge`, `EnableAutoMerge`, `DisableAutoMerge`,
  `DeleteHeadRef`, `EnqueueMergeQueue`, `DequeueMergeQueue`,
  `ReorderMergeQueue` all return `OptimismLevel::None`. `UpdateBranch`
  returns `OptimismLevel::Cautious` — this matches PLAN §3.2 (which
  explicitly puts "update branch" in the Cautious bucket, not the
  no-optimism bucket); the task-description language ("All as
  no-optimism") was looser than PLAN §3.2.
- IPC commands `list_pr_pushes` + `compute_range_diff` registered in
  `apps/desktop/src-tauri/src/ipc/mod.rs` and exposed via Specta to
  `apps/desktop/src/lib/ipc/bindings.ts`.
- Frontend components present:
  - `apps/desktop/src/lib/components/merge/{MergeBox,NoOptimismButton,MergeableBackoffMeter}.svelte`
  - `apps/desktop/src/lib/components/account/{AccountSwitcher,AccountBadge}.svelte`
  - `apps/desktop/src/lib/components/status/RateLimitMeter.svelte`
- `apps/desktop/src-tauri/src/range_diff/mod.rs` exports both
  `LocalGitRangeDiff` (shells `git range-diff`) and `RestCompareRangeDiff`
  (REST `/compare`); fallback verified by `range_diff_fallback.rs`.
- `apps/desktop/src-tauri/src/auth/mod.rs::derive_endpoint_config` plus
  `ghe_round_trip.rs` no-leakage assertion (dotcom dead-loopback +
  wiremock ghe.local) all green.

## Recorded artifacts present

- `artifacts/m4-mergeable-backoff/tick-001-2s.png` … `tick-007-resolved.png`
- `artifacts/m4-range-diff/{local-git,rest-compare}.png`
- `artifacts/m4-multi-account/{inbox-aggregated,composer-identity-dropdown-open}.png`
- `artifacts/m4-foreground-bypass/{01-low-budget-meter,02-background-throttled,03-foreground-bypass}.png`
- `artifacts/m4-ghe/{01-add-account,02-ghe-inbox,03-ghe-pr-detail}.png`

## Acceptance criteria verdict

- [x] Merge / squash / rebase respect repo branch-protection + allowed
  methods; auto-merge enable/disable + merge queue + delete-branch all
  live — `m4-merge-surface.spec.ts` Playwright cases pass for protection
  variations (none/soft/hard), merge confirmation reconcile sequencing,
  merge + delete-branch ordering, auto-merge toggle, merge queue
  enqueue/reorder/dequeue, update-branch transitions, mergeable-null
  backoff screenshots.
- [x] Force-push range-diff: local-git + REST `/compare` paths both
  render — `m4-range-diff.spec.ts` local-git + rest cases + force-push
  banner all pass; Rust tests `range_diff_local_git`,
  `range_diff_rest_compare`, `range_diff_fallback` green.
- [x] Multi-account UI: switcher + multi-account inbox + posting-identity
  quick-switch + per-account rate-limit meter live —
  `m4-multi-account.spec.ts` exercises switcher → badged inbox →
  composer identity → rate meter; Rust `multi_account_inbox`,
  `multi_account_rate_limit`, `composer_posting_identity` green; vitest
  `Composer.quick-switch` green.
- [x] `mergeable: null` backoff verified live with recorded session
  (2/5/15/45/120/300 s, max 5 min) — unit test asserts the schedule;
  `tick-001…007` screenshots recorded; `m4-merge-surface.spec.ts`
  mergeable-null case green.
- [x] GHE schema + per-host endpoint config round-trips against stubbed
  host — `ghe_round_trip.rs` proves no api.github.com leakage;
  `m4-ghe.spec.ts` UI flow green; 3 screenshots recorded.
- [x] All prior milestone gates + perf budgets still green; markdown
  corpus 0.000115 ≤ 0.015 (well under the 1.5% M3-tightened bar,
  comfortably under the 1% M6 target).

## Notes & follow-ups (carried forward from the m4-finalize verifier)

- Concurrent `pnpm bench` + Playwright collide on port 4173. Sequential
  matrix is green; CI should sequence or pin distinct ports.
- `db_integration::open_fixture_loads_inbox_under_timing_budget_best_effort`
  can drift past its 500 ms regression bound under concurrent CPU load;
  consider `PERF_GATE=1` gating.
- `inbox_first_paint_ms_frontend` had only 2 ms of headroom in
  m4-finalize's run; this run measured 23 ms (76 ms headroom), but
  there is real run-to-run variance — M5 should watch for inbox-paint
  regressions.
- Two Playwright fixtures (`/fixture-user.png`,
  `src/assets/grammars/tree-sitter-rust.wasm`) 404 during specs (404
  lines are filtered out as allowed). Adding the assets would silence
  log noise.
- One pre-existing M3 quiet-hours flake noted; not re-tripped during
  this M4 verification.

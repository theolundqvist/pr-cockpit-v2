# M3 Verifier Notes (`m3-diff-worktree-notifs`)

Verifier ran the full inherited M2 matrix plus new M3 specs against
`46cfd8b` on `orch/pr-cockpit/m3-diff-worktree-notifs` from a clean
cloud-agent VM (Ubuntu 24.04, x86_64, xvfb webkit, Tauri Linux deps
installed at run time, Playwright webkit + chromium installed at run
time). Logs under `verify/m3/logs/`. Reproduction script
`verify/m3/scripts/run-matrix.sh`.

## Verdict

`live-ui-verified` for the M3 deliverable surface; two carry-over
acceptance gates failed on the cloud-agent hardware and are noted
below.

## Evidence

| Step | Outcome |
| --- | --- |
| `cargo fmt --check` | OK |
| `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| `cargo test --workspace` (first run @ 00:34 UTC) | FAILED — `notifications_quiet_hours::quiet_hours_suppress_os_dispatch_but_record_events` |
| `cargo test --workspace` (rerun @ 00:51 UTC) | OK — 73 passed / 0 failed |
| `cargo test -p desktop --test notifications_quiet_hours` (@ 00:51 UTC) | OK |
| `pnpm ipc:bindings` | OK |
| `pnpm typecheck` | OK |
| `pnpm svelte-check` | OK |
| `pnpm lint` | OK |
| `pnpm test` (vitest) | OK — 9 passed / 9 |
| `pnpm corpus` | OK — `weighted_mean = 0.000115` (gate `0.015`, 202 entries) |
| `xvfb-run pnpm bench` (first run) | **FAILED** — `inbox_first_paint_ms_frontend = 119ms > 100ms` hard budget |
| `xvfb-run pnpm bench` (rerun) | **FAILED** — `inbox_first_paint_ms_frontend = 106ms > 100ms` hard budget |
| Playwright `m3-smoke.spec.ts` | OK (5 screenshots written to `artifacts/m3-smoke/`) |
| Playwright `diff-polish.spec.ts` | OK |
| Playwright `worktree.spec.ts` | OK |
| Playwright `notifications.spec.ts` | OK |
| Playwright `a11y-pr-detail.spec.ts` | OK |
| Playwright `m2-smoke.spec.ts` | OK |
| Playwright `airplane.spec.ts` | OK |

## M3 acceptance criteria

- [x] Multi-line review comments compose & post; suggestion-block fence
  renders & posts; viewed checkboxes per file persist via head_sha;
  images & binary render; renames detected and shown — verified in
  `m3-smoke.spec.ts` + `diff-polish.spec.ts` (5 screenshots under
  `artifacts/m3-smoke/`), `viewed_at_head_sha` enforced in
  `migrations/0007_diff_polish.sql` + `db/mod.rs`,
  `mutations/handlers/review_comments.rs::AddReviewComment` is wired in
  the dispatch table.
- [x] Worktree discovery via configured roots + `git worktree list
  --porcelain`; per-worktree override file honored; no `$HOME`
  autoscan — `worktree/discovery.rs` runs `git worktree list
  --porcelain` (l. 121–135) against roots only; integration tests
  `worktree_discovery.rs` + `worktree_mapping_signals.rs` +
  `worktree_cleanup_safety.rs` all pass.
- [x] `notify` watchers + 200–500 ms debounce; status pulled via `git
  status --porcelain=v2 --branch` + `git rev-list --left-right
  --count` — `worktree/watcher.rs` l. 313 + l. 406.
- [x] Confidence-scored PR mapping with manual override; app-created
  vs user-managed tracked; cleanup never touches user-managed worktrees
  or uncommitted work — covered by `worktree_mapping_signals.rs` and
  `worktree_cleanup_safety.rs`.
- [x] Native OS notifications fire post-reconcile; dedup on `(account,
  repo, pr, event_type, actor, server_event_id)`; configurable
  triggers; quiet hours + focus mode + per-repo filters — wired in
  `lib.rs::tauri_plugin_notification::init()` + `notify/dispatcher.rs`;
  Rust tests `notifications_dedup`, `notifications_focus_mode`,
  `notifications_per_repo_filters`, `notifications_quiet_hours`,
  `notifications_rule_engine` all pass after 00:51 UTC; Playwright
  `notifications.spec.ts` exercises the settings UI + `__NOTIF_DEBUG__`
  proxy.
- [PARTIAL] All M1 + M2 acceptance and perf budgets still green;
  markdown corpus regression ≤ 1.5% — markdown corpus is green at
  `0.000115`. `inbox_first_paint_ms_frontend` regressed to 106–119 ms
  vs 100 ms budget on this cloud-agent VM (two consecutive runs).
  Prior verifier reported it green on its own hardware; either the
  budget needs to be reconciled against typical CI hardware or the
  initial paint needs further optimization.
- [x] Accessibility screen-reader pass on PR detail —
  `a11y-pr-detail.spec.ts` exercises keyboard tab order + ARIA naming
  + `:focus-visible` rings under xvfb. Native AT (VoiceOver/NVDA/Orca)
  interaction is structurally out-of-scope for the cloud agent.

## Findings (severity-ordered)

1. **(high) Perf budget regression on cloud-agent hardware.**
   `inbox_first_paint_ms_frontend` is 106–119 ms on the cloud-agent VM
   vs the 100 ms hard budget (baseline 90 ms). Two consecutive
   `pnpm bench` runs both failed. Probably a hardware variance vs the
   M3 finalize verifier's local machine, but it means the planner-
   facing claim "All PLAN.md §10 budgets green" cannot be reproduced
   here. Suggested action for M4: re-baseline the frontend perf
   metrics on the cloud-agent class hardware (or document the
   intended baseline machine) so the gate is meaningful in CI.

2. **(high) `notifications_quiet_hours` test is flaky around UTC
   midnight.** The test sets `inside_start = now - 30 min`,
   `inside_end = now + 30 min`, and `days = [today]` where `today` is
   the local-day at the current `now`. When `now` is within ~30 min
   of midnight in the configured tz (`UTC` here), the window crosses
   midnight and the implementation correctly looks for the start day
   (yesterday) in `config.days`, which the test never includes. I
   observed this fail at 00:34 UTC and pass at 00:51 UTC on this VM.
   Recommended fix: in `apps/desktop/src-tauri/tests/notifications_quiet_hours.rs`,
   compute `today` against the start-of-window time, or set
   `days = [yesterday, today]`. The product code is fine.

3. **(low) Legacy `tools/markdown-corpus/score.ts` still hardcodes
   `GATE = 0.02`.** `pnpm corpus` runs `score.mjs` (0.015), so the
   `.ts` file is dead, but the inconsistency was already flagged by
   the M3 finalize verifier and is worth fixing or deleting outright.

4. **(low) `tree-sitter-rust.wasm` 404 in Playwright preview server.**
   Allow-listed in `diff-polish.spec.ts`; M5 should bundle the WASM
   grammars so the allow-list can be removed.

5. **(low) Tauri Linux deps + Playwright browsers are still NOT
   pre-baked in the cloud-agent base image.** Installing
   `libwebkit2gtk-4.1-dev` and friends + Playwright webkit/chromium
   ate ~7 minutes of verifier wall time. Suggested env-setup prompt
   from the M3 finalize handoff still applies.

## Reproduction

```bash
bash verify/m3/scripts/run-matrix.sh
```

Logs land under `verify/m3/logs/`.

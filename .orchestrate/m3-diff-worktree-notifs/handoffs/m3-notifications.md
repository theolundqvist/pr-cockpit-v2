<!-- orchestrate handoff
task: m3-notifications
branch: orch/m3-diff-worktree-notifs/m3-notifications
agentId: bc-e8a8ce6e-a734-4076-b77a-09cacc21f9f5
runId: run-0329751e-ab6a-4198-beef-8adc9f532364
resultStatus: finished
finishedAt: 2026-05-17T21:47:18.762Z
-->

## Status
success

## Branch
`orch/m3-diff-worktree-notifs/m3-notifications`

## What I did
- Wired native notifications end-to-end on desktop:
  - Added `tauri-plugin-notification` / `@tauri-apps/plugin-notification` dependencies.
  - Enabled `notification:default` capability.
  - Registered plugin init in Tauri bootstrap (`lib.rs`), keeping renderer plugin-free.
- Implemented the M3 notification backend slice under `apps/desktop/src-tauri/src/notify/`:
  - `triggers.rs`: all 8 trigger kinds (`review_requested`, `changes_requested`, `approved`, `mention`, `ci_fail`, `ci_recover`, `merge_conflict`, `mutation_failure`) with old/new snapshot predicates.
  - `dedup.rs`: INSERT OR IGNORE storage path against unique dedup tuple.
  - `rules.rs`: rule toggles, quiet hours, focus mode, per-repo allow/deny filters with deny precedence.
  - `dispatcher.rs` + `mod.rs`: OS dispatch, suppression handling (`deduped=1`), event fanout (`notification:event`), debug simulation.
- Added schema + IPC + sync wiring:
  - Migration `0009_notifications.sql` with `notification_events`, `notification_rules`, `notification_settings` and dedup unique constraint.
  - Added/connected reconciled sync broadcast consumption in engine flow.
  - Added notification IPC commands in `ipc/mod.rs` and regenerated `bindings.ts`; updated `client.ts` wrappers and mock behavior.
- Delivered settings UI + route + e2e:
  - Added `Notifications.svelte` settings panel (triggers, quiet hours, focus mode, per-repo filters, events list).
  - Added `/settings` page and sidebar entry.
  - Added Playwright `notifications.spec.ts`, plus Rust tests:
    - `notifications_rule_engine.rs`
    - `notifications_dedup.rs`
    - `notifications_quiet_hours.rs`
    - `notifications_focus_mode.rs`
    - `notifications_per_repo_filters.rs`
- Updated `DECISIONS.md` with dated M3 notifications contracts:
  - Trigger semantics/predicates, dedup primitive, quiet-hours suppression model, focus mode behavior, filter precedence, mutation failure classification, and cross-platform OS behavior notes.

## Measurements
- `notification dedup dispatches (100 synthetic duplicate events): 100 -> 1`
- `quiet-hours suppressed event rows (inside-window batch): 0 -> 3`
- `quiet-hours OS dispatches (outside-window batch): 0 -> 3`
- `focus-mode OS dispatches (focus on/off batches): 0 -> 3`
- `cargo test --workspace failing tests: 0 == 0`
- `desktop vitest suite: 8 passed == 8 passed`
- `diff_scroll_fps (pnpm bench): 62.06 >= 60`
- `markdown corpus weighted_mean (pnpm corpus): 0.015459 <= 0.02`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Full required gates were run and passed:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm typecheck`
  - `pnpm svelte-check`
  - `pnpm lint`
  - `pnpm test`
  - `pnpm bench`
  - `pnpm corpus`
- UI/interactive recording artifact captured at:
  - `/opt/cursor/artifacts/notifications-settings.mp4`
- Minor hardening added for strict clippy in shared test support modules (`#![allow(dead_code)]`) so per-test-target compilation with `-D warnings` stays green.

## Suggested follow-ups
- Add a small IPC getter for current notification settings state (quiet-hours/focus/filter values) so the settings panel can preload persisted values instead of only writing updates.
- Consider extending the notification events UI to subscribe globally and support pagination/seen filtering for larger inbox histories.
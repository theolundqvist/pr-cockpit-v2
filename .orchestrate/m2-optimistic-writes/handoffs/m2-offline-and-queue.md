<!-- orchestrate handoff
task: m2-offline-and-queue
branch: orch/m2-optimistic-writes/m2-offline-and-queue
agentId: bc-b0fcbf9f-df2c-4641-989a-4d3f8ad1200a
runId: run-3bbd260c-71a2-4d29-a019-e69bbb92579f
resultStatus: finished
finishedAt: 2026-05-17T17:13:32.794Z
-->

## Status
success

## Branch
`orch/m2-optimistic-writes/m2-offline-and-queue`

## What I did
- Added offline network state monitoring in `apps/desktop/src-tauri/src/mutations/net.rs`:
  - Implemented `NetworkMonitor` actor with injectable `NetProbe`.
  - Added `NetState` (`Online` / `Offline { error_kind }`) with `watch::Receiver` subscription.
  - Wired probe cadence to 15s and offline/online transition behavior, including API-error signaling.
  - Added deterministic unit tests for offline→online and immediate API-error offline transitions.
- Extended mutation engine offline behavior in `apps/desktop/src-tauri/src/mutations/engine.rs`:
  - Offline submit now persists + projects optimism, defers `apply`, and emits `Queued { reason: "offline" }`.
  - Replay/drain processes `pending_mutations` in stable submission order.
  - Unsafe (`OptimismLevel::None`) rows remain pending with `requires_connection_confirmation`.
  - Added hard-conflict path: targeted refetch + diff creation + `MutationEvent::HardConflict` emission; marks mutation failed/non-retryable.
  - Preserved idempotent resubmission/no-op behavior via existing idempotency key handling.
- Added queue/draft schema and DB API updates:
  - New migration `apps/desktop/src-tauri/migrations/0006_offline_queue_and_drafts.sql`.
  - Added `drafts` table and `db::upsert_draft`, `db::list_drafts`, `db::delete_draft` (`src/db/mod.rs`, `src/db/types.rs`).
  - Extended pending-mutation record shape for `requires_connection_confirmation` and `last_error_json`.
- Wired IPC/event surface and types:
  - Updated mutation IPC payloads/events in `src/mutations/ipc_types.rs`.
  - Wired event exposure in `src/ipc/mod.rs` and module export in `src/mutations/mod.rs`.
  - Regenerated bindings (`apps/desktop/src/lib/ipc/bindings.ts`).
- Added deterministic airplane drill integration test:
  - New `apps/desktop/src-tauri/tests/airplane_drill.rs` covering offline queueing, engine reboot persistence, reconnect drain/reconcile, id mappings, and merge gating semantics.
  - Updated harness expectations for optimism-none behavior in `tests/mutations_harness.rs`.
- Extended performance coverage:
  - Updated `benches/mutation_submit_visible.rs` for online + offline submit paths.
  - Updated `tools/perf-bench/run-rust-benches.mjs` and `bench/budgets.json` with separate online/offline metrics.
- Documented decisions in `DECISIONS.md`:
  - NetworkMonitor cadence/event semantics.
  - `requires_connection_confirmation` semantics.
  - Hard-conflict diff schema/event contracts.
  - Airplane-drill recipe and extension guidance.
- Ran all requested quality gates and pushed commit `8bcb9a0a38432cc7296475c943e2524bb4c486f6`.

## Measurements
- `airplane_offline_queue_replay_drill wall time: 0.20 s < 10 s`
- `mutation_submit_visible_online_ms: 0.30 ms < 16 ms`
- `mutation_submit_visible_offline_ms: 0.29 ms < 16 ms`
- `cargo fmt --check: fail → pass`
- `cargo clippy --workspace --all-targets -- -D warnings: pass == pass`
- `cargo test --workspace: 0 failed == 0 failed`
- `pnpm typecheck: fail (missing node_modules) → pass`
- `pnpm svelte-check: 0 errors == 0 errors`
- `pnpm lint: pass == pass`
- `pnpm test: 6 passing == 6 passing`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Branch was already at the required name; changes were committed and pushed directly there as requested.
- Full `pnpm bench` fails in this environment because Playwright WebKit is not installed; Rust bench budgets (including offline/online mutation submit visible metrics) were validated via `node tools/perf-bench/run-rust-benches.mjs`.
- No UI behavior was changed in this task slice (backend/IPC/tests/bench/docs only), so no screen recording artifact was produced.

## Suggested follow-ups
- Install Playwright browsers in the cloud environment (`pnpm exec playwright install`) if planner wants full `pnpm bench` (frontend perf) to run green in CI/agents.
- Consider adding cloud env bootstrap to run `pnpm install` automatically for fresh agents to avoid first-run typecheck failures.
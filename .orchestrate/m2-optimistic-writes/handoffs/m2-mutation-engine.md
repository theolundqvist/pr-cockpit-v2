<!-- orchestrate handoff
task: m2-mutation-engine
branch: orch/m2-optimistic-writes/m2-mutation-engine
agentId: bc-6106d758-ead8-4fc7-9a1a-642906c9e77b
runId: run-8412579e-c45e-417c-974e-e3665cbd2ce4
resultStatus: finished
finishedAt: 2026-05-17T15:53:37.043Z
-->

## Status
success

## Branch
`orch/m2-optimistic-writes/m2-mutation-engine`

## What I did
- Landed the M2 mutation engine core in `apps/desktop/src-tauri/src/mutations/`:
  - Added `OptimismLevel`, full `MutationKind` enum + `as_str`/`FromStr`, mutation context structs, `Mutation` trait (predict/apply/rollback/reconcile defaults), `PredictedEffect`, `ServerResponse`, `ErrorKind`.
  - Implemented `patch.rs` with JSON-serializable cell-level patch algebra and inverse generation (`Patch::merge_inverses`).
  - Implemented `projector.rs` with `apply_patch` and `PatchSource::{OptimisticPrediction,Rollback,ServerReconcile}` behaviors, including pending overlay stamping/clearing.
  - Implemented `engine.rs` with `submit`, `drain`, `retry`, `discard`, event broadcast, idempotency dedupe, retry/backoff/error classification, and mutation attempt recording.
  - Implemented `reconciler.rs` shared reconcile path for upserts, markdown adjustment flags, id mapping writes/swaps, targeted refetch enqueue, and PR cache invalidation signaling.
  - Added `stub_handlers.rs` with `AddComment` end-to-end stub + additional minimal stubs used to exercise runtime/property tests.
  - Added `ipc_types.rs` with `SubmitPayload`, `PendingMutationView`, `MutationEvent`, `HardConflictDiff`, etc., specta-exportable via `Type`.
- Added optimistic writes migration pair:
  - `apps/desktop/src-tauri/migrations/0005_optimistic_writes.sql`
  - `apps/desktop/src-tauri/migrations/0005_optimistic_writes.down.sql`
  - Includes `body_server_adjusted` / `server_adjusted_at`, `pending_state` overlays, `mutation_attempts`, and read-model view updates exposing `pending_overlay`.
- Updated DB surface in:
  - `apps/desktop/src-tauri/src/db/mod.rs`
  - `apps/desktop/src-tauri/src/db/types.rs`
  - `apps/desktop/src-tauri/tests/db_integration.rs`
  to parse/surface `pending_overlay` and support new pending mutation fields.
- Added proptests in `apps/desktop/src-tauri/tests/mutation_engine_proptest.rs` covering rollback restoration, reconcile equivalence, interleaving convergence, id_mapping monotonicity, and projector involution with deterministic seeds.
- Added/updated bench + budgets:
  - `apps/desktop/src-tauri/benches/mutation_submit_visible.rs`
  - `tools/perf-bench/run-rust-benches.mjs`
  - `bench/budgets.json`
- Fixed benchmark hang and reliability issues:
  - Warmup hang root cause was idempotency-key reuse across criterion calibration runs; switched to monotonic atomic sequence keys.
  - Fixed benchmark DB lifetime issue by retaining `TempDir` for the full benchmark duration.
- Appended M2 decisions in `DECISIONS.md` (patch schema, server-adjusted semantics, retry/backoff behavior, deterministic proptest seed reproduction).
- Committed and pushed:
  - `d776190` (`feat(desktop): add m2 optimistic mutation engine core`)

## Measurements
- `mutation_submit_visible_ms: 16.00 ms > 0.32 ms`
- `cargo test --workspace: 36 passed == 36 passed`
- `pnpm --filter desktop test: 6 passed == 6 passed`
- `svelte-check diagnostics: 0 errors == 0 errors`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- The benchmark hang was deterministic and caused by deduped submits emitting no `Submitted` event during criterion warmup/calibration; fixed in-bench without changing engine semantics.
- The mutation kind list in instructions says “25” but enumerates 27 variants; implementation follows the full enumerated list.
- No UI bug was addressed in this scoped Rust task, so no screen recording artifact was produced.

## Suggested follow-ups
- Implement real per-kind handler bodies for all mutation kinds in place of stubs (`m2-mutation-handlers`).
- Wire IPC commands/events on top of `mutations::ipc_types` (`m2-ipc-and-frontend`).
- Add offline/airplane replay drill and hard-conflict UX coverage in frontend/integration layers.
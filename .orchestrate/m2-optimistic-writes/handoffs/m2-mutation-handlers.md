<!-- orchestrate handoff
task: m2-mutation-handlers
branch: orch/m2-optimistic-writes/m2-mutation-handlers
agentId: bc-61d1a3d6-ae01-4de0-b7f7-11e4ed3e4cab
runId: run-3aef58cb-05cf-4e77-a2b6-de4b08491f84
resultStatus: finished
finishedAt: 2026-05-17T16:32:29.541Z
-->

## Status
success

## Branch
`orch/m2-optimistic-writes/m2-mutation-handlers`

## What I did
- Implemented the full real mutation handler stack under `apps/desktop/src-tauri/src/mutations/handlers/`:
  - `comments.rs`, `reactions.rs`, `labels.rs`, `assignees.rs`, `reviewers.rs`, `reviews.rs`, `threads.rs`, `viewed_files.rs`, `pr_meta.rs`, `merge_controls.rs`, plus shared `common.rs`.
  - Each kind now has concrete `Mutation` impl behavior (`predict` / `apply` / default `rollback` / default-or-overridden `reconcile`) with optimism levels wired as Full/Cautious/None per plan policy.
- Replaced stubs with real dispatch wiring:
  - Added `mutations/dispatch.rs`.
  - Updated `mutations/mod.rs` and engine registration path so drains run real handlers.
- Added GraphQL mutation artifacts under canonical location:
  - `apps/desktop/src-tauri/src/api/queries/mutations/*.graphql` (10 files).
  - Updated canonical query guard test to allow mutation files only under that subdirectory.
- Extended API and DB call surface for mutation transport:
  - Added `graphql_mutation`, `rest_mutation_json`, `rest_mutation_no_response` path usage and idempotency header support through `GithubClient`.
- Added/updated tests:
  - Property tests expanded to full handler dispatch set in `tests/mutation_engine_proptest.rs` and made deterministic/green with real preconditions.
  - Wiremock harness and family integration tests:
    - `tests/mutations_harness.rs`
    - `tests/mutations_comments.rs`
    - `tests/mutations_collaboration.rs`
    - `tests/mutations_reviews_threads.rs`
    - `tests/mutations_pr_meta.rs`
    - `tests/mutations_e2e.rs`
- Fixed handler correctness issues found by tests/clippy:
  - `RemoveReaction` endpoint path composition bug (double `/reactions`).
  - `UpdateBranch` optimistic patch incorrectly mutating `pull_requests.state`; now uses `merge_state_status`.
  - `ConvertToDraft`/`MarkReadyForReview` upsert ID conflict on PR identity; now upserts by local `pr_id`.
  - `SetAssignees` rollback timestamp restoration corrected (`previous_assigned_at` support).
- Updated `DECISIONS.md` with dated M2 entry covering:
  - per-kind REST vs GraphQL choice,
  - idempotency strategy,
  - cautious pending convention (`SubmitReview`),
  - no-optimism rationale for merge-family controls.
- Kept migration usage on `0005_optimistic_writes.sql`; no additional schema delta was required, so no `0006_handler_columns.sql` was added.
- Committed and pushed:
  - `b1829dc feat(desktop): implement m2 mutation handlers and wiremock coverage`

## Measurements
- `git diff --shortstat (HEAD^..HEAD): 0 files changed → 36 files changed, 5122 insertions(+), 388 deletions(-)`
- `cargo fmt --all --check: fail → pass`
- `cargo clippy --workspace --all-targets -- -D warnings: fail → pass`
- `cargo test --workspace: fail (1 failing test) → pass`
- `pnpm typecheck: fail (missing node_modules/svelte-kit) → pass`
- `pnpm svelte-check: not run → pass`
- `pnpm lint: fail (3 graphql prettier violations) → pass`
- `pnpm test: 6 passing == 6 passing`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- The upstream prompt says “25 mutation kinds” but the enumerated set is 27; implementation/dispatch/tests follow the full enumerated set from the current `MutationKind` enum.
- Full workspace verification is green now; one perf-sensitive DB test (`open_fixture_loads_inbox_under_timing_budget_best_effort`) was initially over threshold during a cold run and passed on rerun.
- Installed missing system dependencies to unblock Rust desktop builds in this environment: `libgtk-3-dev`, `libsoup-3.0-dev`, `libwebkit2gtk-4.1-dev`.
- Ran `pnpm install` to satisfy JS toolchain checks (`typecheck`/`svelte-check`/`lint`/`test`).

## Suggested follow-ups
- Add an env-setup agent configuration so future cloud agents start with required desktop deps preinstalled. Suggested prompt:
  - “Update this repo’s cloud environment setup to preinstall Linux desktop build dependencies (`libgtk-3-dev`, `libsoup-3.0-dev`, `libwebkit2gtk-4.1-dev`) and run `pnpm install` at startup so `cargo test/clippy` and `pnpm typecheck/svelte-check/lint/test` work without manual bootstrap.”
- If planner wants stricter invariants, add an explicit property asserting rollback restoration for the cross-family e2e sequence (not just per-kind and interleaving determinism).
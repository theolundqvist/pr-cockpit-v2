<!-- orchestrate handoff
task: m6-stacks
branch: orch/m6-stacks-relay-polish/m6-stacks
agentId: bc-9a2f18fc-3902-441d-a4f9-edefe3764edd
runId: run-57ca4624-1977-42eb-9d17-988324710a4b
resultStatus: finished
finishedAt: 2026-05-18T18:24:48.275Z
-->

## Status
success

## Branch
`orch/m6-stacks-relay-polish/m6-stacks`

## What I did
- Implemented and wired the M6 stacked-PR backend surface in Tauri:
  - Added stack schema migration (`0017_stacks.sql` + down) with `stacks`, `pr_stack_position`, `stack_operations`, indexes, and `pr_stack_summary`.
  - Added new `stacks` module (`mod.rs`, `ops.rs`, `graphite.rs`) for stack detection, blocked-by computation, persistence, list/read-model queries, rebase/merge sequencing, resume/abort flow, and Graphite integration.
  - Added GraphQL mutation artifact for base retargeting and wired it into API constants.
  - Connected stack recomputation + event emission into sync flow and IPC (`stacks:<account>:<repo> changed`, `stack_op:<op_id>`).
- Implemented frontend M6 stack UX:
  - Added `StackTree.svelte` and `StackDagWarning.svelte` with linear-first stack rendering, DAG warning UI, blocked/conflict/CI/review badges, base→head SHA display, and stack action controls.
  - Wired stack data into cockpit state/layout/settings, including Graphite toggle behavior and badge rendering.
  - Extended IPC mock client/debug hooks for stack operations/events and Graphite status.
- Added complete M6 test coverage:
  - Rust integration tests for stack detection (linear/diamond/cycle), sync round-trip persistence/events, rebase happy/conflict, merge sequential/pause-on-failure, and Graphite opt-in behavior.
  - Vitest for stack tree render behavior.
  - Playwright `m6-stacks.spec.ts` covering linear/DAG render, rebase happy/conflict modal flow, merge sequencing, and Graphite toggle/badge.
- Fixed critical rebase op state bug:
  - `rebase_stack` was previously overwriting paused outcomes to `succeeded`; changed execution flow to return explicit outcome (`Completed` vs `Paused`) and only mark succeeded on true completion.
- Appended DECISIONS.md with a dated M6 entry covering required schema/algo/ops/Graphite/IPC contracts.
- Added artifacts under `artifacts/m6-stacks/`:
  - `linear-stack.png`
  - `graphite-toggle-flow.mp4` (UI interaction recording)

## Measurements
- `cargo clippy --workspace --all-targets -- -D warnings`: `1 error` → `0 errors`
- `cargo test --workspace`: `1 failed` → `0 failed`
- `pnpm typecheck (apps/desktop)`: `missing deps / failed` → `passed`
- `pnpm svelte-check (apps/desktop)`: `1 error` → `0 errors`
- `pnpm lint (apps/desktop)`: `2 files unformatted` → `0 files unformatted`
- `pnpm test (apps/desktop)`: `15 passing` == `15 passing`
- `xvfb-run -a pnpm exec playwright test playwright/*.spec.ts`: `2 failed, 39 passed` → `0 failed, 41 passed`
- `artifacts/m6-stacks files`: `0` → `2`

## Verification
live-ui-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Full required gates were run and are green in this environment:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm typecheck`
  - `pnpm svelte-check`
  - `pnpm lint`
  - `pnpm test`
  - `xvfb-run -a pnpm exec playwright test playwright/*.spec.ts`
- The Rust perf-sensitive inbox budget test (`open_fixture_loads_inbox_under_timing_budget_best_effort`) was flaky once (563ms) and passed on rerun (62ms); full suite rerun succeeded.
- To get Playwright fully green in this cloud runner, I installed:
  - workspace JS deps (`pnpm install`)
  - Linux browser deps (`libevent-2.1-7t64`, `libgstreamer-plugins-bad1.0-0`, `libavif16`, `libwoff1`)
- Environment setup recommendation for future agents (from Cursor web env-setup agent):  
  `For /workspace, preinstall JS workspace deps (pnpm install) and Playwright Linux deps (libevent-2.1-7t64, libgstreamer-plugins-bad1.0-0, libavif16, libwoff1) in the base image/startup so Playwright and frontend checks run without per-agent bootstrap.`

## Suggested follow-ups
- Proceed with the next scoped milestones (`m6-relay-and-ghe`, `m6-polish`, `m6-finalize`, `m6-tag-v1`) on top of this branch state.
- Consider stabilizing the perf-threshold integration test with a less noisy CI threshold/window if intermittent runner variance remains an issue.
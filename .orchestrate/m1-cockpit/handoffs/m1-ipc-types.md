<!-- orchestrate handoff
task: m1-ipc-types
branch: orch/m1-cockpit/m1-ipc-types
agentId: bc-c554927f-0f3d-45ec-8f6a-dbebe0ed15b4
runId: run-51fd63b9-e1e9-4575-9dd1-3ccc70f7746b
resultStatus: finished
finishedAt: 2026-05-17T08:07:34.448Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-ipc-types`

## What I did
- Added a new typed IPC surface at `apps/desktop/src-tauri/src/ipc/mod.rs`:
  - Implemented all required read-focused Tauri commands under `ipc::` with `#[tauri::command]` + `#[specta::specta]`:
    - account listing/switching
    - inbox listing
    - PR detail summary
    - PR timeline (paged)
    - PR review threads (paged)
    - PR check summary
    - PR files + file tree summary
    - PR patch (canonical `.diff` blob payload)
    - rendered comment HTML
    - notifications listing
    - `system_status` (rate-limit + sync-tier snapshot)
  - Added typed IPC error + request/response payload DTOs for renderer-facing API.
- Wired cache-invalidation event contracts and emission plumbing:
  - Added event payload types and event templates for specta generation:
    - `pr:<id> changed`
    - `inbox:account:<id> changed`
    - `rate_limit:account:<id> changed`
    - `notifications:account:<id> changed`
  - Added `TauriCacheInvalidationEmitter` in `ipc::` for runtime event emission.
  - Extended sync internals in `apps/desktop/src-tauri/src/sync/mod.rs`:
    - `CacheInvalidationEmitter` trait + noop implementation
    - `RateLimitBudgeter::start_with_emitter`
    - `RealActions::new_with_emitter`
    - emit calls after reconcile paths and rate-limit writes
    - sync tier snapshot state store for `system_status`.
- Extended DB read helpers for IPC in:
  - `apps/desktop/src-tauri/src/db/mod.rs`
  - `apps/desktop/src-tauri/src/db/types.rs`
  - Added read methods/rows for timeline paging, review threads, check runs, notification listing, patch blob SHA lookup, rate-limit bucket listing, and PR id resolution by repo/number.
- Integrated IPC builder into app bootstrap (`apps/desktop/src-tauri/src/lib.rs`):
  - Tauri now uses the specta IPC invoke handler from `ipc::`.
  - Mounted specta events in setup.
  - Managed shared `Db` + sync tier state in app state for command access.
  - Added renderer modules and renderer version export.
- Brought in markdown renderer/cache modules from upstream dependency:
  - `apps/desktop/src-tauri/src/render/mod.rs`
  - `apps/desktop/src-tauri/src/storage/mod.rs`
  - Added required crate deps (kuchiki/once_cell/regex/hex/rusqlite-compatible version).
- Added specta/tauri-specta binding generation pipeline:
  - Added generator bin: `apps/desktop/src-tauri/src/bin/generate_ipc_bindings.rs`
  - Added build-time regeneration in `apps/desktop/src-tauri/build.rs` (isolated nested target dir to avoid lock recursion).
  - Generated and committed `apps/desktop/src/lib/ipc/bindings.ts`.
  - Added renderer import surface `apps/desktop/src/lib/ipc/index.ts`.
- Added enforcement tests:
  - `apps/desktop/src-tauri/tests/renderer_isolation.rs` for forbidden renderer direct DB/GitHub access patterns.
  - `apps/desktop/src-tauri/tests/ipc_token_safety.rs` for IPC flow tracing scan against stubbed keychain token bytes.
  - `apps/desktop/src-tauri/tests/ipc_bindings.rs` to enforce bindings freshness.
- Updated CI and scripts:
  - `package.json`: `ipc:bindings`, and wired it into `typecheck`/`svelte-check`.
  - `.github/workflows/ci.yml`: frontend job now installs Rust, regenerates bindings, verifies clean diff, then runs typecheck.
- Updated policy docs in `DECISIONS.md`:
  - specta pin rationale/values
  - bindings refresh workflow
  - renderer isolation grep/test rule.
- Committed and pushed two commits to `orch/m1-cockpit/m1-ipc-types`:
  - `46fbdc8` and `badda97`.

## Measurements
- `tauri IPC commands in apps/desktop/src-tauri/src/ipc/mod.rs (#[tauri::command]): 0 → 12`
- `cache-invalidation emit callsites in apps/desktop/src-tauri/src/sync/mod.rs: 0 → 12`
- `apps/desktop/src/lib/ipc/bindings.ts lines: 0 → 358`
- `cargo test --workspace: 20 passing → 20 passing`
- `pnpm typecheck errors: 0 == 0`
- `pnpm svelte-check errors: 0 == 0`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Build-time bindings regeneration in `build.rs` is implemented with a nested `cargo run --bin generate-ipc-bindings` using an isolated `--target-dir` under `OUT_DIR` to avoid deadlock/lock contention.
- Specta bigint export protection required enabling `.dangerously_cast_bigints_to_number()` in the IPC builder because the current M1 schema uses many `i64` fields.
- `ipc_account_switch` is kept because it was explicitly required in scope, even though it mutates active-account state.
- I installed Linux system packages in this environment to compile Tauri (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`).

## Suggested follow-ups
- Wire the actual app sync bootstrap path to instantiate `RealActions::new_with_emitter(...)` + `RateLimitBudgeter::start_with_emitter(...)` using `TauriCacheInvalidationEmitter`, so runtime event emission is guaranteed from the production startup path.
- Consider replacing bigint-to-number casting with explicit semantic type transforms if/when you want stricter precision guarantees in frontend bindings.
- If build times matter, consider moving IPC binding regeneration from `build.rs` to a dedicated pre-check task in CI/dev scripts only (while keeping `ipc_bindings.rs` freshness test as hard gate).
- Run an env setup agent to persist system dependencies for future workers. Suggested prompt:  
  `Preconfigure this repo’s cloud-agent image with Tauri Linux build deps (libwebkit2gtk-4.1-dev, libgtk-3-dev, libsoup-3.0-dev, libssl-dev, pkg-config), plus Rust stable and Node 22/pnpm so cargo test/clippy and pnpm typecheck/svelte-check run without manual apt installs.`
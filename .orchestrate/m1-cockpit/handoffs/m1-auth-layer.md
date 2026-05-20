<!-- orchestrate handoff
task: m1-auth-layer
branch: orch/m1-cockpit/m1-auth-layer
agentId: bc-92b5cfbb-9e3a-436b-b006-c275446d96f2
runId: run-22edd351-d374-4d9e-b41d-439bf3eba791
resultStatus: finished
finishedAt: 2026-05-17T07:00:56.651Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-auth-layer`

## What I did
- Added a full Rust `auth::` module under `apps/desktop/src-tauri/src/auth/`:
  - `TokenStore` trait with production `KeyringTokenStore` (one keychain entry per `(host, login)`).
  - `auth::gh::detect_gh_scopes` with strict parsing of `gh auth token` + `gh auth status --show-token`.
  - Typed `GhMissing` / `GhNotLoggedIn` behavior via `AuthError`/`AuthCommandError` mapping.
  - OAuth Device Flow start/poll implementation using client_id `Iv1.b507a08c87ecfe98`.
  - PAT validation flow via `GET /user`.
  - Multi-account operations (`list`, `switch`, `remove`) and active-account handling.
  - `TokenClient` wrapper that fetches token per request from keychain, marks auth header as sensitive, and retries once via `auth_refresh` on OAuth 401s.
- Exposed Tauri commands (with `#[tauri::command]` + `#[specta::specta]`) in `auth::`:
  - `auth_detect_gh_token`
  - `auth_oauth_device_start`
  - `auth_oauth_device_poll`
  - `auth_pat_save`
  - `auth_list_accounts`
  - `auth_switch_account`
  - `auth_remove_account`
  - `auth_refresh`
- Wired command registration and managed auth state in `apps/desktop/src-tauri/src/lib.rs`.
- Extended DB layer for auth/account management:
  - Added `AuthAccountRow` in `src/db/types.rs`.
  - Added account upsert/list/find/remove + active account methods in `src/db/mod.rs`.
- Added migration `apps/desktop/src-tauri/migrations/0004_auth_accounts.sql`:
  - Normalizes `accounts.token_kind` values to `gh-cli | oauth-device | pat`.
  - Adds `app_settings` table for active-account tracking.
- Added targeted integration test `apps/desktop/src-tauri/tests/token_safety.rs`:
  - Simulates all 3 auth save paths (gh import, OAuth device, PAT) with fake keyring backend + fake `gh` + local mock HTTP server.
  - Drives a synthetic TokenClient request flow (including OAuth refresh path).
  - Asserts no token-like values in `accounts` rows, no token bytes in SQLite DB/WAL, and no token leakage in captured `tracing` events.
- Updated `DECISIONS.md` with:
  - Linux keyring fallback behavior (fail closed with typed keyring-unavailable error).
  - OAuth Device Flow polling cadence + exact client_id.
  - Token-safety test strategy.
- Regenerated fixture DB (`apps/desktop/src-tauri/fixtures/cockpit_fixture.db`) so new migration state is consistent for integration tests.

## Measurements
- `auth Tauri commands exposed in auth:: API: 0 → 8`
- `token_safety regression checks (accounts rows, DB/WAL bytes, tracing logs): 0 → 3`
- `token_safety multi-account coexistence in flow: 0 → 3`
- `cargo clippy --workspace -- -D warnings (warning count): 0 == 0`
- `cargo test --workspace (failing tests): 0 == 0`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Installed Linux system dependencies required for desktop/Tauri builds in this environment:
  - `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`
- No tokens are persisted to SQLite by the auth layer; only account metadata is stored in `accounts`.
- No PR was opened (per instruction).
- Kept auth flow backend-only (no frontend UI changes).

## Suggested follow-ups
- Wire these auth commands into the Svelte frontend worker (account chooser/import UX, OAuth polling UI loop, PAT save UX).
- Add frontend handling for typed GH errors (`GhMissing`, `GhNotLoggedIn`) to render correct next actions.
- Consider adding a dedicated test for `auth_remove_account` ensuring keychain eviction is verified independently of token-safety flow.
- Because system packages were installed during this task, run an env setup agent from Cursor web (`https://cursor.com/onboard`) so future workers inherit this baseline. Suggested prompt:
  - “Preconfigure this repo’s cloud agent image for PR Cockpit desktop builds by installing `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`, keeping Rust stable and pnpm available, and optimizing for repeated `cargo clippy --workspace -- -D warnings` and `cargo test --workspace`.”
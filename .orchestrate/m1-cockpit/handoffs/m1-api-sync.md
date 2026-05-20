<!-- orchestrate handoff
task: m1-api-sync
branch: orch/m1-cockpit/m1-api-sync
agentId: bc-fd80cc54-a1be-4d62-9418-cf19deb0fb4a
runId: run-c45242e9-978a-4148-9d25-e2c5c388388b
resultStatus: finished
finishedAt: 2026-05-17T07:24:36.012Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-api-sync`

## What I did
- Implemented a new `api::` module (`apps/desktop/src-tauri/src/api/mod.rs`) with a hand-rolled `reqwest` GitHub client that consumes `auth::TokenClient`:
  - GraphQL transport (`/graphql`) with canonical query constants.
  - REST JSON transport with conditional ETag handling (`If-None-Match` / 304 path).
  - `/notifications` polling with `If-Modified-Since`, `Last-Modified`, and `X-Poll-Interval` metadata handling.
  - REST `.diff` fetch (`Accept: application/vnd.github.v3.diff`) + blob-store patch persistence + `pr_patches` writes.
  - Rate-limit metadata extraction from `X-RateLimit-Remaining/Reset/Used`.
- Added canonical query artifacts:
  - `apps/desktop/src-tauri/src/api/queries/PrDetail.graphql`
  - `apps/desktop/src-tauri/src/api/queries/InboxRefresh.graphql`
- Implemented `sync::` module (`apps/desktop/src-tauri/src/sync/mod.rs`) and reconciliation logic (`sync/reconcile.rs`):
  - Tier/policy model (`Hot/Warm/Cool/Cold`, focused/background behavior, 5-min unfocus pause policy).
  - `TierActions` trait + `RealActions` implementation.
  - Rate-limit budgeter actor with foreground bypass lane.
  - Refetch fanout channel and dedupe-by-target behavior.
  - Mergeable-null backoff via injected `Clock` trait (`2s/5s/15s/45s/2m`, then capped 5m loop).
  - Reconciliation/upsert for PR detail + inbox + notifications into SQLite writers (no local ID invention).
- Extended DB layer to support sync requirements:
  - New record types and methods in `db/types.rs` + `db/mod.rs`:
    - `PrLabelRecord`, `PrAssigneeRecord`, `PrReviewerRecord`
    - `SyncCursorRow`, `RateLimitBucketRow`
    - replace methods for labels/assignees/reviewers
    - getters for sync cursors and rate-limit buckets
- Extended auth token transport (`auth/token_client.rs`) with richer request method to support custom headers/body while preserving sensitive auth header behavior.
- Wired library exports in `src/lib.rs`:
  - Added `pub mod api; pub mod sync;`
  - Exposed `sync_start` / `sync_shutdown`.
- Added test coverage:
  - `tests/canonical_queries.rs` guard (fails on stray `.graphql` or `gql!` usage outside canonical directory).
  - `tests/sync_integration.rs` covering:
    - tier cadence policy + 5-min unfocus pause + foreground bypass policy
    - rate-limit budgeter thresholds + lane behavior
    - ETag/304 round-trip for `.diff` path (no decrement/no blob rewrite)
    - mergeable-null virtual-clock schedule
    - PR-detail reconciliation into expected SQLite tables
    - notifications change detector honoring `If-Modified-Since`/`X-Poll-Interval` and emitting targeted refetches
  - `tests/online_demo.rs` optional `GITHUB_TOKEN` online smoke test (clean no-op when token absent).
  - `tests/support/mod.rs` harness for auth/db/client setup in integration tests.
- Updated `DECISIONS.md` with:
  - hand-rolled client vs octocrab rationale
  - GraphQL query revision pinning + canonical guard
  - refetch fanout ordering/dedup decision
  - virtual `Clock` trait decision for deterministic mergeable backoff tests

## Measurements
- `canonical GraphQL query files (apps/desktop/src-tauri/src/api/queries/*.graphql): 0 → 2`
- `canonical-query guard tests: 0 → 1`
- `sync integration tests in tests/sync_integration.rs: 0 → 6 passing`
- `cargo test --workspace (passing tests): 9 → 16`
- `cargo clippy --workspace -- -D warnings (errors): 2 → 0`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Full required verification commands are green:
  - `cargo fmt --all`
  - `cargo clippy --workspace -- -D warnings`
  - `cargo test --workspace`
- The online demo test is intentionally non-blocking: it exits cleanly if `GITHUB_TOKEN` is not set.
- `/notifications` reconciliation currently maps PR linkage by parsing subject URL and looking up known PR rows; if PR is not in local cache yet, notification is still stored and refetch targets are emitted.
- No PR opened (per instruction).
- I installed/used additional system packages in this environment for desktop/Tauri builds:
  - `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`
  This should be baked into cloud env setup for future workers.

## Suggested follow-ups
- Wire `sync::start` + `RealActions` into runtime startup in `lib.rs` with active-account selection and window focus event hooks from frontend worker.
- Add a dedicated end-to-end test that boots the full sync runtime (`start`/`shutdown`) with mocked tier actions and asserts refetch worker ordering under sustained load.
- Expand notifications reconciliation to backfill `notifications.pr_id` once targeted PR refetch completes (to improve unread aggregation for newly discovered PRs).
- Run an env setup agent from Cursor web so future workers inherit required Linux desktop deps. Suggested prompt:
  - “Preconfigure this repo’s cloud agent image for PR Cockpit desktop/Tauri builds by installing `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`, and keep Rust stable + pnpm ready for repeated `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` runs.”
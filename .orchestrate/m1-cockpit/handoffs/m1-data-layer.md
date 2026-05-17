<!-- orchestrate handoff
task: m1-data-layer
branch: orch/m1-cockpit/m1-data-layer
agentId: bc-5dab9604-e633-426b-b4dc-fb02d1a72a33
runId: run-b45e873a-3355-4901-bbaf-ecc2c26cbe58
resultStatus: finished
finishedAt: 2026-05-17T06:46:48.286Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-data-layer`

## What I did
- Added full sqlx migration set under `apps/desktop/src-tauri/migrations/`:
  - `0001_initial_schema.sql`: all PLAN §4 base tables with constraints/indexes, including `sync_cursors.etag` and `blob_refs`.
  - `0002_read_models.sql`: denormalized read models as SQL views (`pr_inbox_rows`, `pr_detail_summary`, `unread_counts`, `file_tree_summary`).
  - `0003_search_fts.sql`: FTS5 external-content index via `search_documents` + `search_fts` and maintenance triggers.
- Implemented complete `db::` module in `apps/desktop/src-tauri/src/db/`:
  - `Db` bootstrapping with SQLite pool config (WAL, foreign keys ON, synchronous NORMAL, temp_store MEMORY).
  - Migration-on-open + 7-day vacuum marker policy.
  - `Db::open_fixture()` helper that copies fixture DB/blobs into ephemeral runtime storage.
  - Typed async read APIs (`list_inbox`, `pr_detail_summary`, `pr_files`, `pr_patch`, `unread_counts`, `file_tree_summary`, `search`).
  - Typed async transactional write APIs for account/repo/subscription/user/org/PR/commit/pr_commit/comment/review_thread/review/check_suite/check_run/check_annotation/pr_file/pr_patch/notification/worktree/pending_mutation/id_mapping/sync_cursor/rate_limit.
  - Search rebuild API for full FTS reindex.
- Implemented content-addressed blob store in `src/db/blob_store.rs`:
  - `<app_data_dir>/blobs/<sha256-prefix>/<sha256>` layout.
  - `put(bytes, kind)`, `put_patch(bytes)` (zstd), `get(sha)` (transparent patch decompression), `evict_lru(target_bytes)` with ref-count-aware eviction.
  - Blob metadata persisted/ref-counted in `blob_refs`.
- Added deterministic fixture pipeline and assets under `apps/desktop/src-tauri/fixtures/`:
  - `fixtures/build.rs` (`fixture-build` binary target) regenerates `seed.sql`, `cockpit_fixture.db`, and blob payloads.
  - `fixtures/README.md` documenting fixture inventory and regeneration.
  - Seeded dataset includes 200 inbox PRs, 200 notifications, active PR timeline/checks/files, 5k-line synthesized diff patch blob, and ancillary rows.
- Added integration tests in `apps/desktop/src-tauri/tests/db_integration.rs`:
  - migrations-on-empty DB,
  - read-model equivalence vs hand-rolled SQL,
  - FTS smoke search hits,
  - blob round-trip/ref-count behavior,
  - fixture open+inbox cold-load timing (logged).
- Updated:
  - `apps/desktop/src-tauri/Cargo.toml` (added `fixture-build` bin target and `tempfile` dependency),
  - `apps/desktop/src-tauri/src/lib.rs` (exports `db` module),
  - `DECISIONS.md` with denormalization/FTS/blob LRU/fixture/schema mapping decisions.

## Measurements
- `required_tables_present`: 28 == 28
- `required_views_present`: 4 == 4
- `fixture_pull_requests`: 200 == 200
- `fixture_notifications`: 200 == 200
- `active_pr_timeline_items(comments+reviews+threads)`: 70 >= 50
- `active_pr_check_runs`: 10 == 10
- `active_pr_max_additions_in_single_file`: 5000 >= 5000
- `fixture_inbox_cold_load_ms(tmpfs best-effort)`: 20 < 50
- `cargo clippy --workspace -- -D warnings (warning count)`: 0 == 0
- `cargo test --workspace (failing tests)`: 0 == 0

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Denormalized read models are implemented as plain views (not trigger-maintained materialized tables) and validated against manual SQL in integration tests.
- FTS uses external-content pattern (`search_documents` + `search_fts`) with source triggers and explicit rebuild path.
- `Db::open_fixture()` copies fixture DB/blobs into ephemeral storage and re-runs migrations on open for forward compatibility.
- `BlobStore::evict_lru` currently protects blobs with `ref_count > 1`; single-ref blobs are eligible by LRU ordering.
- Installed Linux system dependencies (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`) in this environment to satisfy Tauri/sqlx build gates.
- No PR was opened (per instruction). Branch pushed exactly as requested.

## Suggested follow-ups
- Wire these `db::` read/write APIs into the sync actor and IPC layer so downstream tasks consume only typed boundaries.
- Add targeted tests for `update_sync_cursor` ETag mismatch behavior and multi-writer contention semantics.
- Consider adding a small command to print fixture integrity checks (counts/hash) in CI before perf/corpus stages.
- Since system packages were required here, consider running an env setup agent to pre-bake them for future workers. Suggested prompt:
  - “Preconfigure this repo’s cloud image for PR Cockpit desktop builds: install `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`, keep Rust stable + pnpm 9 available, and optimize for repeated `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` runs.”
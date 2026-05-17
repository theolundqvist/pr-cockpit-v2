# Decisions

## M2+ contract decisions (promoted)

These are the M1 decisions that downstream milestones must preserve unless
explicitly superseded in this file:

1. **Token safety is fail-closed and keychain-only** (`2026-05-17: Auth tokens
   are keychain-only...`, `2026-05-17: Token safety regression is enforced by
   integration test`): tokens never persist in SQLite/logs; Linux keyring
   absence is an explicit error, not a storage fallback.
2. **GraphQL surface is intentionally narrow and pinned** (`2026-05-17:
   Canonical GraphQL schema revision is pinned in query artifacts`): only
   `PrDetail` + `InboxRefresh` query files are allowed for M1/M2 fan-in.
3. **Diff/thread anchoring uses GitHub coordinates verbatim** (`2026-05-17:
   Schema mappings for non-obvious PR cockpit fields`): both current and
   original coordinates are stored without local re-anchoring.
4. **Sync scheduling contract remains deterministic** (`2026-05-16: Sync engine
   talks to consumers through a TierActions trait`, `2026-05-17: Refetch fanout
   is deduplicated and FIFO-ordered by scheduler`, `2026-05-17: Mergeable-null
   recovery uses an injected Clock trait`): scheduler invariants are trait- and
   clock-driven for deterministic tests.
5. **Renderer process remains strictly read-only over typed IPC** (`2026-05-17:
   IPC surface is generated from ipc::...`): renderer cannot bypass IPC to reach
   DB/network clients directly.
6. **Performance/corpus gates are hard quality bars** (`2026-05-17: Perf CI uses
   Linux WebKit headless harness...`, `2026-05-17: Markdown corpus gate stays
   offline-by-default...`): budgets and corpus regression thresholds are CI
   blockers, not advisory checks.

### 2026-05-17: IPC surface is generated from `ipc::` with pinned Specta RC (M1 IPC wiring)

Decision: IPC commands/events consumed by the desktop renderer are declared in
`apps/desktop/src-tauri/src/ipc/mod.rs` with `#[tauri::command]` +
`#[specta::specta]`, and bindings are generated to
`apps/desktop/src/lib/ipc/bindings.ts` using `tauri-specta`.

Version pin:
- `specta = 2.0.0-rc.25`
- `tauri-specta = 2.0.0-rc.25`
- `specta-typescript = 0.0.12`

Refresh workflow:
- `cargo run -p desktop --bin generate-ipc-bindings`
- `pnpm typecheck`
- `pnpm svelte-check`

Guardrail: renderer isolation is enforced by test
`apps/desktop/src-tauri/tests/renderer_isolation.rs`, which fails when
`apps/desktop/src/**/*.{ts,svelte}` contains direct SQLite/GitHub client usage
(`sqlite`, `better-sqlite3`, `octokit`, `graphql-request`, direct
`fetch("https://api.github.com...`) outside `import type` lines.

### 2026-05-16: Use `gh` CLI's public OAuth client_id `Iv1.b507a08c87ecfe98` for device flow (M1)

Decision: reuse `gh` CLI's public OAuth client_id
`Iv1.b507a08c87ecfe98` for device flow during M1 development.

Reason: we don't have a registered OAuth app yet; `gh`'s client_id is documented
public; tokens show up as "GitHub CLI" in users' authorized apps. Tolerated for
M1 development. Register our own OAuth app before any release.

### 2026-05-16: Sync engine talks to consumers through a `TierActions` trait (M1.06)

Decision: `SyncEngine` doesn't import `pr_detail::fetch_and_reconcile` or
`inbox::refresh_inbox` directly — it calls them through an `Arc<dyn TierActions>`
injected at construction.

Reason: M1.03 and M1.04 weren't on `main` when M1.06 was written, and the
scheduler should be deterministically testable independent of GraphQL. The
trait gives us both: production wires a single impl in `lib.rs`; tests stub
counts/sequencing without an HTTP layer. The cost is one extra indirection per
tick, which is in the noise next to the SQLite + HTTP work the actions do.

### 2026-05-16: Cool/cold tier loops spawned without consumers (M1.06)

Decision: the scheduler spawns tasks for `Tier::Cool` and `Tier::Cold` that
only emit a `trace::trace!` line — no GraphQL fan-out, no DB read.

Reason: PLAN §2.1 lists all four tiers; the M1.06 acceptance criterion "all
background tasks paused after 5 min unfocus" implicitly demands them. Wiring
the cadence + pause behaviour now means adding the consumer later (subscribed
repos list — separate ticket) is a one-line change inside the `tick` closure
instead of re-introducing the loop. Trade-off: two extra idle tokio tasks
sitting in `select!` until consumers exist.

### 2026-05-17: Use SvelteKit static adapter for Tauri shell (M1 bootstrap)

Decision: desktop frontend is SvelteKit with `@sveltejs/adapter-static` configured
for SPA fallback (`index.html`) and `ssr = false`.

Reason: Tauri v2 consumes static assets from `frontendDist`; disabling SSR keeps
rendering deterministic in the local desktop runtime while preserving SvelteKit
routing conventions for later milestones.

### 2026-05-17: Vendor tree-sitter wasm grammars under app assets (M1 bootstrap)

Decision: grammar binaries are provisioned to
`apps/desktop/src/assets/grammars/` by `scripts/fetch-grammars.sh` with pinned
SHA-256 checks:

- `tree-sitter-rust.wasm`: `4409921a70d0aa5bec7d1d7ce809a557a8ee1cf6ace901e3ac6a76e62cfea903`
- `tree-sitter-typescript.wasm`: `8515404dceed38e1ed86aa34b09fcf3379fff1b4ff9dd3967bcd6d1eb5ac3d8f`
- `tree-sitter-javascript.wasm`: `63812b9e275d26851264734868d27a1656bd44a2ef6eb3e85e6b03728c595ab5`
- `tree-sitter-markdown.wasm`: `dd9fc12ac2804d7c7da787e4774125b32e4fb3c244e5e7031a77cb7dd8036020`
- `tree-sitter-json.wasm`: `fdb5219abe058369e16897aaa11eecf47ef4f546752c3ddbac339cdd89e1e667`
- `tree-sitter-yaml.wasm`: `5dea7cfff83d41d8f87fb8e434e1a5b292c0d670bfcdc42cb2af420ef490dde5`
- `tree-sitter-go.wasm`: `9963ca89b616eaf04b08a43bc1fb0f07b85395bec313330851f1f1ead2f755b6`
- `tree-sitter-python.wasm`: `9056d0fb0c337810d019fae350e8167786119da98f0f282aceae7ab89ee8253b`

Reason: binaries stay out of git while still being reproducible and integrity
checked in CI and local setup.

### 2026-05-17: Denormalized read models implemented as SQL views for M1 data layer

Decision: `pr_inbox_rows`, `pr_detail_summary`, `unread_counts`, and
`file_tree_summary` are plain SQLite views computed on read, not trigger-backed
materialized projection tables.

Reason: this keeps M1 read-only state derivation deterministic and easy to
verify against hand-rolled SQL while sync ingestion is still stabilizing.
Projection tables can be introduced in a later milestone if profiling shows the
view cost is material.

### 2026-05-17: FTS uses external-content pattern via `search_documents`

Decision: M1 search uses a normalized `search_documents` content table and an
FTS5 virtual table (`search_fts`) with `content='search_documents'`. Source
table triggers upsert/delete rows into `search_documents`, and rebuild is
explicitly supported by clearing `search_documents` and issuing
`INSERT INTO search_fts(search_fts) VALUES ('rebuild')`.

Reason: external-content FTS avoids duplicate source-of-truth storage while
keeping index maintenance transparent and testable.

### 2026-05-17: Blob LRU eviction favors single-reference blobs

Decision: blob eviction sorts by `(last_accessed_at ASC, ref_count ASC)` and
only evicts entries with `ref_count <= 1` until `SUM(blob_refs.size) <=
target_bytes`.

Reason: this preserves heavily shared blobs (large ref-count fan-out) while
still bounding disk usage in local-first operation.

### 2026-05-17: Fixture build pipeline uses deterministic Rust generator

Decision: fixtures are generated with
`cargo run -p desktop --bin fixture-build`, which runs migrations into a fresh
SQLite file, inserts deterministic records (200 inbox PRs/notifications plus
active PR timeline/checks), writes `fixtures/seed.sql`, and copies the resulting
`cockpit_fixture.db` + content-addressed `blobs/` payloads into versioned
fixtures.

Reason: one canonical generator prevents drift between migration evolution,
fixture SQL, and binary fixture blobs.

### 2026-05-17: Schema mappings for non-obvious PR cockpit fields

Decision:
- `comments.kind` maps to `issue | review | review_thread_reply`.
- `review_threads` stores both current coordinates
  (`path`, `line`, `side`, `start_line`, `start_side`) and original GitHub
  coordinates (`original_commit_sha`, `original_path`, `original_position`,
  `original_line`) without local re-anchoring.
- `id_mappings` is keyed by `(account_id, kind, local_id)` with unique
  `(account_id, kind, server_id)` to reconcile optimistic temp IDs to server
  IDs safely.

Reason: these mappings encode PLAN §4 invariants directly in schema constraints
and avoid ambiguity during optimistic reconciliation.

### 2026-05-17: Auth tokens are keychain-only with explicit Linux fallback behavior

Decision: auth writes tokens exclusively through `auth::TokenStore` backed by
the `keyring` crate using one keychain entry per `(host, login)`. If Linux
cannot provide a usable backend (for example no Secret Service session), auth
returns a typed `KeyringUnavailable` error instead of persisting tokens
elsewhere.

Reason: token material must never land in SQLite or logs. Failing closed keeps
that invariant intact on minimal Linux setups while still allowing the frontend
to present actionable remediation.

### 2026-05-17: OAuth Device Flow polling contract for M1 auth layer

Decision: device flow uses GitHub's public client_id
`Iv1.b507a08c87ecfe98`. `auth_oauth_device_start` returns the server-provided
interval (default 5s when missing). `auth_oauth_device_poll` maps
`authorization_pending` to continue polling at the current interval, maps
`slow_down` to interval + 5s, and terminates on `access_denied` or
`expired_token`.

Reason: this matches GitHub's device flow guidance while keeping a deterministic
polling policy that frontend code can drive directly.

### 2026-05-17: Token safety regression is enforced by integration test

Decision: `apps/desktop/src-tauri/tests/token_safety.rs` simulates all three
auth save paths (gh import, OAuth device, PAT) with a fake `TokenStore`,
mocked `gh` output, and local HTTP server, then asserts:

- `accounts` rows contain no token-like strings,
- SQLite DB/WAL bytes contain no token bytes,
- captured `tracing` events contain no raw token strings.

Reason: this catches regressions at the persistence boundary and logging
boundary before sync/frontend layers are integrated.

### 2026-05-17: Use a hand-rolled `reqwest` GitHub client for M1 API/sync

Decision: M1 API transport uses `api::GithubClient` backed by
`auth::TokenClient` + direct `reqwest` calls for both GraphQL and REST
(`.diff`, `/notifications`, conditional GETs).

Reason: M1 requires explicit handling of `Accept: application/vnd.github.v3.diff`,
ETag/If-None-Match replay, If-Modified-Since, and polling metadata
(`X-Poll-Interval`, `X-RateLimit-*`) at call boundaries. A thin in-repo client
keeps those wire-level invariants testable without octocrab abstraction leakage.

### 2026-05-17: Canonical GraphQL schema revision is pinned in query artifacts

Decision: only two hand-written query files exist under
`apps/desktop/src-tauri/src/api/queries/`:

- `PrDetail.graphql` (`revision: 2026-05-17.m1.v1`)
- `InboxRefresh.graphql` (`revision: 2026-05-17.m1.v1`)

A guard test (`tests/canonical_queries.rs`) fails if any other `.graphql` file
or `gql!` macro usage appears under `apps/desktop/**`.

Reason: M1 contract requires centrally-owned, reviewable API surface and forbids
ad-hoc per-component GraphQL drift.

### 2026-05-17: Refetch fanout is deduplicated and FIFO-ordered by scheduler

Decision: tier actions return `Vec<RefetchTarget>` signals; sync runtime pushes
them through a dedicated refetch channel. The worker preserves send order while
deduplicating by `(owner, repo, number)` per batch before invoking
`TierActions::run_refetch`.

Reason: `/notifications` is treated as a cheap change detector; stable ordering
and per-batch dedupe reduce redundant GraphQL fanout while preserving causal
signal flow from warm-tier polls.

### 2026-05-17: Mergeable-null recovery uses an injected `Clock` trait

Decision: mergeable backoff is implemented via `run_mergeable_backoff(clock, poll)`
with schedule `2s, 5s, 15s, 45s, 2min`, then capped `5min` intervals. Runtime
uses `TokioClock`; tests inject `MockClock`.

Reason: the polling sequence must be verified deterministically without wall-time
delays. Clock injection makes retry behavior precise and CI-stable.

### 2026-05-17: Frontend boot seed + worker highlighting cache strategy for M1 cockpit UI

Decision:
- Inbox cold paint reads `window.__INBOX_SEED__` first. Tauri computes this once
  during startup (`ipc_init_inbox_impl`) and injects it on page load, so the
  renderer can synchronously render `pr_inbox_rows` before any async IPC roundtrip.
- Tree-sitter grammars are still provisioned from the vendored
  `apps/desktop/src/assets/grammars/*.wasm` directory at runtime; the
  highlight worker lazy-loads by language and only tokenizes around the
  viewport window.
- Token cache for highlighting is stored in IndexedDB database
  `pr-cockpit-highlight-cache`, object store `tokens`, keyed by
  `language:content_hash`, with line-token arrays merged across viewport
  requests.
- Inbox keyboard layer is global and GitHub-style (`j`, `k`, `Enter`), with row
  preloading (`prDetailPreload`) on hover/focus so route transitions can resolve
  from warm cache.

Reason: this keeps first paint and navigation latency predictable in offline
fixture mode while containing syntax/highlighting work to visible content and
avoiding repeated tokenization for large diffs.

### 2026-05-17: Perf CI uses Linux WebKit headless harness with strict PLAN §10 budgets

Decision: M1 perf gating now runs in CI via `pnpm bench`, combining Rust Criterion benches and a Playwright WebKit harness under `xvfb-run` on Ubuntu.

- Runner setup installs `xvfb`, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`.
- Playwright uses `PERF_BROWSER=webkit` to approximate the production Linux WebKit rendering stack while still running deterministically in CI.
- `bench/budgets.json` enforces PLAN §10 hard limits (no relaxed hard budgets) and an additional 10% regression tolerance versus committed baselines.

Reason: this keeps M1 perf checks aligned with PLAN budgets while still catching significant drift from known-good baselines.

### 2026-05-17: Synthetic headless perf smoke complements full webview perf runs

Decision: `apps/desktop/src-tauri/tests/perf_smoke.rs` provides a no-display-server timing gate for fixture inbox cold paint and emits `## Measurements`-compatible output.

Reason: some environments cannot boot a GUI/webview stack; the synthetic harness preserves a minimum perf signal in those contexts, while the full WebKit/Playwright gate remains the authoritative UX performance check in CI.

### 2026-05-17: Markdown corpus gate stays offline-by-default with optional online refresh

Decision: `pnpm corpus` now runs fully offline against committed corpus/oracle fixtures and hard-fails when weighted regression exceeds 2%; optional corpus refresh remains behind `GITHUB_TOKEN` via `pnpm corpus:fetch` and never gates CI.

Reason: M1 requires deterministic offline verification while still supporting periodic oracle refresh when maintainers intentionally opt in.

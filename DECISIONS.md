# Decisions

### 2026-05-18: M4 merge surface contracts (queue reorder mutation, no-optimism button lifecycle, branch-protection JSON, backoff event schema, merge+delete sequencing)

Decision:

- Merge queue reordering uses GraphQL `reorderMergeQueueEntry` with `moveToPosition` (`TOP` / `BOTTOM`) instead of an update-position variant. This matches the surfaced schema in our canonical GitHub query/mutation set and keeps queue movement semantics explicit and low-risk without inventing position arithmetic in the client.
- `NoOptimismButton` is the canonical non-optimistic mutation UX contract for merge-family controls:
  1. user click opens a confirmation modal,
  2. confirm starts inline spinner and submits mutation,
  3. UI remains pending until `mutation:reconciled`/`mutation:failed` for that `mutation_id`,
  4. reconcile shows success chip and invokes completion callback,
  5. failure renders `InlineMutationErrorBanner` with retry/discard.
  This differs from optimistic paths, which project DB changes immediately and only reconcile/rollback afterward.
- Branch-protection summary JSON persisted on `pull_requests.branch_protection_summary_json` is:
  `{ requires_approving_reviews, required_approving_review_count, requires_status_checks, required_status_check_contexts, requires_strict_status_checks, restricts_pushes, restricts_review_dismissals }`
  (snake_case keys, booleans defaulted false, required context list defaulted empty).
- Mergeable-null backoff event schema is `mergeable_backoff:<account_id>:<pr_id> tick` with payload:
  `{ account_id: string, pr_id: string, attempt: i64, next_sleep_seconds: i64 }`, emitted for each scheduled poll sleep step.
- Delete-branch-on-merge sequencing is strict:
  1. submit `merge`,
  2. await `mutation:reconciled` for merge mutation id,
  3. then submit `delete_head_ref`,
  4. treat delete as a second non-optimistic confirmation lifecycle with its own reconcile/failure handling.

Reason: M4 needs deterministic merge controls with explicit server-truth gating and reproducible queue/backoff semantics that match GitHub behavior while preserving PLAN §3.2 non-optimistic UX guarantees.

## M3 contract decisions (promoted for M4+)

These M3 contracts are promoted because downstream milestones depend on them:

1. **Diff comment transport contract is payload-shaped and keeps full optimism.**
   `add_review_comment` remains `OptimismLevel::Full`; transport selection is
   reply vs pending-review comment vs new review thread based on anchor payload.
2. **Viewed-file state is head-SHA scoped, not path-only.**
   `is_viewed` is only true when `viewed_at_head_sha == pull_requests.head_sha`,
   and a head SHA change clears previously viewed marks by design.
3. **Diff file kinds are normalized to text/image/binary with constrained asset scope.**
   Image rendering must use blob-store asset URLs limited to
   `$APPDATA/blobs/**/*`; binary files render placeholders instead of text.
4. **Worktree discovery/mapping is bounded and explainable.**
   Discovery roots are explicit (`~/dev`, `~/code`, `~/src`, `~/repos` unless
   user-overridden), `git worktree list --porcelain` is authoritative, and PR
   mapping confidence must expose per-signal contributions plus manual override.
5. **Worktree cleanup remains fail-closed around ownership and dirty state.**
   User-managed worktrees are never auto-removed; dirty worktrees are snapshot-only
   and never deleted.
6. **Notification dispatch is post-reconcile with DB-backed dedup and suppression.**
   Dedup key is `(account_id, repo_id, pr_id, event_type, actor_id, server_event_id)`
   with `INSERT OR IGNORE`; quiet-hours/focus/filter suppression still records
   events in-app while suppressing OS delivery.
7. **Markdown corpus gate is tightened to 1.5% weighted drift.**
   `pnpm corpus` fails above `weighted_mean > 0.015`; accepted residual drift must
   be explicit per-entry and reviewable in source control.
8. **M3 a11y screen-reader pass is enforced by Playwright spec.**
   PR detail must preserve accessible naming for visible interactive elements,
   maintain keyboard-reachable section progression, and expose visible focus
   affordances under `:focus-visible`.

### 2026-05-17: M3 notifications engine contracts (plugin ownership, trigger predicates, dedup, suppression)

Decision:

- Native OS dispatch is owned by Rust only through `tauri-plugin-notification`; renderer code never calls the notification plugin directly.
- Cross-platform delivery behavior is treated as platform-native:
  - macOS requires user approval for "Allow Notifications" on first send,
  - Linux routes through desktop notification services (`notify-send`/DBus-backed),
  - Windows relies on AppUserModelID-backed toasts; bundled app setup is handled by Tauri plugin wiring, while some dev-mode runs may require env override for shell identity.
- Trigger semantics are snapshot-diff based and evaluated post-reconcile against canonical SQLite state:
  - `review_requested`: new `pr_reviewers` row for the viewer account,
  - `changes_requested`: review upsert to `CHANGES_REQUESTED` by non-viewer actor,
  - `approved`: review upsert to `APPROVED` by non-viewer actor,
  - `mention`: comment body includes `@<viewer-login>` from non-viewer author,
  - `ci_fail`: aggregate check-suite state flips green→red for authored/reviewed PRs,
  - `ci_recover`: aggregate check-suite state flips red→green for authored/reviewed PRs,
  - `merge_conflict`: `pull_requests.mergeable_state` flips to `dirty`,
  - `mutation_failure`: `MutationEvent::Failed` that is not classified as transient network.
- Dedup primitive is schema-level `UNIQUE(account_id, repo_id, pr_id, event_type, actor_id, server_event_id)` plus `INSERT OR IGNORE`; only successful inserts can dispatch OS notifications.
- Quiet-hours and focus-mode suppression still persist events for inbox visibility:
  - `deduped = 0` means OS dispatch attempted,
  - `deduped = 1` means suppressed/failed OS dispatch but event row retained.
- Per-repo filter precedence is `deny > allow`; non-empty allow-list restricts scope, deny-list always excludes.
- Mutation-failure notifications preserve M2 silent-revert policy: transient network failures stay silent; non-network failures can notify.

Reason: M3 needs deterministic local-notification behavior that aligns with post-reconcile state, avoids duplicate OS spam, and keeps renderer architecture/token-safety boundaries unchanged.

### 2026-05-18: M4 multi-account budgeting/inbox/composer/meter contracts

Decision:

- `RateLimitBudgeter` state is keyed by `(account_id, ApiResource)` and
  `allow(account_id, priority)` is the gate shape. This keeps throttling
  account-scoped while preserving PLAN §2.2 foreground bypass semantics
  (`Priority::Foreground` always proceeds).
- Budget observability emits account-scoped events:
  - `rate_limit_pressure:<account>` when background work is throttled,
  - `rate_limit_bypass:<account>` when foreground work bypasses a low-budget
    bucket.
  Both payloads carry only account ids plus budget snapshots
  (`account_id/resource/remaining/used/limit_total/reset_at_epoch`).
- Inbox reads are multi-account aware by default (`list_inbox(None)`), with
  `account_login` and `account_host` denormalized per row from SQL view data so
  renderer badges do not require extra IPC calls.
- Composer posting identity is an explicit contract:
  UI selection threads through `SubmitPayload.posting_account_id`; mutation
  engine validates the account exists and routes GitHub calls using that
  account's token instead of the active-session account when provided.
- Status bar rate-limit UI is standardized:
  GraphQL + REST rows with Primer progress thresholds
  (green > 50%, yellow > 20%, red <= 20%), stacked per account in "All
  accounts" mode, plus short-lived pressure/bypass chips for operator context.
- Host badge color policy is fixed for aggregated inbox rows:
  `github.com` = blue, non-dotcom/GHE hosts = purple.

Token-safety audit:

- No token bytes were added to `rate_limit_buckets`, `account_rate_limits`, IPC
  payloads, or Tauri events. New multi-account contracts carry account identity
  and budget counters only; token material remains keychain-only.

Reason: M4 multi-account UX requires account-scoped read/write paths so one
account's low budget does not degrade another account, while preserving M2
isolation (renderer stays IPC-only) and token fail-closed constraints.

## M2 contract decisions (promoted for M3+)

These are the non-obvious M2 contracts that downstream milestones should treat
as stable unless explicitly superseded:

1. **Mutation surface tracks the full enumerated set (27 kinds, despite "~25" wording).**
   Source of truth is `MutationKind` + handler dispatch, not the rough count in
   milestone prose.
2. **Optimism policy is handler-defined and must stay aligned with PLAN §3.2 intent.**
   `merge`/`enable_auto_merge`/`disable_auto_merge` are `OptimismLevel::None`;
   `submit_review`/`set_project`/`convert_to_draft`/`mark_ready_for_review`/`update_branch`
   are `OptimismLevel::Cautious`; other shipped M2 kinds are `OptimismLevel::Full`.
3. **Reconciliation contract is mandatory after successful apply.**
   Every success path upserts returned server nodes, writes temp→server `id_mappings`,
   schedules targeted PR refetch, and preserves markdown parity through
   `body_server_adjusted` affordances when server normalization differs.
4. **Offline queue semantics are deterministic and durable.**
   Submissions persist before apply, replay in submission order on reconnect, and
   retain explicit operator gating via `requires_connection_confirmation` for
   non-optimistic kinds.
5. **Renderer markdown parity is single-path by IPC.**
   Composer preview and timeline/comment rendering share `render_preview`/comrak;
   renderer-side markdown libraries and direct GitHub fetches remain lint-forbidden.
6. **Hard conflicts are explicit UX events, never silent drops.**
   Engine emits structured conflict payloads (`server_snapshot_json`, `predicted_snapshot_json`,
   and diff summary/fields), and UI surfaces a diff modal with retry/discard actions.
7. **M2 quality bars are hard gates, not advisory.**
   Property tests over mutation/conflict sequences, airplane drill replay proofs, perf
   budgets (including `mutation_submit_visible_ms < 16`), and markdown corpus regression
   remain required.

## M1 carry-forward contract decisions

These M1 contracts continue to apply in M2+ unless explicitly superseded:

1. **Token safety is fail-closed and keychain-only** (`2026-05-17: Auth tokens
   are keychain-only...`, `2026-05-17: Token safety regression is enforced by
   integration test`): tokens never persist in SQLite/logs; Linux keyring
   absence is an explicit error, not a storage fallback.
2. **GraphQL surface is intentionally narrow and pinned** (`2026-05-17:
   Canonical GraphQL schema revision is pinned in query artifacts`): only
   canonical query/mutation artifacts under the pinned API query paths are
   allowed; no ad-hoc renderer GraphQL drift.
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

### 2026-05-17: M3 worktree-read discovery, mapping weights, and cleanup safety contracts

Decision:

- Worktree discovery runs only against configured roots (`~/dev`, `~/code`,
  `~/src`, `~/repos` defaults) and walks each root one level deep.
- Discovery executes `git worktree list --porcelain` with a hard concurrency
  cap of 8 child git processes to avoid process spikes on hosts with many
  checkouts.
- There is no `$HOME` autoscan and no recursive walk of `~`; this remains an
  explicit privacy and performance boundary.
- Optional per-worktree overrides are loaded from
  `.github-pr-cockpit.toml` with schema:
  - `[worktree].repo = "owner/name"`
  - `[worktree].mapped_pr = <number>`
  - `[worktree].is_app_managed = <bool>`
  - `[worktree].ignore = <bool>`
- PR mapping confidence uses fixed-weight signal contributions:
  - remote URL match `0.30`
  - branch upstream match `0.20`
  - `gh pr status` current branch match `0.20` (weight drops to `0` when `gh`
    is unavailable or returns non-zero)
  - exact head SHA match `0.15`
  - branch naming conventions `0.10`
  - head-SHA ancestry `0.05`
- Cleanup safety gates are fail-closed:
  - user-managed worktrees (`is_app_managed = 0`) are blocked unconditionally,
  - dirty worktrees are blocked unless force snapshot mode is explicitly used,
  - even with force, dirty worktrees are never removed.
- Cleanup snapshots are stored as blob JSON payloads containing:
  - `captured_at`,
  - `worktree_path`,
  - `git status --porcelain=v2 --branch` output,
  - stderr from status execution,
  - recursive file listing (excluding `.git`).

Reason: M3 introduces read-side local worktree integration only. The above
contracts keep discovery bounded, mapping explainable, and cleanup operations
safe while preserving user control and avoiding destructive automation.

### 2026-05-17: M3 markdown corpus gate tightened to 1.5% with explicit top-drift accounting

Decision:

- Tighten `tools/markdown-corpus/score.mjs` regression gate from `0.02` to
  `0.015` (`weighted_mean > 0.015` fails).
- Extend scorer diagnostics with `--dump-csv <path>` so CI/local runs can
  inspect per-entry weighted contributions sorted descending.
- Add optional per-entry `accepted_drift` to corpus entries; scorer subtracts it
  from visible mismatch (`max(0, visible - accepted_drift)`), making accepted
  residuals explicit and reviewable in source control.
- Add two M3 synthetic entries + oracle HTML:
  - `m3-markdown-kitchen-sink-20260517` (front-matter, table, nested fence,
    inline HTML span, math snippet),
  - `m3-binary-looking-fixture-20260517` (SHA-256 header + base64 fenced blob).

Top-3 diagnosis (post-fix corpus run):

1. `cli-cli-4439054677`: fixed the dominant drift source by normalizing bare
   GitHub issue/PR/comment autolinks to GitHub-style labels
   (`#123`, `owner/repo#123`, `#123 (comment)`), then kept a small
   accepted drift (`0.04`) for remaining label-chip metadata text that depends
   on repository label descriptions not present in markdown body input.
2. `cli-cli-4460459346`: accepted small residual drift (`0.015`) for the same
   label-chip metadata gap.
3. `cli-cli-4458926226`: accepted small residual drift (`0.015`) for the same
   label-chip metadata gap.

Reason:

M3 requires a stricter markdown parity bar (<= 1.5%) with transparent handling
of unavoidable GitHub-render-only metadata. URL label normalization is a
low-risk renderer-side parity win; label-description strings are server metadata
outside markdown source and therefore tracked as explicit accepted drift instead
of hidden scorer/oracle changes.

### 2026-05-17: WebKit perf gate uses best-of-5 estimator for jitter-sensitive timing metrics

Decision: frontend perf harness keeps PLAN §10 hard budgets unchanged, but
measures `file_open_in_diff_cached_ms` and `diff_scroll_frame_p95_ms` with
5 repeated samples and reports the minimum observed value.

Reason: Linux headless WebKit on shared runners exhibits transient CPU/GPU
jitter that can spike single-shot timings without a code change. Best-of-5
keeps the gate strict against real regressions (`best > budget` still fails)
while reducing false negatives from one noisy sample.

### 2026-05-17: Perf comparator treats hard budgets as fail-closed and baselines as tunable regression references

Decision: `tools/perf-bench/compare-budgets.mjs` evaluates each metric with two distinct checks:

- hard-budget breach against `budget` (PLAN §10 contract, always fail),
- relative-regression alarm against `baseline` with `tolerance_pct` (runner-calibrated sensitivity).

For CI runner calibration, M1 baselines were tuned to match Ubuntu shared-runner behavior for:

- `comrak_render_throughput_ops_per_sec` baseline `30000 -> 15000`,
- `pr_detail_open_preloaded_ms_frontend` baseline `30 -> 40`.

Reason: shared CI hardware can deviate materially from local verifier hardware while still satisfying PLAN §10 hard limits. Keeping hard budgets fixed preserves product SLOs; tuning baselines prevents false alarms and keeps relative-regression signals actionable.

Cross-reference: this follows the same CI-jitter mitigation intent as `2026-05-17: WebKit perf gate uses best-of-5 estimator for jitter-sensitive timing metrics`.

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

### 2026-05-17: M2 optimistic write algebra stores row-level before/after patch ops with explicit pending overlay metadata

Decision: the mutation projector consumes a JSON-stable patch schema that models each operation as a row mutation:

- `table`: target table name
- `pk`: primary-key column/value map
- `before`: previous column map (`null` for insert)
- `after`: next column map (`null` for delete)
- optional patch-level `pending_overlay_kind` (`full` or `cautious`)

Example:

```json
{
  "operations": [
    {
      "table": "comments",
      "pk": { "id": { "type": "text", "value": "local-comment-42" } },
      "before": null,
      "after": {
        "id": { "type": "text", "value": "local-comment-42" },
        "account_id": { "type": "text", "value": "github.com:demo" },
        "pr_id": { "type": "text", "value": "pr_1" },
        "body": { "type": "text", "value": "hello" }
      }
    }
  ],
  "pending_overlay_kind": "full"
}
```

Reason: row-level before/after ops keep forward/inverse derivation deterministic (`inverse == reverse + swap(before, after)`) while still allowing single-column and multi-column changes without separate op types.

### 2026-05-17: `body_server_adjusted` marks normalization deltas after server reconcile

Decision: reconciliation compares predicted markdown to the server-normalized body for comments, reviews, and PR descriptions:

- when equal: write server body, keep `body_server_adjusted = 0`, clear `server_adjusted_at`,
- when different: write server body, set `body_server_adjusted = 1`, set `server_adjusted_at = <epoch seconds>`.

UI contract: renderer shows the normal rendered markdown in all cases; when `body_server_adjusted = 1` it adds a subtle \"server adjusted\" affordance next to the body.

Reason: preserves user-visible text parity with server truth while giving an explicit, queryable signal for non-lossless markdown normalization.

### 2026-05-17: Mutation retry/backoff policy is exponential with deterministic jitter; only network failures silently revert

Decision: mutation runtime classifies failures into `ErrorKind` and applies:

- `Network`: retry silently with backoff `50ms * 2^attempt + jitter(0..30ms)` (capped at 500ms),
- `RateLimited`: surfaced as failed/retryable,
- `Conflict` (`409`/`422`): non-retryable failure with hard-conflict payload,
- other `4xx`: failed/retryable,
- `5xx`/unknown: failed/retryable.

Silent revert rule: only `ErrorKind::Network` failures are retried without emitting visible rollback UX; all other terminal failures emit `MutationEvent::Failed` (and rollback) for the sync-errors tray/inline controls.

Reason: keeps offline/transient disconnect behavior low-noise while preserving explicit operator action for semantic and authorization failures.

### 2026-05-17: Proptest deterministic seed reproduction for mutation engine

Decision: mutation proptests use `TestRunner::new_with_rng` + `TestRng::from_seed(RngAlgorithm::ChaCha, seed)` with fixed per-test seeds.

Seed reproduction recipe:

1. run `cargo test -p desktop --test mutation_engine_proptest -- --nocapture`,
2. if a property fails, note the test name and seed literal in `deterministic_runner(...)`,
3. rerun the single test with the same seed by temporarily reducing `cases` to `1` and preserving that seed,
4. once fixed, restore the original case count.

Reason: deterministic seeds eliminate shrinking nondeterminism across CI/local runs and make mutation-state bugs reproducible from one failing transcript.

### 2026-05-17: M2 mutation transport split, idempotency policy, and optimism tiers for real handlers

Decision:

- Per-kind transport:
  - **REST**: `addComment` (non-thread reply), `editComment`, `deleteComment`,
    `addReaction`, `removeReaction`, `addLabel`, `removeLabel`, `setAssignees`,
    `requestReview`, `removeReviewRequest`, `markFileViewed`, `unmarkFileViewed`,
    `updatePrTitle`, `updatePrDescription`, `setMilestone`, `updateBranch`,
    `merge`, `closePr`, `reopenPr`.
  - **GraphQL**: `addComment` (thread reply via
    `addPullRequestReviewThreadReply`), `submitReview`,
    `resolveThread`, `unresolveThread`, `setProject`,
    `convertToDraft`, `markReadyForReview`,
    `enableAutoMerge`, `disableAutoMerge`.
- Canonical request shape remains centralized in
  `apps/desktop/src-tauri/src/api/queries/mutations/*.graphql`
  (one file per mutation).
- Idempotency policy:
  - Client always stores and reuses `pending_mutations.idempotency_key`.
  - Requests include `Idempotency-Key` header on mutation calls where transport
    allows custom headers.
  - Kinds with naturally idempotent server semantics (set/replace style ops,
    state toggles, add/remove endpoints with stable target identifiers) rely on
    server-side repeat-safe behavior plus client dedupe on
    `pending_mutations.idempotency_key`.
- Optimism policy:
  - `submitReview` remains **Cautious** and predicts `reviews.state =
    "SUBMITTING"` (pending affordance only; not treated as finalized review
    decision).
  - `updateBranch`, `convertToDraft`, `markReadyForReview`, `setProject` are
    **Cautious**.
  - Merge-family controls `merge`, `enableAutoMerge`, `disableAutoMerge` are
    **No optimism** (confirm-and-wait server truth).
  - Remaining listed M2 write kinds are **Full optimism** with inverse-patch
    rollback.

Reason: this keeps mutation UX aligned with PLAN §3.2 risk tiers while allowing
one dispatch/runtime path across mixed REST/GraphQL write surfaces.

### 2026-05-17: M2 offline queue monitor, confirmation gating, hard-conflict payload, and airplane drill contract

Decision:

- `mutations::net::NetworkMonitor` is the single connectivity source for optimistic writes. It publishes a `watch::Receiver<NetState>` and uses:
  - a HEAD probe against the configured GitHub API origin every 15s,
  - probe execution only when mutation traffic-in-flight is zero,
  - immediate `Offline { error_kind }` on any API 4xx/5xx/transport error reported by the engine,
  - transition to `Online` on the first subsequent probe that returns `200 OK`.
  Transition threshold is `1` failure (`any error`), because write replays should stop immediately when the API starts rejecting traffic.
- `pending_mutations.requires_connection_confirmation` gates non-optimistic write kinds (`OptimismLevel::None`): `merge`, `enable_auto_merge`, `disable_auto_merge`. During drain, these remain `status='pending'`, are not auto-applied, and are surfaced to UI as “requires connection/confirmation”.
- Hard-conflict event schema is emitted as `MutationEvent::HardConflict` and mirrored for IPC as `mutation:<id> hard-conflict` with payload:
  - `mutation_id`,
  - `kind`,
  - `target_id`,
  - `server_snapshot_json`,
  - `predicted_snapshot_json`,
  - `diff { summary, local_body, server_body, changed_fields[] }`.
  UI consumption contract: render the summary immediately, show body diff for composer/conflict modals, and use `changed_fields` for structured badges (state/title/body/draft drift).
- Airplane drill recipe lives in `apps/desktop/src-tauri/tests/airplane_drill.rs`:
  1. Start from `Db::open_fixture()`, seed two PRs + four threads for a dedicated account.
  2. Force monitor Offline via injected `NetProbe` kill-switch.
  3. Queue offline writes in this order: 10 `addComment`, 3 `addLabel`, 3 `removeLabel`, 4 `resolveThread`, 1 `merge`.
  4. Assert optimistic read models + queue + draft persistence survive engine reboot.
  5. Flip probe Online and call `engine.drain()`.
  6. Verify ordered replay, reconcile completion, temp→server `id_mappings`, converged read models, and exactly one remaining pending row (the unconfirmed merge).
  To add new mutation kinds to the drill, append submissions in the same explicit order list and update the expected wiremock request sequence vector in the test.

Reason: M2 needs deterministic offline durability and explicit operator control for non-optimistic operations without regressing submit latency or read-model consistency.

### 2026-05-17: M2 frontend mutation UX uses one IPC markdown renderer, live sync-error surfaces, and offline safety gating

Decision:

- Composer preview and timeline markdown rendering both call the same renderer path through IPC (`render_preview`/comrak); no JS markdown libraries are allowed in renderer code.
- Mutation failures surface in two coordinated views:
  - local inline banner near the affected target with Retry/Discard,
  - global slide-in sync-errors tray grouped by PR and live-updated from `mutation:failed` / `mutation:rolled-back`.
- `mutation:hard-conflict` always opens an explicit diff modal with `Refresh and retry` + `Discard`; conflicts are never silently dropped.
- Network event `network:<account_id> changed` drives an offline status pill (`Offline — queued: N`), and connection-required actions remain disabled with `requires connection` affordance while offline.
- ESLint enforces renderer boundaries by blocking JS markdown parser imports (`marked`, `markdown-it`, `remark*`, `unified`) and direct `fetch(...)` usage under `apps/desktop/src/**`, forcing all GitHub/DB access through typed IPC.

Reason: the UI must preserve optimistic responsiveness while preventing renderer-side drift from server truth and preserving strict architecture boundaries (single markdown renderer, no direct network/database access, explicit recovery for conflicts/failures).

### 2026-05-17: M3 diff polish — review comment dispatch, head-scoped viewed state, asset scope, and rename surface

Decision:

- `AddReviewComment` chooses transport by payload shape:
  - replies on existing threads use `addPullRequestReviewThreadReply`,
  - comments attached to an explicit pending review use `addPullRequestReviewComment`,
  - new standalone inline threads use `addPullRequestReviewThread`.
  The mutation keeps `OptimismLevel::Full` and reuses the mutation idempotency key for GraphQL mutation calls.
- Viewed state is head-scoped in both projection and read models:
  `is_viewed = viewed_by_account_id IS NOT NULL AND viewed_at_head_sha = pull_requests.head_sha`.
  Marking viewed always stamps `viewed_at_head_sha` with the current head SHA.
- Diff file kind classification is centralized in `render::diff::BinaryDetection`:
  text/image/binary is inferred from persisted `kind`, `is_binary`, file extension, and blob-byte sniffing.
  Renderer image URLs use Tauri's local asset protocol from blob-store paths, and the protocol scope is restricted to `$APPDATA/blobs/**/*` in `tauri.conf.json` + capability scope.
- Rename display uses `pr_files.previous_path` (`old_path` fallback) plus `rename_similarity` (stored as REAL, surfaced as integer percentage) and renders `previous_path -> path` in diff headers.

Reason: these rules keep optimistic review comments deterministic, prevent viewed-state drift after force-pushes, avoid exposing filesystem paths outside blob storage, and preserve rename intent in the diff UI without re-anchoring heuristics.

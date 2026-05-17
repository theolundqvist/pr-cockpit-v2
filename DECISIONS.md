# Decisions

### 2026-05-16: Use `gh` CLI's public OAuth client_id `Iv1.b507a08c87ecfe98` for device flow (M1)

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

### 2026-05-17: Confirm GitHub OAuth client_id reuse note remains accurate (M1 bootstrap)

Decision: retain the existing M1 note to reuse `gh` CLI's public OAuth client_id
`Iv1.b507a08c87ecfe98` for device flow during development.

Reason: this bootstrap task does not introduce a project-owned OAuth app, and the
existing rationale still applies to local development and fixture-driven runs.

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

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

### 2026-05-17: Markdown renderer versioning + cache invalidation policy (M1 markdown pipeline)

Decision: expose `RENDERER_VERSION` in `apps/desktop/src-tauri/src/lib.rs` and key rendered
HTML cache entries by `(sha256(body), RENDERER_VERSION)`.

Reason: markdown output shape changes must invalidate old cache entries deterministically.
Policy: bump `RENDERER_VERSION` whenever parser options, post-processors, emoji mapping,
or ammonia allowlist behavior changes in a way that can alter output HTML.

### 2026-05-17: Emoji shortcodes render as Unicode equivalents (M1 markdown pipeline)

Decision: expand selected GitHub emoji shortcodes to Unicode glyphs (`:rocket:` → `🚀`) in
the post-processing stage.

Reason: this avoids SVG asset dependencies in M1 while preserving semantic meaning in both
rendered HTML and text-only diffs used by the corpus scorer.

### 2026-05-17: Ammonia allowlist follows GitHub-shaped safe subset (M1 markdown pipeline)

Decision: run every rendered comment through ammonia at the Rust boundary and allow a curated
subset needed for PR cockpit read-only parity: tables, task-list checkboxes, fenced code
attributes (`data-language`), alert blockquote classes, and suggestion block wrapper divs.

Reason: it keeps fidelity high enough for M1 while ensuring script/style/iframe/object/embed
and unsafe URL schemes are stripped before HTML crosses the backend/frontend boundary.

### 2026-05-17: Markdown corpus is committed snapshot with opt-in network refresh (M1 markdown pipeline)

Decision: CI runs against committed `tools/markdown-corpus/corpus.json` plus checked-in
`oracle/*.html` files. Refresh is manual via `pnpm corpus:fetch`, optionally authenticated with
`GITHUB_TOKEN`.

Reason: deterministic CI is required for milestone gating; network refreshes are useful for
maintainers but must never make the gate flaky.

### 2026-05-17: M1 corpus gate fixed at 2%; tighten to 1% in M6

Decision: enforce weighted corpus regression `<= 0.02` in M1 and revisit to `<= 0.01` in M6.

Reason: M1 targets broad read-only parity while we are still finalizing parser extensions and
GitHub-specific post-processing; M6 is where fidelity hardening work can justify the stricter bar.

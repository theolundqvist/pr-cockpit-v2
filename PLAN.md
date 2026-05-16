# PR Cockpit — Engineering Plan

A local-first Tauri desktop app that mirrors the GitHub Pull Request UI with instant + optimistic UX. This file is the contract: all design decisions live here. Edit it when reality forces a change; don't drift silently.

Source: synthesized from three independent design plans (`oracle_runs/20260516T085519Z/synthesis.md`) plus follow-up decisions documented inline below.

## 0. Mission

Make "what PRs need me, why, and where is the local worktree?" disappear as a recurring tax. View and edit everything the github.com PR UI lets you view and edit. Every read is instant (served from local SQLite). Every write applies optimistically and reconciles in the background.

## 1. Architecture

### 1.1 Process model

- **Single Tauri v2 process.** Rust owns the SQLite connection, the GitHub API client, the mutation queue, token storage, and filesystem watchers. The webview is a renderer; it never opens SQLite, never talks to GitHub directly, never persists state.
- **IPC**: Tauri commands for request/response writes and queries. Tauri events for cache invalidations the UI subscribes to. Types are auto-generated from Rust → TypeScript via `specta` to eliminate the "we forgot to update the TS type" bug class.

### 1.2 Tech stack

- **Backend**: Rust + Tauri v2, `sqlx` (SQLite), `keyring` (OS keychain), `octocrab` or hand-rolled `reqwest` (GitHub API), `comrak` (cmark-gfm-compatible markdown), `ammonia` (HTML sanitization), `tree-sitter` (highlighting, wasm grammars), `notify` (FS watchers), `git2` (local git), `specta` (typed IPC).
- **Frontend**: Svelte 5 + TypeScript + Vite + SvelteKit (static adapter for Tauri).
- **Visual parity**: `@primer/css`, `@primer/primitives`, `@primer/octicons-react`-equivalent (use the CSS classes + SVG octicons directly).
- **Inline editor surface** (comment composition, suggestion blocks): `<textarea>` + a Preview tab. No rich-text editor in M1–M2. The Preview tab renders through the same comrak pipeline as the timeline, so parity is guaranteed by construction.
- **Diff renderer**: custom virtualized component built on `@tanstack/svelte-virtual`. Not Monaco (too heavy). Not diff2html (doesn't handle review threads / suggestions / viewed checkboxes).

### 1.3 Why this stack

- **Svelte 5 over React**: smaller bundle, faster cold start, better Tauri webview perf, and `$state`/`$derived`/`$effect` map cleanly onto live SQLite queries. The original React lean was driven by Lexical; we dropped Lexical, so React no longer wins.
- **Textarea + Preview over Lexical**: github.com itself ships this UX. No serialization fidelity risk. No autocomplete-engine to build before M5+. Composer ships in days, not weeks. Slash commands / @-chips / saved-replies deferred to M5 as optional polish.
- **Primer CSS over a custom design system**: github.com uses it. Pinning a Primer version gets us 95% visual parity at zero design cost. Drift from github.com's latest tweaks is acceptable — design moves slower than features.
- **comrak in Rust over JS markdown**: closest open-source match to `cmark-gfm` (which github.com itself uses); one canonical render pipeline cached by `(content_hash, renderer_version)`; sanitization sits naturally at the Rust boundary.
- **GitHub's served `.diff` is canonical**: comment anchoring positions are defined relative to GitHub's served patch. Local-git reconstruction drifts (different rename thresholds, context counts, binary detection) and silently mis-anchors comments. Local git stays in scope for range-diff, blame, history, and suggestion application — operations where positions don't matter.

## 2. Sync engine

SQLite is the unconditional source of truth for every UI read. The sync engine writes to SQLite; the UI subscribes.

### 2.1 Tiered, focus-aware polling

| Tier | Resource                          | Cadence (focused) | Cadence (background) |
| ---- | --------------------------------- | ----------------- | -------------------- |
| Hot  | Currently-viewed PR               | 5s                | paused               |
| Warm | Inbox + assigned/review-requested | 30s               | 2 min                |
| Cool | Subscribed repos' open PR list    | 3 min             | 15 min               |
| Cold | Other watched repos               | 30 min            | on-demand            |

All polling pauses after 5 minutes of window unfocus. Cheap change-detection through the **GitHub Notifications API** (`/notifications` with `If-Modified-Since`, respecting `X-Poll-Interval`) covers most cross-repo signals in a single conditional call. Anything it flags triggers a targeted GraphQL refetch.

### 2.2 Rate-limit budgeter

A Rust actor wraps every API call. It parses `X-RateLimit-Remaining` / `X-RateLimit-Reset`, applies back-pressure to background tasks below 1000 GraphQL points or 500 REST. Foreground (user-initiated) calls bypass back-pressure on a high-priority lane. A rate-limit meter is visible in the status bar.

### 2.3 Cold start (parallel)

1. Render cached inbox from SQLite immediately. **<100 ms first-paint target.**
2. Hit `/notifications since=last-seen` for delta.
3. Fat GraphQL query for the currently-visible PR.
4. Backfill stale inbox PRs by `updatedAt` cursor.

### 2.4 Incremental fetch

- Timeline items by cursor since last seen.
- Files refetched only when `head_sha` changes.
- Checks by check-suite/check-run `updated_at`.
- Diffs invalidated only on `base_sha` / `head_sha` / `merge_base` change.
- `mergeable: null` → dedicated re-poll task with backoff 2 s, 5 s, 15 s, 45 s, 2 min, max 5 min.

### 2.5 Webhook relay (M6, optional)

Self-deploy Cloudflare Worker recipe. Never SaaS. Polling-on-focus + Notifications API hits 90% of perceived latency for free; the relay is a nice-to-have for users running it.

## 3. Optimistic update model

Mutations are **persisted to a `pending_mutations` table and projected into denormalized read models**. The log holds inverse patches; the projection serves the UI in O(1).

### 3.1 Per-mutation-kind handlers (~25)

Each handler declares: predicted local effect, server call, conflict resolver, retry policy, optimism level.

Mutation kinds:
`addComment` / `editComment` / `deleteComment` / `addReaction` / `removeReaction` / `addLabel` / `removeLabel` / `setAssignees` / `requestReview` / `removeReviewRequest` / `submitReview` / `resolveThread` / `unresolveThread` / `markFileViewed` / `unmarkFileViewed` / `updatePrTitle` / `updatePrDescription` / `setMilestone` / `setProject` / `convertToDraft` / `markReadyForReview` / `enableAutoMerge` / `disableAutoMerge` / `updateBranch` / `merge` / `closePr` / `reopenPr`.

### 3.2 Three optimism levels

- **Full optimism**: comments, reactions, labels, assignees, reviewer requests, mark-viewed, thread resolve, draft-description edits.
- **Cautious optimism** (shown as pending, not "done"): submit review, update branch, rerun CI, convert draft/ready, project edits.
- **No optimism** (confirm + spinner + server confirmation): merge, squash, rebase, force-push, branch delete, dismiss review, enqueue merge queue.

### 3.3 Rollback UX

Inline error state on the affected control (red banner near the comment / chip / button), plus a global "sync errors" tray. One-click retry/discard. Silent revert only for transient network errors.

### 3.4 Reconciliation

Every successful server response upserts returned nodes into the cache and schedules a targeted refetch of the affected PR's timeline. Temp local IDs → server IDs via an `id_mappings` table. Server-normalized markdown overwrites local prediction with a subtle "server adjusted" affordance.

### 3.5 Offline

Queue persists. Safe mutations replay in submission order on reconnect; unsafe mutations are blocked with a clear "requires connection" state. Hard conflicts (PR closed while you composed) prompt with a diff modal — never silently dropped.

### 3.6 Tests

Property-based over `(mutation_sequence × server_response_sequence × conflict_sequence)`. Airplane-mode dogfooding drills before each milestone gate.

## 4. Data model (SQLite)

Core tables (column lists abbreviated; full DDL in `src-tauri/migrations/`):

`accounts` (id, host, login, token_kind, scopes, created_at)
`repos` (id, account_id, owner, name, default_branch, …)
`repo_subscriptions` (repo_id, account_id, watch_tier, last_full_sync_at)
`users`, `orgs`
`pull_requests` (id, repo_id, number, state, draft, base_ref, base_sha, head_ref, head_sha, head_repo_id, mergeable_state, merge_state_status, …)
`pr_labels`, `pr_assignees`, `pr_reviewers`, `pr_projects`, `pr_milestones`
`commits`, `pr_commits` (junction, with order)
`comments` (id, pr_id, kind: `issue|review|review_thread_reply`, author_id, body, created_at, updated_at, in_reply_to_id, …)
`review_threads` (id, pr_id, path, line, side, start_line, start_side, original_commit_sha, original_path, original_position, is_outdated, is_resolved, resolved_by_id)
`reviews` (id, pr_id, author_id, state, body, submitted_at)
`check_suites`, `check_runs`, `check_annotations`
`pr_files` (pr_id, head_sha, path, old_path, status, additions, deletions, viewed_by_account_id, viewed_at_head_sha)
`pr_patches` (pr_id, head_sha, patch_blob_sha)
`notifications` (id, account_id, reason, subject_type, subject_id, unread, last_read_at)
`worktrees` (id, repo_id, path, head_sha, branch, dirty, ahead, behind, mapped_pr_id, mapping_confidence, mapping_source)
`pending_mutations` (id, account_id, kind, target_type, target_id, idempotency_key, input_json, optimistic_patch_json, inverse_patch_json, status, retries, created_at, last_error)
`id_mappings` (local_id, server_id, kind)
`sync_cursors` (account_id, resource, cursor, etag, last_fetched_at)
`rate_limit_buckets` (account_id, resource, remaining, reset_at)
`blob_refs` (sha256, kind, size, ref_count)

Denormalized read models: `pr_inbox_rows`, `pr_detail_summary`, `unread_counts`, `file_tree_summary`. FTS5 virtual tables on PR title/body, comments, review bodies, filenames, authors.

**Blob store**: content-addressed under the Tauri app data dir (`blobs/<sha256-prefix>/<sha256>`). Holds patches (zstd-compressed), attached images, raw check-run logs, rendered markdown HTML caches. SQLite holds only `sha256` references.

**Outdated threads**: trust GitHub's `isOutdated`. Store both original coordinates (`original_commit_sha`, `original_path`, `original_position`) and current coordinates (`path`, `line`, `side`, `start_line`, `start_side`) verbatim from the server. Never locally re-anchor.

## 5. GitHub API strategy

### 5.1 REST / GraphQL boundary

- **GraphQL** for: PR detail (one fat query covering title/body/timeline/threads/reviews/check summary/labels/assignees/reviewers/mergeable/headRef/baseRef), inbox refresh (thin batched query), review thread fetch + `resolveReviewThread` / `unresolveReviewThread`, projects v2, node hydration.
- **REST** for: unified diff (`Accept: application/vnd.github.v3.diff`), check-run annotations (`/check-runs/{id}/annotations`), notifications (`/notifications`), branch protection, repo lists with conditional caching, force-push range-diff inputs via `/compare`, raw file content, most mutations.

### 5.2 Two canonical queries

- `PrDetail` — fat, versioned, hand-written, reviewed centrally. No per-component GraphQL.
- `InboxRefresh` — thin, batches ~20 PR node IDs with only fields the inbox row renders.

### 5.3 Auth

1. Detect `gh auth token`, offer one-click import with explicit scope display.
2. OAuth Device Flow as default for new users.
3. PAT for enterprise / constrained setups.

Tokens stored exclusively in OS keychain via `keyring`. Never in SQLite. Never in logs.

### 5.4 Sharp edges (designed-in)

- `mergeable: null` → re-poll backoff 2s/5s/15s/45s/2min, max 5min.
- Review threads + resolve/unresolve: GraphQL only.
- Check annotations: separate REST call, paginated.
- Force-push range-diff: prefer local `git range-diff` if worktree exists; else fetch two head diffs and compute in Rust.
- Suggestion application: prefer GitHub's mutation if it exists for single suggestions; for batched, require clean worktree and commit locally before push.

## 6. Diff viewer architecture

- **Patch source**: GitHub REST `.diff` is canonical for anchoring. Local git supplemental for range-diff, blame, history, suggestion application.
- **Renderer**: custom virtualized line component on `@tanstack/svelte-virtual`.
- **Syntax highlighting**: tree-sitter compiled to wasm, run in a Web Worker. Lazy by viewport. Hard cap: skip highlighting for files > 5000 lines unless user opts in. Token cache keyed by `(language, content_hash)`.
- **Comment anchoring**: GitHub's `line`/`side`/`start_line`/`start_side`. Store `position` only as legacy. Server `isOutdated` is authoritative.
- **Selection-to-comment**: compute target `(file, side, line, start_line)`, send straight to GitHub.

## 7. Notifications

Two channels:

1. `/notifications` polling (focused 60 s, background 5 min) with `If-Modified-Since` — a _signal_, not the inbox.
2. Per-active-PR polling for sub-10 s freshness on the visible PR.

OS notifications generated by a local rule engine after sync reconciliation (so we can dedupe and add app-specific triggers like "CI flipped", "conflict appeared", "mutation failed"). Configurable: review requested, changes requested, approved, mention, CI fail/recover, merge conflict, mutation failure. Quiet hours + focus mode + per-repo filters. Dedup keyed on `(account, repo, pr, event_type, actor, server_event_id)`.

## 8. Local worktree integration

- **Discovery**: configured root dirs (`~/dev`, `~/code`, `~/src`, `~/repos` defaults, user-extensible). `git worktree list --porcelain` is authoritative per repo. Optional `.github-pr-cockpit.toml` per worktree for overrides. No `$HOME` autoscan.
- **Freshness**: `notify` watchers on `.git/HEAD`, `.git/refs/`, `.git/index`, working tree. Debounce 200–500 ms then `git status --porcelain=v2 --branch` + `git rev-list --left-right --count`. No periodic polling.
- **Branch ↔ PR mapping**: multi-signal, confidence-scored. Signals: remote URL match, branch upstream, `gh pr status` output, exact head SHA, branch name conventions (`fork-owner/branch`, `pr/123`), PR head SHA ancestry. UI shows inferred mapping with confidence + manual override.
- **App-created vs. user-managed**: tracked in `worktrees` table. Cleanup blocked on user-managed worktrees; app-created cleanup requires confirmation + dirty-state snapshot. Never delete uncommitted work.

## 9. Multi-account / multi-org

Every viewer-dependent field is account-scoped: viewed state, notifications, available merge methods, permissions, project access, rate-limit counters, pending mutations. Token storage: OS keychain only. UI: account switcher in sidebar; per-row badge in multi-account inbox; mutation composer shows posting identity with quick-switch before submission. Global "all accounts" aggregated inbox.

GHE: schema designed for it (host field, per-host endpoint config). Full support is a stretch, not v1.

## 10. Performance targets (CI-enforced)

| Target                                 | Budget                     |
| -------------------------------------- | -------------------------- |
| Inbox first paint (warm cache)         | < 100 ms                   |
| Inbox fully refreshed                  | < 800 ms p50               |
| PR detail open (preloaded)             | < 50 ms                    |
| PR detail open (cold cache hit)        | < 250 ms                   |
| File open in diff (cached)             | < 100 ms                   |
| File open with highlighting (viewport) | < 300 ms                   |
| Comment submit visible                 | < 16 ms (single frame)     |
| Command palette open                   | < 75 ms                    |
| Command palette result                 | < 150 ms                   |
| Diff scroll                            | 60 fps on ≤ 10k-line diffs |

## 11. Stacked PR support (M6)

- Detection: walk PRs where `base.ref == another_open_pr.head.ref` in the same repo at sync time. Cache `stack_id` + `stack_position`. Diamond / ambiguous graphs render as a DAG with a warning.
- Render: linear-stack-first sidebar tree (most stacks are linear). Per-PR review state, CI state, conflict state, "blocked by" reasons. Explicit base/head SHAs.
- Operations: shell out to local git for `rebase stack` (sequential `git rebase`) and `merge stack` (sequential merges in order). On conflict, pause and surface the worktree. Retarget base via GraphQL `updatePullRequest(baseRefName:…)` after a merge.
- Integration: defer to Graphite CLI (`gt`) if installed and the user opts in. M6 feature, not v1.

## 12. Milestones

Each milestone exits when its acceptance criteria are met (not when the code "looks done"). Dogfood gates are non-negotiable.

### M1 — Read-only cockpit

- Auth: `gh` token import + Device Flow fallback, multi-account.
- Sync 3 real repos; cold-start < 100 ms inbox paint.
- PR detail: description, timeline, labels, reviewers, checks, side-by-side + unified diff with tree-sitter highlighting on a 5k-line diff at 60 fps scroll.
- Markdown corpus regression ≤ 2 % pixel-diff against github.com on 200 real comments.
- Dogfood gate: read-only PR viewer for one week.

### M2 — Optimistic writes

- 25 mutation kinds with predict/apply/rollback/reconcile; property tests pass.
- Airplane-mode drill: 10 comments + label changes + thread resolutions offline; on reconnect everything replays, no data loss.
- Composer: textarea + Preview tab (same comrak pipeline).
- Dogfood gate: comments + reviews + labels + thread resolution as primary for one week.

### M3 — Diff polish + worktree read + notifications

- Multi-line comments, suggestion blocks (compose), viewed checkboxes, images/binary/renames.
- Worktree discovery + `notify` watchers + confidence-scored PR mapping.
- Native OS notifications + quiet hours + focus mode.

### M4 — Merge + force-push + multi-account

- Merge/squash/rebase + auto-merge + merge queue + delete branch.
- Force-push range-diff.
- Multi-account UI + per-account rate-limit meters.
- `mergeable: null` backoff verified.

### M5 — Editor power + worktree write

- Suggestion apply (single via API, batched via clean-worktree commit).
- Saved replies (text-insert), command palette, full keyboard layer, paste-image upload.
- Check annotations on diff, failed-job log tail, rerun checks.

### M6 — Stacked PRs + webhook relay + polish

- Stack detection + tree + sequential rebase/merge (plain git, optional Graphite).
- Self-deploy webhook relay (Cloudflare Worker).
- GHE compatibility pass. Markdown corpus ≤ 1 %. All perf budgets green. v1.0 tag.

## 13. Top risks

1. **GraphQL rate-limit exhaustion on large orgs** — Mitigation: tiered polling, per-account budgeter with back-pressure, two centrally-owned fat queries (no ad-hoc per-component queries), Notifications API as cheap change detector, opt-in repo subscriptions, rate-limit meter in the status bar, optional webhook relay.
2. **Comment anchoring divergence across force-push / rebase** — Mitigation: store GitHub's `line`/`side`/`start_line`/`start_side` + `original_*` verbatim, trust `isOutdated`, never invent local anchors. Real-PR force-push test corpus.
3. **Markdown rendering fidelity** — Mitigation: comrak + cmark-gfm-equivalent extensions + custom post-processors for GitHub extras; CI visual-regression corpus of ~200 real comments against github.com; GitHub's markdown REST endpoint as oracle; explicit fidelity bar (~95%); "open on github.com" escape hatch.
4. **Optimistic update correctness** — Mitigation: persisted log + projected state + inverse patches + per-kind handlers (~25); property-based tests over `(mutations × responses × conflicts)`; airplane-mode dogfooding; failed-mutation tray always legible.
5. **Tauri webview perf on large diffs / large markdown** — Mitigation: virtualization everywhere; off-main-thread tree-sitter; Rust-side markdown render; hard caps with graceful degradation; CI perf benchmarks on WebKitGTK specifically (Linux worst case). Escape hatch: native Rust diff renderer in a separate Tauri window via `egui`.
6. **Local worktree automation damaging user work** — Mitigation: ownership metadata; cleanup requires confirmation; dirty-state snapshot before destructive ops; never delete unmanaged worktrees; suggestion-batch-apply gated on clean worktree.
7. **Primer CSS drift from github.com** — Mitigation: pin a Primer version; accept the gap; quarterly upgrade pass.

## 14. Non-goals (v1)

- Issues (separate from PR-linked issues).
- Discussions.
- Actions runs not directly tied to a PR check.
- Code search.
- Insights / analytics dashboards.
- Mobile (iOS / Android).
- Browser extension.

# Synthesis: PR Cockpit design plan

## 1. Consensus

All three voices converge on the load-bearing engineering choices. These can be locked.

- **SQLite is the unconditional source of truth for every UI read.** No component awaits the network for display; the sync engine writes to SQLite, the UI subscribes. Claude, GPT, and Grok all frame this as the foundational decision and cite the same prior art (Linear, Figma, Replicache, Obsidian).
- **Rust owns sync, git, token storage, and the mutation queue.** Webview is a renderer plus an editor surface — never a writer to the database.
- **Tiered, focus-aware polling, not a flat cadence.** Focused PR: seconds. Inbox: 30–60s. Background repos: minutes. Cold repos: on demand. All three give roughly the same numbers.
- **REST with `ETag`/`If-None-Match` for cheap freshness; GraphQL for nested reads and resources REST exposes poorly.** Specifically: review threads, projects v2, and batched node hydration must go through GraphQL; check annotations, notifications, raw `.diff`, and most mutations stay on REST.
- **Optimistic updates persist in a `pending_mutations` table.** Survives restarts; replays in submission order; reconciliation runs against server-returned authoritative state, not against the prediction.
- **Destructive ops opt out of optimism.** Merge, force-push, rebase, branch delete, dismiss review: confirm + spinner + explicit server-confirmed state. All three flag silent optimistic-merge as the worst possible UX bug.
- **`mergeable: null` is a known sharp edge** and needs a dedicated backoff re-poll task (5s → 15s → 45s → 2min).
- **OAuth Device Flow as default**, PAT fallback, opportunistic `gh auth token` import, tokens in OS keychain via the `keyring` crate. Multi-account from day one; rate limits, repos, notifications all per-account.
- **Diff highlighting via Tree-sitter, viewport-lazy, off the main thread.** Hard cap somewhere around 5k–10k lines, then degrade.
- **Local worktrees discovered via configured roots + `git worktree list --porcelain`**, freshness via `notify` crate FS watchers with debounce, never periodic `git status` polling.
- **Stacked-PR detection = transitive `base.ref == another_open_pr.head.ref`** walked at sync time. Operations shell out to local `git` (or Graphite CLI if present) because GitHub has no atomic stack-rebase API.
- **Cold start renders the cached inbox first (<300ms), then refreshes in phases.** Notifications API first as a cheap change-detector, then targeted GraphQL hydration of the focused PR, then backfill.

## 2. Disagreements

### a. Frontend framework — React vs. Svelte 5
**Type: interpretive, but the Lexical constraint moves it toward factual.**

- **Grok**: Svelte 5 + runes. Smallest bundle, best Tauri integration, no runtime overhead, ergonomic reactivity.
- **Claude & GPT**: React + TypeScript. Ecosystem depth — CodeMirror 6, TanStack Virtual, Radix, react-arborist, react-markdown plugins — is overwhelmingly React-first.

**Recommendation: React.** Svelte's reactivity story is genuinely nicer for a SQLite-backed live-query layer, but the user's Lexical preference is decisive: **Lexical is a React-first framework**. There's a `@lexical/svelte` community port, but it's thin and untested for the plugin surface we need (slash commands, mention autocomplete, paste-image, suggestion blocks). Combined with CodeMirror 6 for diff inline editing and TanStack Virtual for the lists, React is the only framework where every load-bearing dependency exists in mature form. We pay bundle size; we save engineering months.

### b. Markdown render location — Rust (comrak) vs. JS (remark/markdown-it)
**Type: interpretive, with Lexical pushing one way.**

- **Claude**: Render in Rust with `comrak` + `syntect` + `ammonia`, ship sanitized HTML to the webview, hydrate interactive bits in React.
- **GPT & Grok**: Render in JS with remark/rehype (or markdown-it), DOMPurify for sanitization, GitHub markdown API as a parity oracle.

**Recommendation: hybrid, weighted toward Rust render.** Render with `comrak` in Rust because (1) `comrak` is the closest open-source match to `cmark-gfm` (which is what GitHub itself uses); (2) consistent output across every comment surface, cacheable by `(body_hash, version)`; (3) the security boundary is naturally in Rust where untrusted markdown enters the system; (4) one canonical rendered HTML blob per comment is reusable from anywhere (preview, timeline, inline diff). Hydrate post-render in React for task-list checkbox toggles, suggestion `Apply` buttons, hover previews on `@user`/`#issue`/SHA, and auth'd image loading. GitHub's markdown REST endpoint is useful as a parity-test oracle in CI, not as a runtime fallback.

### c. Diff source — local git vs. GitHub `.diff` endpoint
**Type: factual. Claude is right.**

- **Grok**: Prefer local git via libgit2, fall back to GitHub API.
- **Claude**: GitHub `.diff` is canonical because comment anchoring (`position`, `line`/`side`/`start_line`/`start_side`) is defined relative to GitHub's served patch. A locally reconstructed diff has different rename thresholds, different context-line counts, different binary detection — anchors will drift.
- **GPT**: Hybrid, but explicit about indicating provenance in the UI.

**Recommendation: GitHub `.diff` is canonical for anything anchor-related; local git for range-diff, blame, file history, and suggestion application.** Claude's argument is correct and not interpretive: GitHub's anchor coordinates exist in one frame of reference. Mixing frames silently mis-anchors comments. Local git stays in the toolbox for operations where positions don't matter.

### d. Comment anchoring across force-push — verbatim store vs. local re-match
**Type: factual. Claude is right.**

- **Grok**: Re-match by line content + 3 surrounding lines, surface "nearest match" with confidence score.
- **Claude**: Store GitHub's `position`/`original_position`/`commit_sha` verbatim. Use the server's `isOutdated` flag. Don't invent anchoring.

**Recommendation: store verbatim, trust the server.** GitHub's anchoring algorithm is not fully documented and evolves. Local re-matching will drift from what github.com renders for the same comment, and "drifted comment" is the highest-regret class of bug because it's silent. If we want to *help* the user find where an outdated comment "would be now", show it as best-effort UI sugar — but never overwrite the server anchor.

### e. Optimism shape — event-sourced overlay vs. pending flag on rows
**Type: interpretive, leans Claude.**

- **Claude**: `pending_mutations` is an ordered log; UI reads `authoritative ⊕ replay(pending)`; never write fake rows.
- **GPT**: Persist mutations *and* a projected entity state, with inverse patches per mutation for rollback.
- **Grok**: Write to cache immediately with `pending=true`, UI renders pending state; pending_mutations queue is the rollback record.

**Recommendation: GPT's projected-state-plus-log model.** Claude's pure overlay is cleaner conceptually but expensive in practice — every read goes through replay. Grok's in-place flag is fast but corrupts under multi-mutation dependency chains. GPT's hybrid (write the projection into a `_projected` view or a denormalized read model, *plus* keep the durable log with inverse patches) gets both: O(1) reads, deterministic rollback, and the log is still the source of replay truth when conflicts force a rebuild. Per-mutation-kind handlers (Claude's framing) is mandatory regardless of shape — `addComment` and `mergePR` cannot share a generic optimism path.

### f. MVP timeline — 5/6/7 milestones, weeks vs. open-ended
**Type: interpretive.**

- **Grok**: 12 weeks total, M1–M6, very aggressive.
- **GPT**: 6 milestones, no week estimates, sensibly ordered (read → diff → comments → metadata → worktree → merge/stacked).
- **Claude**: 7 milestones, conservative, merge intentionally deferred to M4 so the optimism framework is battle-tested first.

**Recommendation: GPT's ordering, Claude's caution on merge placement, no week estimates in the plan itself.** Grok's 12-week timeline is unrealistic for a parity-level cockpit (markdown fidelity alone can eat that). Claude's instinct to put merge after labels/comments is correct: merge is the highest-stakes write and should inherit a stress-tested mutation engine.

### g. Webhook relay — when and how
**Type: interpretive.**

- **Claude**: M6, self-deploy script, never a SaaS.
- **GPT**: Optional acceleration, not required.
- **Grok**: Optional Cloudflare Worker or `gh webhook forward`, mentioned at M6.

**Recommendation: opt-in, self-deploy, post-MVP.** All three agree on the principle; just confirm it doesn't slip earlier. Polling-on-focus + Notifications API hit 90% of perceived latency for free.

## 3. Distinctive contributions

**Claude** brought:
- The argument for **canonical GitHub-patch anchoring** (the single most important correctness call in the plan).
- **Content-addressed blob store** outside SQLite for patches, images, logs, keyed by sha256 — dedupes across syncs and keeps hot pages hot.
- **`specta`/`tauri-bindgen`** for auto-generated Rust↔TS types, eliminating a whole bug class.
- **Property-based tests over (mutations × server responses × conflicts)** for the optimism engine.
- **Visual-regression corpus of real github.com renders** as the markdown fidelity bar.
- Explicit numerical perf budgets (inbox <100ms first paint, comment-submit <16ms, etc.).
- The framing that batched suggestion-apply should require a clean worktree; never silently rewrite the user's branch.

**GPT** brought:
- **Account-scoping discipline**: every viewer-dependent field is per-account, even when the underlying object is global. Catches subtle bugs (private image auth, available merge methods, viewed state).
- **GitHub parity matrix** as a deliverable: every visible control on a PR page classified as API-supported / local-git / approximated / blocked, with documented fallback UX. This is a process artifact the others missed.
- **Inverse patches per mutation** for rollback — the cleanest formalism for partial-failure recovery.
- **Confidence-scored branch-to-PR mapping** with manual override, instead of relying on a single signal.
- Explicit call-out that **paste-image upload is not a documented public API** and needs verification — easy to miss.
- **Reconciliation always upserts server-returned nodes** + schedules a targeted refetch, even on success.

**Grok** brought:
- **`mergeable: null` polled every 2s until non-null** — the most concrete cadence for this sharp edge.
- **Rate-limit meter visible in the status bar** — turns an opaque background failure into a legible UX.
- **`gh auth token` as the *first* auth path** with PAT/device-flow as fallback (Claude/GPT make device flow default; Grok's ordering is friendlier for the existing-user case and worth adopting as a third path).
- **Per-PR webhook-relay endpoint** sketch (Cloudflare Worker + Durable Object).
- A **lighter diff renderer** discussion: explicit reject of Monaco (5MB+), considers canvas-vs-div tradeoff. Worth keeping in mind even though we'll likely pick CodeMirror 6 + TanStack Virtual.
- **Convention file `.github-pr-cockpit.toml` per worktree** as a fallback PR-mapping signal.

## 4. What to verify next

Each check should be cheap and yield a yes/no before we lock the design.

- **Lexical + comrak round-trip fidelity.** Compose a comment with task lists, suggestion blocks, alerts, tables, math, `<details>`, and `@user`/`#123`/SHA refs in Lexical; serialize to markdown; render via comrak; compare to github.com's render of the same source. Spike a 50-example corpus.
- **Comrak vs. markdown-it-with-plugins on a real-comment corpus.** Scrape ~200 PR comments via API, render both pipelines, score divergence from github.com (use GitHub's markdown REST endpoint as oracle). Confirms our render-in-Rust call.
- **Fat GraphQL query point cost on a large PR.** Run our designed PR-detail query against `torvalds/linux`, `kubernetes/kubernetes`, `microsoft/vscode`. Verify it stays under ~20 points and ~500KB. If not, narrow the query.
- **Conditional REST really doesn't charge rate limit on 304.** Documented but worth observing on `/notifications`, `/repos/{o}/{r}/pulls/{n}`, `/repos/{o}/{r}/issues/{n}/comments`. Run with instrumentation.
- **WebKitGTK perf on a 10k-line diff with Tree-sitter wasm highlighting.** Linux is the worst-case webview. If we can't hit 60fps scroll there, we need a Plan B before locking React + Tauri.
- **`mergeable: null` recovery time on a real fresh push.** Confirm the 2s/5s/15s backoff is right for typical and pathological cases.
- **Suggestion-apply API behavior.** Verify whether GitHub's GraphQL exposes a `applyPullRequestSuggestion` mutation (or equivalent) for single suggestions. If not, the local-worktree-commit path is mandatory for v1.
- **Optimistic label add/remove/add with mid-flight failure.** End-to-end test against real API; verify the projected state and the log reconcile to the correct final set.
- **`keyring` crate on minimal Linux (i3/Sway, no Secret Service).** Define the fallback before users hit it.
- **`gh webhook forward` in prod.** Confirm it's strictly dev-only as docs say, or whether it's viable for a small self-deployed relay.

## 5. Recommended final plan

### 5.1 Sync engine & data freshness

SQLite is the unconditional source of truth for every UI read. Rust owns the connection pool (`sqlx` + `r2d2`); the webview never opens SQLite directly. Components subscribe to query results via a thin live-query layer that emits coalesced invalidation events ("pr:123 changed") over Tauri events.

Freshness is tiered and focus-aware:

| Tier | Resource | Cadence (focused) | Cadence (background) |
|---|---|---|---|
| Hot | Currently-viewed PR | 5s | paused |
| Warm | Inbox + assigned/review-requested | 30s | 2min |
| Cool | Subscribed repos' open PR list | 3min | 15min |
| Cold | Other watched repos | 30min | on-demand |

Polling pauses after 5 minutes of window unfocus.

Cheap change-detection runs through the **GitHub Notifications API** (`/notifications` with `If-Modified-Since` and respecting `X-Poll-Interval`) — a single conditional call covers most cross-repo signals. Any flagged PR triggers a targeted GraphQL refetch.

**Cold start order** (parallel):
1. Render cached inbox immediately from SQLite (<100ms target).
2. Hit `/notifications since=last-seen` for delta.
3. Run the canonical fat GraphQL query for the currently-visible PR tab.
4. Backfill stale inbox PRs by `updatedAt` cursor.

**Incremental fetch**: timeline items by cursor since last seen; files only refetched when `head_sha` changes; checks by check-suite/check-run `updated_at`; diffs invalidated only on `base_sha`/`head_sha`/`merge_base` change.

**Rate-limit budgeter** is a Rust actor wrapping every API call. It parses `X-RateLimit-Remaining` and `X-RateLimit-Reset`, applies back-pressure to background tasks below 1000 GraphQL points or 500 REST. Foreground (user-initiated) calls have a high-priority lane that bypasses back-pressure. A rate-limit meter is visible in the status bar (Grok's contribution).

**Webhook relay**: optional, M6, ship as a one-command Cloudflare Worker / Fly deployer. Not required for v1.

### 5.2 Optimistic update model

Mutations are **persisted to a `pending_mutations` table and projected into denormalized read models**. The log holds inverse patches; the projection serves the UI in O(1).

- Per-mutation-kind handler registry (~25 kinds): `addComment`, `editComment`, `deleteComment`, `addReaction`, `addLabel`, `removeLabel`, `requestReview`, `submitReview`, `resolveThread`, `unresolveThread`, `markFileViewed`, `updateBranchAutoMerge`, …
- Each handler declares: predicted local effect, server call, conflict resolver, retry policy, optimism level.
- **Three optimism levels**:
  - *Full optimism*: comments, reactions, labels, assignees, reviewer requests, mark-viewed, thread resolve, draft-description edits.
  - *Cautious optimism* (shown as pending, not completed): submit review, update branch, rerun CI, convert draft/ready, project edits.
  - *No optimism* (confirm + spinner + server confirmation required): merge, squash, rebase, force-push, branch delete, dismiss review, enqueue merge queue.
- **Rollback UX**: inline error state on the affected control (red banner near the comment / chip / button), plus a global "sync errors" tray. One-click retry/discard. Silent revert only for transient network errors.
- **Reconciliation**: every successful server response upserts returned nodes into the cache and schedules a targeted refetch of the affected PR's timeline. Temp local IDs → server IDs via an `id_mapping` table. Server-normalized markdown overwrites local prediction; a subtle "server adjusted" affordance appears.
- **Offline**: queue persists; safe mutations replay in submission order on reconnect; unsafe mutations are blocked with a clear "requires connection" state. Hard conflicts (PR closed while you composed) prompt with a diff modal — never silently dropped.
- **Tests**: property-based over `(mutation_sequence × server_response_sequence × conflict_sequence)`; airplane-mode dogfooding drills.

### 5.3 Data model (SQLite schema sketch)

Core tables:
- `accounts` (id, host, login, token_kind, scopes, created_at)
- `repos` (id, account_id, owner, name, default_branch, …)
- `repo_subscriptions` (repo_id, account_id, watch_tier, last_full_sync_at)
- `users`, `orgs`
- `pull_requests` (id, repo_id, number, state, draft, base_ref, base_sha, head_ref, head_sha, head_repo_id, mergeable_state, merge_state_status, …)
- `pr_participants`, `pr_labels`, `pr_assignees`, `pr_reviewers`, `pr_projects`, `pr_milestones`
- `commits`, `pr_commits` (junction with order)
- `comments` (id, pr_id, kind: issue|review|review_thread_reply, author_id, body, created_at, updated_at, in_reply_to_id, …)
- `review_threads` (id, pr_id, path, line, side, start_line, start_side, original_commit_sha, original_path, original_position, is_outdated, is_resolved, resolved_by_id)
- `reviews` (id, pr_id, author_id, state, body, submitted_at)
- `checks` → `check_suites`, `check_runs`, `check_annotations`
- `pr_files` (pr_id, head_sha, path, old_path, status, additions, deletions, viewed_by_account_id, viewed_at_head_sha)
- `pr_patches` (pr_id, head_sha, patch_blob_sha) — patch text in blob store
- `notifications` (id, account_id, reason, subject_type, subject_id, unread, last_read_at)
- `worktrees` (id, repo_id, path, head_sha, branch, dirty, ahead, behind, mapped_pr_id, mapping_confidence, mapping_source)
- `pending_mutations` (id, account_id, kind, target_type, target_id, idempotency_key, input_json, optimistic_patch_json, inverse_patch_json, status, retries, created_at, last_error)
- `id_mappings` (local_id, server_id, kind)
- `sync_cursors` (account_id, resource, cursor, etag, last_fetched_at)
- `rate_limit_buckets` (account_id, resource, remaining, reset_at)
- `blob_refs` (sha256, kind, size, ref_count)

Denormalized read models: `pr_inbox_rows`, `pr_detail_summary`, `unread_counts`, `file_tree_summary`. FTS5 virtual tables on PR title/body, comments, review bodies, filenames, authors.

**Diff representation**: dual-level. Store the canonical unified-diff patch per `(pr_id, head_sha)` in `pr_patches` (text → zstd-compressed blob store). Store per-commit patches too for commit browsing and stacked-PR review. "What changed since I last looked" = textual diff between `last_viewed_head_sha` patch and current patch.

**Outdated threads**: trust GitHub's `isOutdated`. Store both original coordinates (`original_commit_sha`, `original_path`, `original_position`) and current coordinates (`path`, `line`, `side`, `start_line`, `start_side`) verbatim from the server. Do not locally re-anchor; show best-effort "where this comment was anchored" as a UI hint only.

**Blob store**: content-addressed under the Tauri app data dir (`blobs/<sha256-prefix>/<sha256>`). Holds patches, attached images, raw check-run logs, rendered markdown HTML caches. SQLite holds only `sha256` references. Reference-counted; eviction by LRU when total blob size exceeds a configurable cap.

### 5.4 GitHub API strategy

**Boundary**:
- **GraphQL** for: PR detail (one fat query covering title/body/timeline/threads/reviews/check summary/labels/assignees/reviewers/mergeable/headRef/baseRef), inbox refresh (thin query batching node IDs), review thread fetch + resolve/unresolve, projects v2, node hydration.
- **REST** for: unified diff (`Accept: application/vnd.github.v3.diff`), check-run annotations (`/check-runs/{id}/annotations`), notifications (`/notifications`), branch protection, repo lists with conditional caching, force-push range-diff inputs via `/compare`, raw file content.

**Two canonical queries**:
- `PrDetail` — one fat query, versioned shape, hand-written and reviewed centrally. No per-component GraphQL.
- `InboxRefresh` — thin query batching ~20 PR node IDs with only fields the inbox row renders.

**Auth path order**:
1. Detect `gh auth token` and offer one-click import with explicit scope display (Grok's friendlier ordering).
2. OAuth Device Flow as default for new users.
3. PAT for enterprise/constrained setups.

Tokens stored exclusively in OS keychain via the `keyring` crate. Never in SQLite. Per-account everything.

**Known sharp edges, designed-in**:
- `mergeable: null` → dedicated re-poll task with backoff 2s, 5s, 15s, 45s, 2min, max 5min.
- Review threads + resolve/unresolve: GraphQL only.
- Check annotations: separate REST call, paginated.
- Force-push range-diff: prefer local `git range-diff` if a worktree exists; else fetch the two head diffs and compute server-side in Rust.
- Large PRs exceed the REST files endpoint's hard limits: graceful degradation with "open on GitHub" escape hatch.
- Suggestion application: prefer GitHub's server-side mutation if it exists for single suggestions (verify in spike); for batched, require clean worktree and commit locally before push.

### 5.5 Diff viewer architecture

- **Patch source**: GitHub REST `.diff` is canonical. Local git is supplemental for range-diff, blame, history, and suggestion application. Anchoring uses GitHub's coordinates verbatim.
- **Renderer**: custom virtualized line component built on TanStack Virtual. Not Monaco (too heavy), not diff2html / react-diff-view (don't handle review threads, suggestion blocks, viewed checkboxes, per-line annotations).
- **Inline editor surfaces** (composing a comment, suggestion block, multi-line comment): CodeMirror 6.
- **Syntax highlighting**: Tree-sitter compiled to wasm, run in a Web Worker. Lazy by viewport. Hard cap: skip highlighting for files >5000 lines unless user opts in. Token cache keyed by `(language, content_hash)`.
- **Comment anchoring**: GitHub's `line`/`side`/`start_line`/`start_side` (modern model); store `position` only as legacy field if returned. Server's `isOutdated` is authoritative.
- **Selection-to-comment**: compute target `(file, side, line, start_line)`; send straight to GitHub; never invent local anchors.
- **Suggested-change apply**: single-suggestion via GitHub API if supported (verify in spike); batched via local worktree commit + push, gated on clean worktree.

### 5.6 Notifications

Two-channel:
1. **Local change-detection** via `/notifications` polling (focused: 60s; background: 5min) with `If-Modified-Since`. Treated as a *signal*, not the inbox itself.
2. **Per-active-PR polling** for sub-10s freshness on the visible PR.

OS notifications via Tauri's notification plugin, generated by a local rule engine after sync reconciliation (not directly from GitHub events — so we can dedupe and add app-specific triggers like "CI flipped", "conflict appeared", "mutation failed").

Configurable triggers: review requested, changes requested, approved, mention, CI failed/recovered, merge conflict, mutation failure. Quiet hours + focus mode + per-repo filters. Dedup keyed on `(account, repo, pr, event_type, actor, server_event_id)`.

Webhook relay: opt-in, M6, self-deploy script (Cloudflare Worker + Durable Object recommended; Fly.io as alternative). Never SaaS.

### 5.7 Local worktree integration

- **Discovery**: configured root dirs (`~/dev`, `~/code`, `~/src`, `~/repos` defaults, user-extensible). For each known repo, `git worktree list --porcelain` is authoritative. Optional `.github-pr-cockpit.toml` per worktree for overrides. No `$HOME` autoscan.
- **Freshness**: `notify` crate FS watchers on `.git/HEAD`, `.git/refs/`, `.git/index`, and the working tree. Debounce 200–500ms, then run `git status --porcelain=v2 --branch` and `git rev-list --left-right --count`. No periodic polling.
- **Branch ↔ PR mapping**: multi-signal, confidence-scored (GPT's framing). Signals: remote URL match, branch upstream, `gh pr status` output, exact head SHA, branch name conventions (`fork-owner/branch`, `pr/123`), PR head SHA ancestry. UI shows the inferred mapping with a confidence indicator and "link this worktree to PR" manual override.
- **App-created vs. user-managed worktrees**: tracked with ownership metadata in the `worktrees` table. Cleanup operations are blocked on user-managed worktrees and require confirmation + dirty-state snapshot for app-created ones. Never delete uncommitted work.

### 5.8 Multi-account / multi-org

- Account model: `(host, login, token, scopes, token_kind, created_at)`.
- Every viewer-dependent field is account-scoped: viewed state, notifications, available merge methods, permissions, project access, rate-limit counters, pending mutations.
- Token storage: OS keychain only.
- UI: account switcher in the sidebar; per-row badges only when the inbox spans multiple accounts; mutation composer always shows the posting identity with quick-switch before submission. Global "all accounts" aggregated inbox view.
- GitHub Enterprise Server: schema designed for it (host field, per-host endpoint config) but full support is a stretch goal, not v1.

### 5.9 Performance targets

| Target | Budget |
|---|---|
| Inbox first paint (warm cache) | <100ms |
| Inbox fully refreshed | <800ms p50 |
| PR detail open (preloaded) | <50ms |
| PR detail open (cold cache hit) | <250ms |
| File open in diff (cached) | <100ms |
| File open with highlighting (viewport) | <300ms |
| Comment submit visible | <16ms (single frame) |
| Command palette open | <75ms |
| Command palette result | <150ms |
| Diff scroll | 60fps on ≤10k-line diffs |

Levers: virtualized lists everywhere (TanStack Virtual); Rust-side SQLite reads with coalesced IPC invalidation events; Tree-sitter highlighting in Web Workers; preloading PR detail on inbox-row hover; per-PR selectors (Zustand + shallow equality); content-hash caching for rendered markdown; CI-enforced perf benchmarks (headless render) gated on every PR to the cockpit itself.

### 5.10 Tech stack inside Tauri

- **Backend**: Rust + Tauri v2. `sqlx` for SQLite, `r2d2` pool, `keyring` for tokens, `notify` for FS watchers, `octocrab` (or hand-written reqwest client) for GitHub API, `git2` for local git, `comrak` for markdown, `ammonia` for sanitization, `tree-sitter` + grammar wasms for highlighting tokens passed to JS.
- **Frontend**: React + TypeScript + Vite. **Reasoning**: the user's Lexical preference is decisive — Lexical is React-first. CodeMirror 6, TanStack Virtual, Radix primitives, react-arborist for the stacked-PR tree are all React-first. Svelte would be ergonomically nicer for a SQLite-live-query layer but the integration debt for Lexical alone outweighs the gain.
- **State**: Zustand for UI state. Server state arrives through a live-query layer that wraps Tauri commands (request/response) and Tauri events (pub/sub invalidations). No React Query, no Redux. Components subscribe per-query, not per-row, and the Rust side coalesces invalidation events to "pr:123 changed" granularity.
- **IPC**: Tauri commands for writes; Tauri events for invalidations. Auto-generated TS types from Rust structs via `specta` or `tauri-bindgen` — eliminates the entire class of "we forgot to update the TS type" bugs.
- **Diff renderer**: custom on top of TanStack Virtual, with CodeMirror 6 embedded for inline comment composition. No Monaco.

**Markdown editor + renderer pipeline (with Lexical):**

| Approach | Pros | Cons | Verdict |
|---|---|---|---|
| Lexical compose + Lexical render | Editor/render fidelity is perfect; one AST | Reimplement GFM tables, footnotes, math, alerts, suggestion fences, autolinks, syntax highlighting as Lexical nodes; large ongoing maintenance to stay aligned with cmark-gfm; no parity oracle | Reject |
| Lexical compose → markdown text → comrak (Rust) render → React hydrate | Mature GFM via cmark-gfm-compatible engine; one render pipeline shared across timeline, preview, and stored caches; security boundary in Rust; cacheable by content hash | Lexical→markdown serialization must be lossless for GFM (custom transformers needed for suggestion blocks, alerts, math, mentions) | **Adopt** |
| Lexical compose → markdown text → remark/rehype (JS) render | All JS, easy interactive extensions, big plugin ecosystem | Render runs in webview (perf-relevant on large timelines); sanitization in JS (DOMPurify); divergence from cmark-gfm is harder to pin down; harder to cache across surfaces | Reject as primary; keep as escape hatch |
| markdown-it + custom plugins, no Lexical | Battle-tested | Composer becomes a textarea-plus-toolbar, losing slash commands, mention chips, autocomplete fidelity | Reject (loses the Lexical win) |

**Locked pipeline**:
1. **Composer**: Lexical, with custom nodes/transformers for mentions, issue refs, SHA refs, emoji shortcodes, slash commands, suggestion blocks, alerts (`> [!NOTE]`), task lists, `<details>`, math, paste-image-as-upload, drag-and-drop attachments, saved replies. Autocomplete sources query SQLite (cached users, issues, emoji).
2. **Serialization**: Lexical state → canonical GFM markdown text via custom transformers. This markdown is what gets stored, sent to GitHub, and rendered everywhere. The Lexical JSON state is *not* persisted as the source of truth — markdown is. (Round-trip lossless serialization is a verify-next item.)
3. **Renderer**: comrak (Rust, cmark-gfm-compatible: tables, task lists, autolinks, strikethrough, footnotes, math) + post-processors for GitHub-specific extras not in comrak (alerts, suggestion fences, `@user`/`#123`/SHA autolinks beyond plain URL) + syntect / tree-sitter for fenced code highlighting + ammonia for HTML sanitization. Output: sanitized HTML blobs, cached by `(content_hash, renderer_version)` in the blob store.
4. **Hydration**: React mounts the HTML and attaches interactive behaviors — task-list checkbox toggle (fires a mutation), suggestion `Apply` button, hover preview for `@user`/`#issue`/SHA, auth'd image loader, copy-link-to-comment.
5. **Live preview in composer**: rendered through the same comrak pipeline via an IPC call → parity with the final timeline render is guaranteed by construction.
6. **GitHub markdown REST endpoint**: used in CI as a parity oracle on a corpus of real comments; not on the runtime path.

### 5.11 Stacked PR support

- **Detection**: at sync time, walk PRs where `base.ref == another_open_pr.head.ref` in the same repo. Cache `stack_id` and `stack_position` on each PR row. Diamond/ambiguous graphs degrade to a DAG view with a warning.
- **Rendering**: linear-stack-first sidebar tree (most stacks are linear); per-PR review state, CI state, conflict state, and "blocked by" reasons; explicit base/head SHAs.
- **Operations**: shell out to local git for `rebase stack` (sequential `git rebase`), `merge stack` (sequential merge in order, with merge-queue if enabled). On conflict, pause and surface the worktree for manual resolution. Retarget base via GraphQL `updatePullRequest(baseRefName:…)` after a merge.
- **Integration**: defer to Graphite CLI (`gt`) if installed and the user opts in; otherwise plain `git`. M6 feature, not v1.

### 5.12 MVP slicing

Six milestones, each independently useful. No week estimates — markdown parity alone is unpredictable.

- **M1 — Read-only cockpit.** Auth (gh import + device flow), repo selection, SQLite cache, sync engine v1, rate-limit budgeter, fat GraphQL query, inbox + PR detail (description, timeline, labels, reviewers, check status), basic unified/side-by-side diff with Tree-sitter highlighting, full GFM rendering via comrak. **Success bar**: open app → real inbox in <100ms; click any PR → read description + timeline + diff with correct rendering; 60fps scroll on a 5k-line diff. No writes.
- **M2 — Optimistic write surface.** Pending-mutation engine, per-kind handlers for: comments (issue + review), reactions, labels, assignees, reviewers, mark file viewed, resolve/unresolve threads, submit review (approve/request changes/comment-only), pending review batching. Inline error UX, global sync-errors tray, retry/discard, offline queue. Lexical composer with slash commands, mention/issue/emoji autocomplete, live preview.
- **M3 — Diff polish + worktree read.** Multi-line comments, suggestion blocks (compose), viewed checkboxes, side-by-side polish, image/binary/rename handling, large-file degradation. Local worktree discovery, `notify` watchers, dirty/ahead/behind, jump-to-editor/terminal. Notifications (in-app inbox + native OS via Tauri).
- **M4 — Merge surface + force-push + multi-account.** Merge / squash / rebase respecting repo settings, edit squash message, auto-merge, merge queue enqueue, delete branch after merge, update branch. Force-push detection + range-diff (local `git range-diff` if worktree, else fetched). Multi-account UI + per-account rate-limit meters.
- **M5 — Editor power + worktree write.** Suggested-change apply (single via API, batched via local commit on clean worktree), saved replies, full command palette, keyboard layer, paste-image-as-upload (after verifying API), drag-and-drop attachments. Check annotations on the diff, failed-job log tail, rerun checks where permitted.
- **M6 — Stacked PRs + webhook relay + polish.** Stack detection + tree rendering + rebase/merge operations (plain git, optional Graphite). Self-deploy webhook relay script. Markdown parity edge-case polish via CI corpus. GHE compatibility pass.

### 5.13 Top risks + mitigations

1. **GraphQL rate-limit exhaustion on large orgs.** Mitigation: tiered polling, per-account budgeter with back-pressure, two centrally-owned fat queries (no ad-hoc per-component queries), Notifications API as cheap change-detector, opt-in repo subscriptions, rate-limit meter in the status bar, optional webhook relay.
2. **Comment anchoring divergence across force-push / rebase.** Mitigation: store GitHub's `line`/`side`/`start_line`/`start_side` + `original_*` verbatim, trust `isOutdated`, never invent local anchors. Local re-matching is best-effort UI hint only. Real-PR force-push test corpus.
3. **Markdown rendering fidelity gap.** Mitigation: comrak + cmark-gfm-equivalent extensions + custom post-processors for GitHub extras; CI visual-regression corpus of ~200 real-comment renders against github.com; GitHub's markdown REST endpoint as oracle; explicit fidelity bar (~95%, document the gap); "open on github.com" escape hatch on every comment.
4. **Optimistic update correctness under multi-write + flaky network.** Mitigation: persisted log + projected state + inverse patches + per-kind handlers (bounded ~25); property-based tests over `(mutations × responses × conflicts)`; airplane-mode dogfooding; failed-mutation tray is always legible.
5. **Tauri webview perf on large diffs and large markdown.** Mitigation: virtualization everywhere; off-main-thread Tree-sitter; Rust-side markdown render; hard caps with graceful degradation; CI-enforced perf benchmarks on WebKitGTK specifically (Linux is worst case). Escape hatch if it fails: native Rust diff renderer in a separate Tauri window using `egui` — expensive but exists.
6. **Local worktree automation damaging user work.** Mitigation: ownership metadata distinguishes app-created from user-managed worktrees; cleanup requires confirmation; dirty-state snapshot before any destructive op; never delete unmanaged worktrees; suggestion-batch-apply gated on clean worktree.
7. **Lexical ↔ markdown serialization fidelity.** Not in the original prompt but added because of the editor choice. Mitigation: spike a round-trip test corpus before committing to Lexical; custom Lexical transformers for every GFM extension we render; markdown text (not Lexical JSON) is the persisted source of truth, so any serialization bug is fixable without data migration.

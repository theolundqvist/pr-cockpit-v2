# Changelog — PR Cockpit

Format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [1.0.0] — 2026-05-18

### Added — M1: Read-only cockpit

- Authentication via `gh` import and OAuth device flow fallback, including
  multi-account support.
- Local-first SQLite sync/read models for inbox and PR detail surfaces.
- Inbox + PR detail + side-by-side/unified diff with tree-sitter highlighting
  that sustains 60 fps diff scrolling on 5k-line fixtures.
- Markdown corpus gate established with regression control at ≤ 2%.

### Added — M2: Optimistic writes

- 25+ mutation kinds shipped with predict/apply/rollback/reconcile flow.
- Property-test coverage for mutation algebra and conflict handling.
- Airplane-mode drill for queued offline writes and reconnect replay.
- Composer textarea + Preview path unified through Rust/comrak rendering.

### Added — M3: Diff polish + worktree + notifications

- Multi-line comments, suggestion block composition, viewed-file toggles, and
  image/binary/rename diff rendering.
- Worktree discovery plus `notify`-driven freshness and confidence-scored PR
  mapping.
- Native OS notifications with dedup, quiet hours, focus mode, and rule filters.
- Markdown corpus tightened to ≤ 1.5%.

### Added — M4: Merge surface + multi-account

- Merge/squash/rebase, auto-merge, merge queue, and delete-branch flows with
  branch-protection-aware no-optimism UX.
- Force-push range-diff support with local-git preference and REST `/compare`
  fallback.
- Per-account rate-limit budgeter and aggregated multi-account inbox, including
  composer posting identity and rate-meter foreground bypass cues.
- GHE schema readiness and `mergeable: null` backoff handling.

### Added — M5: Editor power + worktree write

- Single-suggestion apply via API, plus batched suggestion apply via clean
  worktree patch/commit/push flow.
- Saved replies (per-account), command palette, and full keyboard command layer
  with performance-gated open/result latency.
- Paste-image upload pipeline for GitHub user-content URLs.
- Diff check annotations, failed-job log tail streaming, and rerun-check actions.

### Added — M6: Stacked PRs + webhook relay + polish

- Stack detection and sidebar stack tree with linear-first rendering and DAG
  warning for ambiguous topology.
- Sequential stack rebase/merge operations with conflict pause surface,
  Resume/Abort flow, and GraphQL base retarget sequencing.
- Optional Graphite (`gt`) integration when installed and enabled.
- Self-deploy Cloudflare Worker relay recipe for signed webhook forwarding to a
  local Tauri receiver (no SaaS relay path).
- GHE compatibility pass across M1–M5 happy paths against stubbed host fixtures.
- Demo GIF in README, MIT license, changelog publication, and markdown corpus
  tightened to ≤ 1.0%.

### Performance

- All PLAN §10 hard budgets are green on the v1.0 benchmark run; full numbers,
  methodology, and trend notes are in [PERF_REPORT.md](PERF_REPORT.md).

### Security

- Tokens remain keychain-only across dotcom, multi-account, and GHE host paths.
- Relay architecture is self-deploy only and uses signed forwarding with replay
  protection (no hosted SaaS relay).
- Security-sensitive flows are covered by integration-test audits and regression
  checks.

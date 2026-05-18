# pr-cockpit

Local-first GitHub PR cockpit (Tauri v2 + Svelte 5 + Rust + SQLite).

This repo is being built by Cursor cloud agents under `/orchestrate`.

## Contract documents

Read in this order:

1. `AUTONOMY_BRIEF.md` — the brief. Acceptance criteria per milestone, quality bars, escalation rules.
2. `PLAN.md` — the engineering contract. Architecture, sync engine, mutation model, perf budgets. Edit it when reality forces a change; don't drift silently.
3. `synthesis.md` — the source synthesis the plan derives from.
4. `DECISIONS.md` — non-obvious calls, dated.

Status: **M5 editor power + worktree write + check annotations complete**.
Milestone M6 continues per the brief and `PLAN.md`.

## Feature highlights

- Local-first PR cockpit backed by SQLite denormalized read models.
- Optimistic mutation pipeline with replay-safe queueing and inverse-patch rollback.
- Composer with textarea + Preview parity via the shared comrak renderer path.
- Suggestion apply is shipped for both single and batched flows: single suggestion
  uses the GitHub mutation path, and batched apply is gated on clean worktree
  checks with local commit + push sequencing (proofs:
  `apps/desktop/playwright/m5-suggestion-apply.spec.ts`,
  `apps/desktop/src-tauri/tests/worktree_write.rs`,
  `apps/desktop/src-tauri/tests/mutations_harness.rs`).
- Saved replies are shipped as per-account presets with settings CRUD, composer
  insertion, and quick-switch palette integration (proofs:
  `apps/desktop/playwright/m5-saved-replies-paste-image.spec.ts`,
  `apps/desktop/src-tauri/tests/saved_replies.rs`).
- Command palette + keyboard layer are fully shipped across inbox and PR detail,
  including PR-number jump, account switching, apply/resolve/view commands, and
  open-in-github flows (proofs:
  `apps/desktop/playwright/m5-command-palette.spec.ts`,
  `apps/desktop/playwright/m5-a11y.spec.ts`,
  `tools/perf-bench/command-palette.mjs`).
- Paste-image upload is shipped from clipboard to GitHub user-content URL with
  placeholder replacement while preserving composer/Preview parity (proofs:
  `apps/desktop/playwright/m5-saved-replies-paste-image.spec.ts`,
  `apps/desktop/src/lib/components/Composer.paste-image.test.ts`,
  `apps/desktop/src-tauri/tests/image_uploads.rs`).
- Check annotations are shipped inline on diff lines with failed-job log tail
  streaming and rerun-check/run-suite actions (proofs:
  `apps/desktop/playwright/m5-check-annotations.spec.ts`,
  `apps/desktop/src-tauri/tests/check_annotations_sync.rs`,
  `apps/desktop/src-tauri/tests/check_log_stream.rs`,
  `apps/desktop/src-tauri/tests/rerun_check_run.rs`).
- Diff polish is fully shipped: multi-line inline review comments, suggestion
  block insertion/preview, head-SHA-scoped viewed files, and image/binary/rename
  rendering in the diff surface (proofs:
  `apps/desktop/playwright/diff-polish.spec.ts`,
  `apps/desktop/src-tauri/tests/diff_polish_viewed_files_head_sha.rs`,
  `apps/desktop/src-tauri/tests/mutations_review_comments.rs`).
- Worktree read integration is shipped: bounded root discovery, confidence-scored
  PR mapping with manual override, and cleanup safety contracts (proofs:
  `apps/desktop/src-tauri/tests/worktree_discovery.rs`,
  `apps/desktop/src-tauri/tests/worktree_mapping_signals.rs`,
  `apps/desktop/src-tauri/tests/worktree_cleanup_safety.rs`,
  `apps/desktop/playwright/worktree.spec.ts`).
- Notification rule engine + native dispatch are shipped: dedup, quiet hours,
  focus mode, and per-repo filtering (proofs:
  `apps/desktop/src-tauri/tests/notifications_*.rs`,
  `apps/desktop/playwright/notifications.spec.ts`).
- Merge surface is shipped with repo-settings-aware merge/squash/rebase controls,
  auto-merge enable/disable, merge queue enqueue/dequeue/reorder, update-branch,
  and delete-branch-after-merge no-optimism sequencing (proofs:
  `apps/desktop/playwright/m4-merge-surface.spec.ts`,
  `apps/desktop/src-tauri/tests/mutations_harness.rs`,
  `apps/desktop/src-tauri/tests/sync_integration.rs`).
- Force-push range-diff is shipped with local `git range-diff` preference and
  REST fallback rendering parity (proofs:
  `apps/desktop/playwright/m4-range-diff.spec.ts`,
  `apps/desktop/src-tauri/tests/range_diff_local_git.rs`,
  `apps/desktop/src-tauri/tests/range_diff_rest_compare.rs`,
  `apps/desktop/src-tauri/tests/range_diff_fallback.rs`).
- Multi-account UX is shipped with aggregated inbox badges, composer posting
  identity quick-switch, and per-account rate-limit meters/bypass signaling
  (proofs:
  `apps/desktop/playwright/m4-multi-account.spec.ts`,
  `apps/desktop/src-tauri/tests/multi_account_inbox.rs`,
  `apps/desktop/src-tauri/tests/multi_account_rate_limit.rs`,
  `apps/desktop/src-tauri/tests/composer_posting_identity.rs`).
- GHE schema readiness is shipped for host-aware endpoint/auth plumbing against
  a stubbed enterprise host (proofs:
  `apps/desktop/playwright/m4-ghe.spec.ts`,
  `apps/desktop/src-tauri/tests/ghe_endpoint_derivation.rs`,
  `apps/desktop/src-tauri/tests/ghe_round_trip.rs`,
  `apps/desktop/src-tauri/tests/ghe_token_storage.rs`).
- Offline/airplane behavior: queued safe mutations replay on reconnect with
  explicit connection-required affordances for unsafe writes (proof drill:
  `apps/desktop/src-tauri/tests/airplane_drill.rs`).

## Quick start (desktop cockpit)

```bash
pnpm install
pnpm --filter desktop tauri dev
```

## Offline fixtures mode

Deterministic offline verification is supported against committed fixtures:

- Fixture generator and assets:
  `apps/desktop/src-tauri/fixtures/README.md`
- The fixture SQLite + blob corpus used by tests/bench:
  `apps/desktop/src-tauri/fixtures/`

For headless checks without a desktop display server, run:

```bash
pnpm --filter desktop test:smoke
```

## CI quality gates (M5)

The required local/CI gate matrix is:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm svelte-check
pnpm lint
pnpm test
pnpm bench
pnpm corpus
```

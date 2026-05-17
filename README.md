# pr-cockpit

Local-first GitHub PR cockpit (Tauri v2 + Svelte 5 + Rust + SQLite).

This repo is being built by Cursor cloud agents under `/orchestrate`.

## Contract documents

Read in this order:

1. `AUTONOMY_BRIEF.md` — the brief. Acceptance criteria per milestone, quality bars, escalation rules.
2. `PLAN.md` — the engineering contract. Architecture, sync engine, mutation model, perf budgets. Edit it when reality forces a change; don't drift silently.
3. `synthesis.md` — the source synthesis the plan derives from.
4. `DECISIONS.md` — non-obvious calls, dated.

Status: **M3 diff polish + worktree read + notifications complete**. Milestones
M4 → M6 continue per the brief and `PLAN.md`.

## Feature highlights

- Local-first PR cockpit backed by SQLite denormalized read models.
- Optimistic mutation pipeline with replay-safe queueing and inverse-patch rollback.
- Composer with textarea + Preview parity via the shared comrak renderer path.
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

## CI quality gates (M3)

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

# M3 verifier — m3-finalize-merge

Target branch: `orch/m3-diff-worktree-notifs/m3-finalize-merge`
Bookkeeping alias: `orch/pr-cockpit/m3-diff-worktree-notifs` (same HEAD)
HEAD: `46cfd8b1f75d166a1a10398b5751d0376c4e4271`

## Reproduction recipe

Environment setup (already-installed deps documented in plan):

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
    libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf \
    build-essential xvfb pkg-config
pnpm install --frozen-lockfile
pnpm --filter desktop exec playwright install --with-deps webkit chromium
```

Verification commands (run from repo root):

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm ipc:bindings && pnpm typecheck
pnpm svelte-check
pnpm lint
pnpm test
pnpm bench
pnpm corpus
( cd apps/desktop && xvfb-run -a pnpm exec playwright test \
    playwright/airplane.spec.ts playwright/m2-smoke.spec.ts \
    playwright/diff-polish.spec.ts playwright/worktree.spec.ts \
    playwright/notifications.spec.ts playwright/a11y-pr-detail.spec.ts \
    playwright/m3-smoke.spec.ts )
```

## Captures committed under `verify/m3/`

- `cargo_test.log` — full `cargo test --workspace` run (73 passed / 0 failed).
- `bench.log` — `pnpm bench` output incl. all PLAN.md §10 + M3 budgets green.
- `playwright.log` — required Playwright suite headless under xvfb (7 passed).
- `smoke/01-...png` … `smoke/05-...png` — M3 live smoke screenshots reproduced
  by the verifier's `playwright/m3-smoke.spec.ts` run (mirror of the worker's
  capture at `artifacts/m3-smoke/`).
- `worktree/01-inbox-worktree-roots.png`, `worktree/02-pr-worktree-mapping.png`
  — verifier reproduction of the `playwright/worktree.spec.ts` capture.

## Key invariants confirmed by source

- AddReviewComment mutation: `apps/desktop/src-tauri/src/mutations/handlers/review_comments.rs:32` declares `OptimismLevel::Full`. GraphQL: `apps/desktop/src-tauri/src/api/queries/mutations/addReviewComment.graphql`. Canonical-queries guard: `tests/canonical_queries.rs` (asserts the file lives in the canonical directory).
- Viewed-file head-SHA scoping: `apps/desktop/src-tauri/src/db/mod.rs:220-224` and `migrations/0007_diff_polish.sql:27-30` both compute `is_viewed = (viewed_by_account_id IS NOT NULL AND viewed_at_head_sha = pr.head_sha)`.
- Asset-protocol scope: `apps/desktop/src-tauri/tauri.conf.json` (`scope: ["$APPDATA/blobs/**/*"]`) + `capabilities/default.json` (fs:scope and fs:allow-read-file both tightly limited to `$APPDATA/blobs/**/*`).
- Worktree watcher uses `notify` (`watcher.rs:7`), 200–500 ms debounce (`watcher.rs:285`: `200 + (hasher.finish() % 301)`), `git status --porcelain=v2 --branch` + `git rev-list --left-right --count` (`watcher.rs:313, 406`).
- Worktree cleanup safety: `apps/desktop/src-tauri/src/worktree/mod.rs:269-327` returns `CleanupError::UserManaged` first; even when `force=true`, dirty worktrees only snapshot (`would_remove: false`, `removed: false`) and clean worktrees only return `would_remove: true` without calling `git worktree remove`. Test: `tests/worktree_cleanup_safety.rs::cleanup_safety_gates_fail_closed`.
- Notification dedup UNIQUE constraint: `migrations/0009_notifications.sql:17`. INSERT OR IGNORE: `src/notify/dedup.rs:49`. Rule engine fires post-reconcile via `SyncReconciledEvent` broadcast (`src/notify/mod.rs:90-127`).
- Renderer isolation: no `@tauri-apps/plugin-notification` or `sendNotification`/`isPermissionGranted` references in `apps/desktop/src` (grep returned zero hits). `tests/renderer_isolation.rs` enforces the broader renderer-isolation contract.
- Corpus gate at 0.015: `tools/markdown-corpus/score.mjs:12` (the `pnpm corpus` script runs the `.mjs`).
- Token-leak grep on M3 surfaces (`worktree/`, `notify/`, `viewed_files.rs`, `review_comments.rs`, M3 svelte components, `ipc/mod.rs`) → 0 hits (the only `token` strings are `mention_token` for @-mention parsing).

## Findings of note

1. Tab-order asserted by `a11y-pr-detail.spec.ts` is `title → description → composer → timeline → right rail → merge box`, while the planner stated `title → description → timeline → composer → right rail → merge box`. The spec passes against the implementation; the implementation places the composer above the timeline. Not flagged as blocking — the spec still proves keyboard reachability, accessible naming, and `:focus-visible` ring on PR detail.
2. Bench `bench/budgets.json` lists `comrak_render_throughput_ops_per_sec` budget `850` with baseline `15000`. Actual measured: `36564.14 ops/s` ≫ both. PLAN.md §10 perf-budget invariants therefore all green.

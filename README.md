# pr-cockpit

Local-first GitHub PR cockpit (Tauri v2 + Svelte 5 + Rust + SQLite).

This repo is being built by Cursor cloud agents under `/orchestrate`.

## Contract documents

Read in this order:

1. `AUTONOMY_BRIEF.md` — the brief. Acceptance criteria per milestone, quality bars, escalation rules.
2. `PLAN.md` — the engineering contract. Architecture, sync engine, mutation model, perf budgets. Edit it when reality forces a change; don't drift silently.
3. `synthesis.md` — the source synthesis the plan derives from.
4. `DECISIONS.md` — non-obvious calls, dated.

Status: **M1 read-only cockpit complete**. Milestones M2 → M6 continue per the
brief and `PLAN.md`.

## Quick start (desktop cockpit)

```bash
pnpm install
pnpm --filter desktop tauri dev
```

## Offline fixtures mode

M1 supports deterministic offline verification against committed fixtures:

- Fixture generator and assets:
  `apps/desktop/src-tauri/fixtures/README.md`
- The fixture SQLite + blob corpus used by tests/bench:
  `apps/desktop/src-tauri/fixtures/`

For headless checks without a desktop display server, run:

```bash
pnpm --filter desktop test:smoke
```

## CI quality gates (M1)

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

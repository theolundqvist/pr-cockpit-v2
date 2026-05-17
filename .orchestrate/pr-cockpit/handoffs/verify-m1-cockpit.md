<!-- orchestrate handoff
task: verify-m1-cockpit
branch: `orch/pr-cockpit/m1-cockpit` (verifier artifacts committed under `verify/RERUN_AUDIT.md` + `verify/logs-rerun/`, pushed)
agentId: bc-efdde0d3-d6b9-4e54-9500-b7dfa529c577
runId: run-480c8eed-849d-4af7-88b2-967b6845cff1
resultStatus: finished
finishedAt: 2026-05-17T14:09:31.674Z
-->

## Verification
unit-test-verified

## Target
`m1-cockpit` on branch `orch/pr-cockpit/m1-cockpit`

## Branch
`orch/pr-cockpit/m1-cockpit` (verifier artifacts committed under `verify/RERUN_AUDIT.md` + `verify/logs-rerun/`, pushed)

## Execution
- `pnpm install --frozen-lockfile` → ok (359 packages, lockfile up-to-date).
- `cargo fmt --check` → exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0, 0 warnings.
- `cargo test --workspace` → exit 0, 16 test binaries, 29 tests passed incl. `token_safety`, `ipc_token_safety`, `renderer_isolation`, `canonical_queries`, `sync_integration` (×6), `db_integration` (×5), `render` (×8), `perf_smoke`, `ipc_bindings`, `online_demo`.
- `pnpm typecheck` (regen specta bindings → `svelte-kit sync && tsc --noEmit`) → exit 0.
- `pnpm svelte-check` → exit 0, 0 errors / 0 warnings.
- `pnpm lint` (eslint + prettier) → exit 0.
- `pnpm test` (vitest) → 5 files / 6 tests passed.
- `pnpm corpus` → `weighted_mean=0.015459`, gate `0.02` → pass on 200-entry corpus.
- `xvfb-run -a pnpm bench` → all 11 PLAN §10 budgets met: inbox 10.96 ms (rust) / 22 ms (frontend) < 100 ms; pr_detail preloaded 0.94/17 ms < 50 ms; cold 8.17/19 ms < 250 ms; file-open 0.89/58 ms < 100 ms; comrak 37 380 ops/s ≥ 850; diff_scroll 62.17 fps ≥ 60; p95 16.20 ms ≤ 16.7.
- `xvfb-run -a pnpm --filter desktop exec playwright test` → 1/1 pass (33.2 s) — real WebKit, real Svelte 5 app served via `pnpm preview`: inbox → PR detail → unified diff → side-by-side → scroll to `LINE_5000`; 30 s WebM artifact in `apps/desktop/artifacts/playwright/`.
- Structural spot checks: 4 sqlx migrations present (`0001_initial_schema.sql` … `0004_auth_accounts.sql`); exactly two GraphQL files in `src-tauri/src/api/queries/` (`PrDetail.graphql`, `InboxRefresh.graphql`); `bindings.ts` 426 lines specta-generated; `bench/budgets.json` carries PLAN §10 hard budgets; corpus + 200 oracle HTML files committed; `DECISIONS.md` 17 KB with M1 entries.

## Findings
Per acceptance criterion:
- [x] Tauri v2 + Svelte 5 + TS + Vite + Rust crate scaffold builds clean: rust clippy / typecheck / svelte-check / vitest / playwright all green locally; CI matrix run 25990142848 + 25992045585 green on both ubuntu and macos (met).
- [x] rustfmt + clippy `-D warnings` + pnpm typecheck + svelte-check pass: all four exit 0 (met).
- [x] CI workflow runs rust + frontend unit/integration + headless perf bench + markdown corpus regression: `.github/workflows/ci.yml` defines all jobs; latest green CI run 25992045585 (met).
- [x] Auth (gh / OAuth Device Flow / PAT) with keychain-only tokens: `token_safety.rs` and `ipc_token_safety.rs` integration tests both pass; cover all 3 paths and assert no token bytes in SQLite/WAL/tracing (met).
- [x] Multi-account model: `0004_auth_accounts.sql` present; tests verify account list/switch/remove via IPC (met).
- [x] Sync engine: 6 `sync_integration` tests pass — tiered scheduler with focus pause + foreground bypass, rate-limit budgeter, `If-None-Match` ETag 304, `/notifications` conditional poller honoring `If-Modified-Since` + `X-Poll-Interval`, mergeable-null backoff (met).
- [x] Two canonical queries, no per-component GraphQL: `canonical_queries` test passes — asserts exactly two `.graphql` files in canonical dir and no `graphql!` macro elsewhere (met).
- [x] SQLite migrations cover PLAN §4 + denormalized read models + FTS5 + blob store: 4 migrations including `0002_read_models.sql` and `0003_search_fts.sql`; `db_integration` tests pass against fixture (met).
- [x] IPC: specta-generated TS types check; renderer never opens SQLite or talks to GitHub directly: `pnpm ipc:bindings` regenerates `bindings.ts` from specta; `renderer_isolation.rs` integration test passes (met).
- [x] Inbox cold-paint < 100 ms in headless perf bench on 200-row warm cache: rust 10.96 ms, frontend 22 ms (met).
- [x] PR detail renders description/timeline/labels/reviewers/checks + unified + side-by-side diff: Playwright smoke reached all four sections; screenshots `02–04` in committed `artifacts/m1-smoke/` (met).
- [x] Diff viewer: tree-sitter wasm in Web Worker, viewport-lazy; 60 fps on 5k-line: bench shows 62.17 fps, p95 16.2 ms; Playwright smoke confirms virtualized scroll reaches `LINE_5000` (met; see caveat on grammar availability below).
- [x] Comrak + ammonia pipeline + cache by `(content_hash, renderer_version)`: 8 `render` tests pass including `uses_render_cache_on_second_render` (met).
- [x] Markdown corpus regression ≤ 2%: `pnpm corpus` weighted_mean = 0.015459 on 200 real comments; corpus + oracle committed (met).
- [x] Perf benchmark harness enforces PLAN §10 and gates CI: `bench/budgets.json` + `tools/perf-bench/compare-budgets.mjs`; all 11 metrics under budget locally; latest CI perf-bench green (met).
- [x] DECISIONS.md updated with non-obvious M1 calls: 17 KB, 22+ dated entries, M2+ contract section, perf comparator split documented (met).

Other findings (severity-ordered):
- (med) Tree-sitter grammar wasms are **not checked in** to the repo — they live behind `scripts/fetch-grammars.sh`. During my Playwright smoke, the page hit `[404] GET /assets/grammars/tree-sitter-rust.wasm` three times and the highlighter silently fell back to plain text. The diff still renders and scrolls (acceptance criteria pass), and CI / the upstream `artifacts/m1-smoke/` recording fetch grammars first, but the M2 worker needs to keep `scripts/fetch-grammars.sh` in every perf/smoke pre-step or the highlighter regresses invisibly.
- (low) Diff fixture corpus only contains `.rs` and `.ts` files. The `.md` highlight path in the diff viewer is exercised by comrak (corpus regression) and the markdown timeline renderer, but not end-to-end through the in-diff tree-sitter highlighter. The strictest reading of PLAN §12 M1 ("tree-sitter highlighting on Rust/TS/Markdown fixtures") is incompletely covered in the diff viewer. The upstream handoff already flags this as a follow-up; cheap to fix in M2.
- (low) No real Tauri native window was booted (cloud-agent VM is headless WebKit only). The IPC, renderer, virtualized diff, comrak pipeline, and perf bench all use the same WebKit Tauri uses, so the platform surface is well covered, but a human-eye Tauri pass on macOS/Linux desktop is still owed before user-facing shipping. Matches the upstream verifier's same caveat.

## Notes & suggestions
- I am not rounding up to `live-ui-verified` despite the Playwright headless-WebKit smoke working end-to-end, because (a) there is no real Tauri window in this env, and (b) the tree-sitter highlighter silently regressed in my smoke due to the missing grammar wasm. This matches the upstream `m1-reverify` verdict.
- The M1 deliverable on `orch/pr-cockpit/m1-cockpit` is, by my evidence, ready for M2 to branch from. Every PLAN §12 M1 + AUTONOMY_BRIEF §M1 + task-level acceptance bullet is met with headroom on every PLAN §10 budget.
- Follow-ups worth queuing for M2: (1) bake `scripts/fetch-grammars.sh` into the dev/preview path (or check small grammar wasms in directly) so the highlighter cannot regress silently; (2) add a `.md` file to the seeded diff corpus so the in-diff tree-sitter pipeline is exercised end-to-end; (3) do at least one real-Tauri-window smoke on macOS/Linux desktop before M3 worktree work lands; (4) run the env-setup agent the upstream verifier proposed — the local install of Tauri Linux deps + Playwright WebKit was the single most expensive setup step this run, and every M2 worker will pay it again.
<!-- orchestrate handoff
task: m1-verify
branch: `cursor/verify-m1-finalize-6204` (verifier-only; pushed to origin with all execution logs and a one-shot lint reproducer under `verify/`)
agentId: bc-d0525f4c-97c5-4583-bf7a-ebda7c836204
runId: run-b14696d3-c661-4a9d-83fa-ce5bed4b9484
resultStatus: finished
finishedAt: 2026-05-17T11:28:52.819Z
-->

Now let's run the CI gates. I'll start with cargo fmt and clippy in parallel-ish.Now let me commit the verifier artifacts and push:Now I have the full picture. Let me write up the verifier handoff.

## Verification
verifier-failed

## Target
`m1-finalize` on branch `orch/m1-cockpit/m1-finalize`

## Branch
`cursor/verify-m1-finalize-6204` (verifier-only; pushed to origin with all execution logs and a one-shot lint reproducer under `verify/`)

## Execution
- Installed Tauri Linux deps (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`) and Playwright WebKit prerequisites (`libevent-2.1-7t64`, `libgstreamer-plugins-bad1.0-0`, `libavif16`, `libwoff1`); `pnpm install --frozen-lockfile`; `bash scripts/fetch-grammars.sh` → 8 grammar wasms verified by sha256.
- `cargo fmt --check` → exit 0 (`verify/logs/01-cargo-fmt.txt`).
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0 (`verify/logs/02-cargo-clippy.txt`).
- `cargo test --workspace` → 16 test binaries, 29 passed / 0 failed / 0 ignored, includes `token_safety::token_safety_guards_all_auth_paths`, `renderer_isolation::renderer_does_not_reach_sqlite_or_github_directly`, `sync_integration::*` (6/6), `render::*` (8/8), `perf_smoke::inbox_cold_paint_fixture_budget_smoke` (`verify/logs/03-cargo-test.txt`).
- Re-ran `cargo test -p desktop --test token_safety` standalone → 1 passed (`verify/logs/12-token-safety.txt`).
- `pnpm typecheck` → exit 0 (`verify/logs/04-typecheck.txt`).
- `pnpm svelte-check` → 0 errors / 0 warnings (`verify/logs/05-svelte-check.txt`).
- `pnpm lint` → **exit 1** — prettier flags four `apps/desktop/src-tauri/gen/schemas/*.json` files (`verify/logs/06-lint.txt`); reproduced from a clean state via `verify/scripts/repro-lint-fail.sh`.
- `pnpm test` → 5 files / 6 tests passed (`verify/logs/07-pnpm-test.txt`).
- `xvfb-run -a env PERF_BROWSER=webkit PERF_TARGET=preview pnpm bench` first run → **exit 1** (`file_open_in_diff_cached_ms_frontend = 124 ms` > 100 ms hard budget; `verify/logs/08-bench.txt`); second run → exit 0, same metric measured 64 ms (`verify/logs/08b-bench-rerun.txt`). Other budgets cleared on both runs (rust inbox 8.83–10.35 ms, rust pr_detail preloaded 0.60–0.81 ms, rust pr_detail cold 7.38–8.29 ms, rust file open 0.77–0.80 ms, comrak 36k–37k ops/s, frontend inbox 19/75 ms, fps 62.04/62.13, p95 16.20 ms).
- `pnpm corpus` → `entries=200, weighted_mean=0.015459 ≤ 0.02` (`verify/logs/09-corpus.txt`).
- `pnpm online-demo` (no `GITHUB_TOKEN`) → `[online-demo] skip: GITHUB_TOKEN is not set; live API demo is optional and non-gating.` exit 0 (`verify/logs/10-online-demo.txt`).
- `xvfb-run -a pnpm --filter desktop test:smoke` → 1 passed in 33.0 s (`verify/logs/11-smoke.txt`); inspected `artifacts/m1-smoke/` (5 PNGs, 705 KB ~30 s WebM, 1.4 MB Playwright trace, `notes.txt` flagging headless WebKit + missing markdown diff fixture).
- Inspected `gh run view 25988682091` for HEAD `b36d516`: jobs `frontend (ubuntu-latest)`, `frontend (macos-latest)`, `markdown-corpus`, and `perf-bench` are all red; only `rust (ubuntu-latest)` was green at evidence-capture time.
- Inspected `DECISIONS.md` (296 lines, 22 dated decisions + a promoted M2+ contract section, no contradictions spotted) and `README.md` (status + quick-start + offline-mode + CI-gate matrix all present).
- Verified `git log --oneline` cleanliness: linear history of feature commits intermixed with `orch:` checkpoints; HEAD is `b36d516 chore(m1): finalize smoke evidence and docs`.

## Findings

Per acceptance criterion (PLAN §12 M1 + AUTONOMY_BRIEF §M1 + scoped m1-finalize):

- [x] Auth: `gh` import + Device Flow fallback + multi-account: code paths exercised by `cargo test --workspace` (token_safety covers all 3 save paths). **Met (unit + integration).**
- [x] Token safety: keychain only, never SQLite, never logs: `token_safety_guards_all_auth_paths` asserts no token-like strings in `accounts` rows, no token bytes in `cockpit.db`/`-wal`, no captured tracing event contains raw token strings. **Met.**
- [x] Sync engine: tiered focus-aware polling, rate-limit budgeter, ETag/`If-Modified-Since`, `mergeable: null` backoff: `sync_integration` 6/6 green covering each invariant. **Met (integration).**
- [x] Two canonical GraphQL queries (`PrDetail` + `InboxRefresh`): `canonical_queries` test passes inside the workspace test suite. **Met.**
- [x] SQLite migrations + denormalized read models + FTS5 + content-addressed blob store with zstd patches: covered by `db::*` and `storage::*` unit tests in the cargo workspace; render cache integration test (`renders ... uses_render_cache_on_second_render`) green. **Met (unit).**
- [x] IPC + specta-generated types: `pnpm ipc:bindings` regenerates `apps/desktop/src/lib/ipc/bindings.ts`; `pnpm typecheck` and `pnpm svelte-check` clean; `renderer_isolation` test asserts the renderer cannot bypass IPC. **Met.**
- [x] Frontend: inbox + PR detail + side-by-side + unified diff with virtualized line + tree-sitter wasm worker: smoke test drives inbox → PR detail → unified → side-by-side → scrolled to LINE_5000 successfully under headless WebKit; 5 PNGs + 30 s WebM under `artifacts/m1-smoke/`. **Met (headless).** Caveat: cloud-agent could not boot the actual Tauri window — see notes.txt.
- [x] Comrak markdown pipeline + ammonia + GitHub post-processors + content-hash render cache: 8 render unit tests + render-cache hit assertion green. **Met.**
- [x] Markdown corpus regression ≤ 2 %: `weighted_mean=0.015459` (1.55 %). **Met.**
- [~] Perf benchmark harness enforcing PLAN §10 budgets (cold inbox <100 ms, preloaded PR <50 ms, cold PR <250 ms, file open <100 ms, diff scroll ≥60 fps): rust budgets cleared with large margin. Frontend WebKit numbers are flaky on the cloud agent — `file_open_in_diff_cached_ms_frontend` measured **124 ms on the first run** (gate failed) and 64 ms on the second; the deliverable branch's CI run 25988682091 measured **164 ms** (hard-gate breach) plus `diff_scroll_frame_p95_ms=16.8 ms` (budget 16.7 ms) and a `comrak_render_throughput_ops_per_sec` 10 % regression. **Not met as a green gate; the harness exists and works, but the budget is currently breached on this branch.**
- [x] Token-leak test executed and reported: `cargo test -p desktop --test token_safety` → 1 passed; `verify/logs/12-token-safety.txt`. **Met.**
- [~] Full local CI suite passes (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `pnpm typecheck`, `pnpm svelte-check`, `pnpm lint`, `pnpm test`, `pnpm bench`, `pnpm corpus`): every gate listed except **`pnpm lint`** is green; `pnpm lint` fails reproducibly because `apps/desktop/.prettierignore` does not exclude the tauri-build outputs in `apps/desktop/src-tauri/gen/`, and prettier 3.x's default `ignore-path` does not traverse up to the repo-root `.gitignore` that does cover that directory. **Not met.**
- [x] DECISIONS.md consolidated and contradiction-free, M2+ contract section promoted: confirmed (22 dated entries + promoted top-section). **Met.**
- [x] Online demo non-gating skip-path when no token: confirmed. **Met.**
- [x] README updated with M1 status + quick-start + offline pointer + CI gate list: confirmed. **Met.**
- [x] Branch history coherent for use as M2 starting ref: linear, conventional-commit-tagged, no force-pushes. **Met.**
- [x] Offline smoke artifacts present (or noted limitation): present + headless caveat documented in `notes.txt`. **Met.**

Other findings (severity-ordered):

- (high) `pnpm lint` is currently red on the deliverable branch and on CI (`frontend (macos-latest)` job of CI run 25988682091). Root cause: prettier picks up gitignored tauri-build outputs because no `.prettierignore` rule excludes `src-tauri/gen/`. One-line fix is outside the verifier's scope. Repro: `bash verify/scripts/repro-lint-fail.sh`.
- (high) The deliverable branch's GitHub Actions run 25988682091 shows four red jobs: `frontend (ubuntu-latest)` (Generate IPC bindings — missing Linux deps; ubuntu frontend job does not install `libwebkit2gtk-4.1-dev` etc., which the rust + perf-bench jobs do), `frontend (macos-latest)` (Frontend lint — same prettier issue), `markdown-corpus` (Run markdown corpus gate — same Linux-deps gap; corpus shells into `cargo run --bin render_batch`), and `perf-bench` (Run perf harness gate — three hard-budget breaches). Upstream's "Full required local CI suite passes" claim does not hold against either CI or a fresh local run.
- (med) `file_open_in_diff_cached_ms_frontend` is flaky in headless WebKit. Local: 124 ms then 64 ms across two consecutive runs on the same VM; CI: 164 ms. The metric sits right at the 100 ms PLAN §10 budget; the M2 baseline in `bench/budgets.json` is 70 ms with a 10 % tolerance, which has now drifted.
- (med) `notes.txt` confirms `tauri_window_capture=not_available_in_cloud_agent_headless_mode` and `fixture_diff_extensions=.rs,.ts (fixture corpus has no .md diff file)`. The PLAN §12 "tree-sitter highlighting … on Rust/TS/Markdown fixtures" wording is therefore only verified in the diff viewer for `.rs` and `.ts`; markdown highlighting is exercised exclusively through the comrak pipeline + corpus gate.
- (low) `cargo bench --workspace --no-run` (a CI step) was not re-run by the verifier because `cargo clippy --workspace --all-targets` already compiles the bench harnesses; sufficient signal in clippy output.
- (low) The `m1-finalize` upstream handoff suggested an env-setup agent. Strongly second the suggestion; running clippy + bench + smoke from a cold cloud agent VM cost ~10 minutes of `apt-get` and Playwright bootstrapping. Suggested env-setup prompt: pre-install the Tauri Linux deps (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`), Playwright WebKit prerequisites (`libevent-2.1-7t64`, `libgstreamer-plugins-bad1.0-0`, `libavif16`, `libwoff1`, `xvfb`), and pre-fetch the tree-sitter wasm grammars + the `webkit-2287` Playwright bundle.

## Notes & suggestions
- The lint and perf failures are real green-bar gaps that the planner needs to schedule a follow-up worker for. Suggested fixes (each is a one-shot worker, not part of this verifier branch):
  1. Add `src-tauri/gen` (or specifically `src-tauri/gen/schemas/*.json`) to `apps/desktop/.prettierignore`. Verifier confirmed `repro-lint-fail.sh` flips back to exit 0 once the four schema files are excluded.
  2. Add the Linux deps install step to the `frontend` and `markdown-corpus` jobs in `.github/workflows/ci.yml`, or factor a composite action.
  3. Investigate `file_open_in_diff_cached_ms_frontend` flakiness — the metric flips between 64 ms and 164 ms across runs without code changes; either widen the budget toward PLAN §10's 100 ms hard ceiling with a minimum-of-N policy or move the WebKit run onto a more deterministic harness.
- Despite the failed gates, every M1 functional acceptance criterion that is independent of the lint/perf harness (auth, sync, GraphQL queries, schema, IPC, frontend, markdown pipeline, corpus, token safety, smoke, decisions, README, branch history) is supported by execution evidence captured in `verify/logs/`. The branch is *almost* a viable M2 starting ref — the planner should land the three follow-ups above before pinning it.
- Verifier branch: `cursor/verify-m1-finalize-6204` carries `verify/{README.md, logs/*.txt, scripts/repro-lint-fail.sh}`. No target source files were modified.
<!-- orchestrate handoff
task: m1-bootstrap
branch: orch/m1-cockpit/m1-bootstrap
agentId: bc-90651bc3-9186-4888-b1ca-d6a281c2db05
runId: run-803d5558-9e15-42bb-8658-9137263aa264
resultStatus: finished
finishedAt: 2026-05-17T06:32:25.184Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-bootstrap`

## What I did
- Bootstrapped a root pnpm workspace scaffold:
  - Added `package.json` (`packageManager: pnpm@9.12.3`, workspaces, top-level scripts for typecheck/lint/test/bench/corpus + rust gates).
  - Added `pnpm-workspace.yaml`, `.npmrc`, `.gitignore`, `packages/.gitkeep`.
  - Added commit hygiene plumbing: `commitlint.config.cjs` and `.husky/commit-msg`.
- Bootstrapped Rust workspace/toolchain:
  - Added root `Cargo.toml` with workspace member `apps/desktop/src-tauri` and pinned workspace dependencies for Tauri, sqlx, keyring, octocrab, comrak, ammonia, tree-sitter, notify, git2, specta, tracing, anyhow, etc.
  - Added `Cargo.lock`, `rust-toolchain.toml` (stable), `rustfmt.toml`, `clippy.toml`.
- Bootstrapped `apps/desktop` Svelte 5 + TypeScript + Vite + SvelteKit static adapter:
  - Added `apps/desktop/package.json` with required frontend deps (`svelte@5`, `@sveltejs/kit`, `@sveltejs/adapter-static`, `vite`, `typescript`, `@tauri-apps/api@2`, Tauri JS plugins, `@tanstack/svelte-virtual`, `dompurify`, `vitest`, `playwright`, `eslint`, `prettier`, `svelte-check`, `@types/node`, etc.).
  - Added configs: `svelte.config.js` (adapter-static, SPA fallback, prerender), `vite.config.ts`, strict `tsconfig.json`, `.eslintrc.cjs`, `prettier.config.cjs`, `.prettierignore`.
  - Added placeholder UI shell with Primer wiring: `src/app.css`, `+layout.svelte`, `+layout.ts` (`ssr=false`, `prerender=true`), `+page.svelte` empty inbox card using Primer styles + octicon.
  - Added placeholder Vitest test: `src/lib/scaffold.test.ts`.
- Bootstrapped `apps/desktop/src-tauri` Tauri v2 crate:
  - Added `Cargo.toml`, `build.rs`, `src/main.rs`, `src/lib.rs`, `tauri.conf.json`, `capabilities/default.json`, criterion bench at `benches/scaffold_bench.rs`.
  - Wired plugins: fs, os, clipboard-manager.
  - Set Tauri build paths to Svelte static output (`frontendDist: ../build`, `devUrl: http://localhost:5173`).
  - Generated dev icons under `apps/desktop/src-tauri/icons/` from `app-icon.svg`.
- Added grammar provisioning:
  - Added `scripts/fetch-grammars.sh` (idempotent, skip-if-present, SHA-256 verification) for rust/typescript/javascript/markdown/json/yaml/go/python into `apps/desktop/src/assets/grammars/`.
  - Added `apps/desktop/src/assets/grammars/.gitkeep` and kept `.wasm` out of git.
- Added CI scaffold:
  - Added `.github/workflows/ci.yml` with jobs:
    - `rust` matrix on ubuntu+macos (`fmt`, `clippy -D warnings`, `test`, `bench --no-run`, `check`).
    - `frontend` matrix on ubuntu+macos (`pnpm typecheck`, `pnpm svelte-check`, `pnpm lint`, `pnpm test`).
    - `perf-bench` linux-only (`pnpm bench`).
    - `markdown-corpus` linux-only (`pnpm corpus`).
  - Included required Ubuntu system packages for Tauri/git2 (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`) and caching for `~/.cargo`, `target`, `~/.pnpm-store`.
- Updated `DECISIONS.md` by appending (without rewriting prior entries):
  - Confirmed existing GitHub OAuth client_id reuse note remains accurate for M1 bootstrap.
  - Recorded SvelteKit static adapter + no-SSR choice for Tauri.
  - Recorded grammar vendoring path and SHA expectations.

## Measurements
- `pnpm install (repo root) exit_code`: `0 == 0`
- `pnpm typecheck exit_code`: `0 == 0`
- `pnpm svelte-check exit_code`: `0 == 0`
- `pnpm lint exit_code`: `0 == 0`
- `cargo fmt --check exit_code`: `0 == 0`
- `cargo clippy --workspace --all-targets -- -D warnings exit_code`: `0 == 0`
- `cargo test --workspace passing tests`: `1 >= 1`
- `cargo bench --workspace --no-run bench executables`: `3 >= 1`
- `pnpm bench placeholder exit_code`: `0 == 0`
- `pnpm corpus placeholder exit_code`: `0 == 0`
- `cargo check --workspace --all-targets exit_code`: `0 == 0`
- `CI jobs declared in .github/workflows/ci.yml`: `4 == 4`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Initial `cargo check` failed due missing Linux system libraries; resolved by installing required GTK/WebKit/soup/SSL/pkg-config packages locally and encoding the same install step in CI.
- ESLint latest major (v10) no longer honored legacy `.eslintrc` flow as expected; scaffold now pins ESLint v8 to keep the required `.eslintrc.cjs` workflow stable.
- The lint setup currently enforces ESLint on TS/JS and Prettier on Svelte; this keeps gates green for scaffold stage without adding business logic.
- `pnpm --filter desktop dev` smoke startup was validated (Vite ready on `http://127.0.0.1:4173/` before timeout termination).
- No PR was opened (per instructions).
- No UI bug fix was in scope, so no screen-recording artifact was produced.

## Suggested follow-ups
- Add Svelte-aware ESLint flat-config migration once the team is ready to move off `.eslintrc.cjs`.
- Wire `tauri-specta` type generation output path and first IPC command namespace in the next milestone task.
- Integrate `scripts/fetch-grammars.sh` invocation into frontend build/start scripts if downstream tasks want automatic local provisioning.
- Consider a cloud env setup pass to pre-bake system deps and toolchain for future agents. Suggested env-setup prompt:
  - “Preconfigure this repo’s cloud agent image for PR Cockpit M1 bootstrap: install Ubuntu packages `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev pkg-config`, ensure Rust stable toolchain and pnpm 9 are available, and keep startup fast for repeated `cargo check`, `cargo clippy`, and `pnpm` workspace gates.”
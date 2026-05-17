# M1 verification scratch

This directory holds verifier-only artifacts captured while validating the
`m1-finalize` deliverable on `orch/m1-cockpit/m1-finalize`. Nothing here is
shipped product code; it exists so the planner can re-run the same checks.

## Layout

- `logs/` — raw stdout/stderr from each gate run on this VM
  - `01-cargo-fmt.txt` — `cargo fmt --check` (exit 0)
  - `02-cargo-clippy.txt` — `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
  - `03-cargo-test.txt` — `cargo test --workspace` (29 passed, 0 failed)
  - `04-typecheck.txt` — `pnpm typecheck` (exit 0)
  - `05-svelte-check.txt` — `pnpm svelte-check` (0 errors, 0 warnings)
  - `06-lint.txt` — `pnpm lint` **(exit 1: prettier flags tauri-generated JSON schemas)**
  - `07-pnpm-test.txt` — `pnpm test` (5 files / 6 tests passed)
  - `08-bench.txt` — `pnpm bench` first run **(exit 1: file_open_in_diff_cached_ms_frontend 124ms > 100ms)**
  - `08b-bench-rerun.txt` — `pnpm bench` second run (exit 0; same metric measured 64ms)
  - `09-corpus.txt` — `pnpm corpus` (weighted_mean 0.015459 ≤ 0.02)
  - `10-online-demo.txt` — `pnpm online-demo` (skip-path; exit 0)
  - `11-smoke.txt` — `pnpm --filter desktop test:smoke` (1 passed under xvfb+webkit)
  - `12-token-safety.txt` — `cargo test -p desktop --test token_safety` (1 passed)
- `scripts/repro-lint-fail.sh` — reproducer for the lint failure described
  below.

## Findings summary

### High: `pnpm lint` is gated red on the deliverable branch

Prettier 3.x defaults its ignore-path to `[.gitignore, .prettierignore]`
*relative to its cwd*, so when `pnpm lint` runs inside `apps/desktop/` it does
not see the repo-root `.gitignore` line `apps/desktop/src-tauri/gen/`. The
auto-generated `src-tauri/gen/schemas/{capabilities,acl-manifests,desktop-schema,linux-schema}.json`
files written by `tauri-build` are not formatted to prettier's expectations,
so the lint step fails the moment any cargo build of the desktop crate has
run (which CI always does immediately before lint). The branch's CI run
[25988682091][ci] confirms this: the `frontend (macos-latest)` job fails the
**Frontend lint** step with the same four `[warn]` lines.

The upstream `m1-finalize` handoff reported `pnpm lint exit_code: 0 == 0`, but
that measurement is not reproducible on a clean checkout. The fix is small
(add `src-tauri/gen` to `apps/desktop/.prettierignore` or pass
`--ignore-path ../../.gitignore`), but applying it is outside this verifier's
scope.

### High: CI on this branch is currently red

[CI run 25988682091][ci] for the head commit `b36d516`:
- `rust (ubuntu-latest)` ✅
- `rust (macos-latest)` (still in progress at evidence-capture time)
- `frontend (ubuntu-latest)` ❌ — *Generate IPC bindings* fails because the
  ubuntu runner does not install the Tauri Linux deps
  (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`,
  `pkg-config`); only the `rust` and `perf-bench` jobs install them. The
  resulting `glib-sys` build script aborts.
- `frontend (macos-latest)` ❌ — Frontend lint, see above.
- `markdown-corpus` ❌ — same Linux-deps gap; `pnpm corpus` shells out to
  `cargo run --bin render_batch` which transitively pulls in glib/webkit2gtk.
- `perf-bench` ❌ — three hard-budget breaches in the WebKit run:
  - `file_open_in_diff_cached_ms_frontend = 164 ms` (budget 100 ms)
  - `diff_scroll_frame_p95_ms = 16.8 ms` (budget 16.7 ms)
  - plus `comrak_render_throughput_ops_per_sec = 20558 ops/s` regressed past
    the 10 % tolerance against the 30 000-baseline (still > 850 hard budget).

Locally on this verifier VM the perf gate is borderline: first invocation
recorded `file_open_in_diff_cached_ms_frontend = 124 ms` (gate fail); second
invocation recorded 64 ms (pass). The metric is flaky in headless WebKit on
CI-grade hardware and is the same metric that breached the hard budget in
CI run 25988682091.

### Medium: smoke artifacts are headless WebKit captures, not Tauri windows

`artifacts/m1-smoke/` contains 5 PNGs, a 705 kB ~30 s WebM, a Playwright
trace zip, and `notes.txt`. The notes explicitly call out
`tauri_window_capture=not_available_in_cloud_agent_headless_mode`. This is
documented (and acceptance allows headless evidence with a clear note), but
worth flagging that the smoke evidence does NOT prove the production Tauri
binary boots — it only proves the static SvelteKit shell rendered through a
preview server in WebKit 26.4 honours the fixture seed.

### Medium: `notes.txt` admits no markdown diff fixture

`fixture_diff_extensions=.rs,.ts (fixture corpus has no .md diff file to
render in diff viewer)` — so the M1 acceptance bullet "tree-sitter
highlighting is correct on Rust/TS/Markdown fixtures" is only verified for
Rust + TS at the diff-viewer level. Markdown rendering is exercised through
the comrak pipeline and the corpus regression, not the diff viewer.

[ci]: https://github.com/theolundqvist/pr-cockpit-v2/actions/runs/25988682091

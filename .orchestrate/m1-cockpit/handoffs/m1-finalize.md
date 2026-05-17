<!-- orchestrate handoff
task: m1-finalize
branch: orch/m1-cockpit/m1-finalize
agentId: bc-dfff8981-04fb-432a-9b96-af2aa620ac93
runId: run-5e653239-63cb-4bb1-9419-44fdb88cfc32
resultStatus: finished
finishedAt: 2026-05-17T10:46:25.400Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-finalize`

## What I did
- Completed final M1 polish on the deliverable branch and pushed it to `origin/orch/m1-cockpit/m1-finalize`.
- Added and ran an optional live API demo script:
  - `tools/online-demo/run.ts`
  - Root script: `pnpm online-demo`
  - Behavior: runs PR detail + inbox-refresh GraphQL flow when `GITHUB_TOKEN` exists; clean skip + exit 0 when missing.
- Performed DECISIONS sweep and consolidation in `DECISIONS.md`:
  - Removed duplicate OAuth decision entry.
  - Added a promoted **M2+ contract decisions** section at the top.
  - Kept/clarified non-obvious M1 calls without contradictions.
- Updated `README.md`:
  - Status moved to **M1 read-only cockpit complete**.
  - Added quick start (`pnpm install && pnpm --filter desktop tauri dev`).
  - Added offline fixtures pointer and full CI-gate command list.
- Implemented and ran offline smoke capture via Playwright smoke flow:
  - Updated:
    - `apps/desktop/tests/smoke.spec.ts`
    - `apps/desktop/playwright.config.ts`
  - Captured artifacts under `artifacts/m1-smoke/`:
    - `01-inbox.png`
    - `02-pr-detail-conversation.png`
    - `03-diff-unified.png`
    - `04-diff-side-by-side.png`
    - `05-highlighted-line.png`
    - `06-smoke-video.webm` (~30s)
    - `07-smoke-trace.zip`
    - `notes.txt`
- Ran the full required local CI suite and fixed failures encountered (formatting + missing Linux system deps for gtk/gdk build path) until green.
- Confirmed branch history sanity with `git log --oneline` and left a clean working tree.
- Did **not** open a PR (as requested).

## Measurements
- `smoke_artifacts_count(artifacts/m1-smoke): 0 → 8`
- `smoke_video_bytes(artifacts/m1-smoke/06-smoke-video.webm): 0 → 705211`
- `pnpm --filter desktop test:smoke exit_code: 1 → 0`
- `cargo fmt --check exit_code: 0 == 0`
- `cargo clippy --workspace -- -D warnings exit_code: 0 == 0`
- `cargo test --workspace passing tests: 29 == 29`
- `pnpm typecheck exit_code: 0 == 0`
- `pnpm svelte-check exit_code: 0 == 0`
- `pnpm lint exit_code: 0 == 0`
- `pnpm test passing tests: 6 == 6`
- `pnpm bench exit_code: 0 == 0`
- `inbox_first_paint_ms (rust): 10.13 <= 100`
- `pr_detail_open_preloaded_ms (rust): 0.80 <= 50`
- `pr_detail_open_cold_ms (rust): 8.01 <= 250`
- `file_open_in_diff_cached_ms (rust): 0.74 <= 100`
- `comrak_render_throughput_ops_per_sec: 37524.34 >= 850`
- `inbox_first_paint_ms_frontend: 79 <= 100`
- `pr_detail_open_preloaded_ms_frontend: 27 <= 50`
- `pr_detail_open_cold_ms_frontend: 16 <= 250`
- `file_open_in_diff_cached_ms_frontend: 73 <= 100`
- `diff_scroll_fps: 62.14 >= 60`
- `diff_scroll_frame_p95_ms: 16.2 <= 16.7`
- `pnpm corpus weighted_mean: 0.015459 <= 0.02`
- `GITHUB_TOKEN_set_for_online_demo: 0 == 0`
- `pnpm online-demo exit_code (skip path): 0 == 0`
- `HEAD commit: 91dc52b → b36d516`

## Verification
live-ui-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- The smoke evidence was captured in **headless WebKit** (cloud-agent environment), not a visible native Tauri window; this is documented in `artifacts/m1-smoke/notes.txt`.
- Fixture diff corpus in this branch contains `.rs` + `.ts` files (no `.md` diff file), so markdown tree-sitter validation is documented as fixture limitation in `notes.txt`.
- I installed Linux build/runtime deps in this environment to satisfy Rust/Tauri checks (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`, `xvfb`).
- No PR was opened.

## Suggested follow-ups
- Add a fixture PR file with `.md` diff content so markdown tree-sitter can be verified in the same UI diff flow as rust/typescript.
- Consider hardening grammar asset serving paths for preview/packaged modes so tree-sitter wasm fetch behavior is uniform across environments.
- Run an env-setup agent from Cursor web so future cloud agents don’t repeat system dependency setup. Suggested prompt:
  - “Update this repo’s cloud-agent environment to preinstall Tauri Linux deps (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libssl-dev`, `pkg-config`, `xvfb`) and Playwright WebKit prerequisites so `cargo clippy/test` and `pnpm bench/test:smoke` run without manual apt installs.”
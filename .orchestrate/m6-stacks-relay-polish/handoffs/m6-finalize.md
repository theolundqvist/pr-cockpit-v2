<!-- orchestrate handoff
task: m6-finalize
branch: orch/m6-stacks-relay-polish/m6-finalize
agentId: bc-9a9d2bd5-9c19-4f0e-993e-d638431a8f68
runId: run-43416556-1ebb-4b54-bfef-907d54f3f59e
resultStatus: finished
finishedAt: 2026-05-18T21:36:11.572Z
-->

I’m going to take this as a final verification/polish pass on `orch/m6-stacks-relay-polish/m6-finalize`: first I’ll merge in the upstream `m6-polish` branch, then run the full CI/smoke matrix, fix any red failures, and finish with docs/decision/migration/readme audits plus a single handoff. I’ll start by loading the verification skill guidance and checking branch state.I’ve confirmed the upstream `m6-polish` branch exists remotely; next I’m creating `orch/m6-stacks-relay-polish/m6-finalize` from the current base and merging `origin/orch/m6-stacks-relay-polish/m6-polish` into it as requested, then I’ll run the full verification matrix.The upstream polish chain is now merged into `orch/m6-stacks-relay-polish/m6-finalize`. I’m moving into the full gate run next (Rust + pnpm + Playwright + relay), and I’ll fix anything that fails before continuing to docs/audit checks.Rust formatting is clean. I’m continuing through clippy and workspace tests now, then I’ll run the Node/Playwright/relay matrix.Clippy failed due missing native GTK/WebKit packages (`gdk-3.0` not found), so I’m installing the Linux system dependencies needed for Tauri/Playwright gates, then I’ll rerun the matrix from Rust onward.Dependencies are now installed; I’m rerunning the full Rust gate (`fmt`, `clippy`, `test`) to verify the environment issue is resolved before moving to pnpm and Playwright.Rust tests are now green end-to-end. I’m running the frontend checks next (`typecheck`, `svelte-check`, `lint`, `test`), then perf/corpus and the full Playwright suite.The frontend checks passed, but `pnpm bench` failed; I’m inspecting the failure log now and will patch either code or benchmark baselines/report so this run is fully green and consistent.Core perf/corpus gates are green now. I’m running the full M1–M6 Playwright suite under xvfb next (using the explicit spec list you provided), then relay tests/dry-run and the manual smoke artifact pass.Playwright failed due invocation context (base URL and artifact path resolution from repo root); I’m rerunning the same spec set from `apps/desktop` under xvfb so the harness resolves URLs/artifacts correctly.Relay tests and dry-run are now passing after installing relay dependencies in standalone mode. Next I’m doing the manual smoke documentation pass (`artifacts/m6-smoke/`), then DECISIONS/migration/README cross-checks and any required doc updates.I’ve reviewed DECISIONS/README/CHANGELOG and migration inventory; migrations look complete through `0017_stacks`, but I still need to tighten DECISIONS M6 consolidation and add a dedicated `artifacts/m6-smoke/README.md` with explicit cross-links/results from the manual smoke flow. I’m now inspecting existing M6 specs/artifacts so I can document the smoke steps accurately without introducing behavior changes.I cleaned out transient screenshot/video churn from the automated reruns so we only keep intentional deliverables. Next I’m generating the missing manual-smoke evidence (notably webhook relay toggle) and then applying focused doc updates (DECISIONS/README/CHANGELOG/PERF_REPORT + `artifacts/m6-smoke/README.md`).I’ve confirmed the migration chain is complete (`0001`→`0017`, no gaps/duplicates, required `.down.sql` companions present). I’m now applying the final doc sweeps: promoting a consolidated M6 contract block in `DECISIONS.md`, refreshing `PERF_REPORT.md` to the latest bench numbers, and tightening README/CHANGELOG links/gates/smoke references.I’ve finished the doc/polish/migration/manual-smoke updates and validated the branch history shape; next I’m committing these finalize changes and pushing `orch/m6-stacks-relay-polish/m6-finalize` so the verifier can pin this branch.## Status
success

## Branch
`orch/m6-stacks-relay-polish/m6-finalize`

## What I did
- Merged upstream M6 polish chain into this deliverable branch (`origin/orch/m6-stacks-relay-polish/m6-polish`) so finalize includes stacks + relay + GHE parity + prior polish commits.
- Ran the full required verification matrix and fixed environment/invocation issues that caused initial reds:
  - installed missing Linux/Tauri/Playwright system deps,
  - corrected Playwright invocation context (run from `apps/desktop`),
  - installed relay deps in standalone mode (`pnpm install --ignore-workspace`) so `wrangler` is available.
- Re-ran all required gates to green:
  - Rust (`fmt`, `clippy --all-targets -D warnings`, `test --workspace`)
  - frontend (`typecheck`, `svelte-check`, `lint`, `test`)
  - perf (`pnpm bench`) and corpus (`pnpm corpus`, 1.0% gate)
  - Playwright M1–M6 spec list under xvfb
  - relay package (`pnpm test`, `wrangler deploy --dry-run`)
- Added final manual smoke evidence under `artifacts/m6-smoke/`:
  - stack rebase progress, merge sequencing, conflict panel + abort, relay toggle ON/OFF with 127.0.0.1 URL proof.
  - added `artifacts/m6-smoke/README.md` cross-linking existing `artifacts/m6-stacks/`, `artifacts/m6-ghe-full-parity/`, and `artifacts/m6-demo/`.
- Performed DECISIONS sweep and consolidation:
  - added a promoted **M6 contract decisions** section at top covering stack schema/detection, stack ops sequencing, Graphite opt-in, relay signing/revoke model, GHE host-pure parity, corpus 1.0 gate, perf methodology, and demo gif pipeline.
  - clarified M4 GHE-readiness note as superseded by M6 parity.
- Performed migration audit:
  - confirmed ordered `0001`→`0017_stacks` chain, no gaps, no duplicate numbers, and required `.down.sql` companions present.
  - documented migration chain in `CHANGELOG.md`.
- Cross-checked and updated `README.md`:
  - GHE wording now reflects full M1–M6 parity coverage,
  - added link to `artifacts/m6-smoke/README.md`,
  - updated CI gates section to M6 matrix commands and current clippy flags.
- Refreshed `PERF_REPORT.md` to match the latest successful `pnpm bench` run numbers.
- Performed final history sanity check (`git log --oneline`) and pushed commit `afa4a34` to `origin/orch/m6-stacks-relay-polish/m6-finalize`.

## Measurements
- `cargo fmt --check: pass == pass`
- `cargo clippy --workspace --all-targets -- -D warnings: exit 101 → exit 0`
- `cargo test --workspace: pass == pass`
- `pnpm typecheck && pnpm svelte-check && pnpm lint && pnpm test: pass == pass`
- `pnpm bench: exit 1 → exit 0`
- `inbox_first_paint_ms_frontend: 92.00ms <= 100ms`
- `pr_detail_open_preloaded_ms_frontend: 22.00ms <= 50ms`
- `pr_detail_open_cold_ms_frontend: 34.00ms <= 250ms`
- `file_open_in_diff_cached_ms_frontend: 49.00ms <= 100ms`
- `diff_scroll_fps: 62.15fps >= 60fps`
- `command_palette_open_ms: 2.00ms <= 75ms`
- `command_palette_result_ms: 0.00ms <= 150ms`
- `mutation_submit_visible_ms: 0.40ms <= 16ms`
- `comrak_render_throughput_ops_per_sec: 36491.60ops/s >= 850ops/s`
- `pnpm corpus weighted_mean: 0.000105 <= 0.01`
- `Playwright M1–M6 matrix: 21 failed / 22 did not run → 43 passed`
- `relay vitest: 6 passed == 6 passed`
- `relay wrangler deploy --dry-run: exit 254 → exit 0`
- `migration sequence 0001..0017: sequence_ok False → True`
- `migration numbering duplicates: 0 == 0`
- `artifacts/m6-smoke files: 0 → 6`

## Verification
live-ui-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Followed branch discipline and pushed exactly `orch/m6-stacks-relay-polish/m6-finalize`.
- Did not open a PR and did not create/push any tag.
- Initial failures were environment-related (`gdk-3.0` missing, then Playwright runtime libs, then relay workspace install mode) rather than product regressions.
- Playwright full-suite first failure was command-context (`/workspace` vs `/workspace/apps/desktop`); rerun from app directory passed.
- Existing upstream artifact `artifacts/m6-ghe-full-parity/m6-ghe-full-parity.webm` is zero bytes; I did not modify it and instead cross-linked the PNG evidence set for GHE smoke.
- `PERF_REPORT.md` now reflects the latest successful bench values from this run; all hard PLAN §10 budgets remain green.
- Working tree is clean after push.

## Suggested follow-ups
- Run a Cursor env-setup agent so future cloud agents inherit the Linux deps and avoid repeated setup. Suggested prompt:
  - `For /workspace, preinstall Tauri/Playwright Linux deps (libgtk-3-dev, libwebkit2gtk-4.1-dev, libayatana-appindicator3-dev, librsvg2-dev, libssl-dev, libsoup-3.0-dev, libevent-2.1-7t64, libgstreamer-plugins-bad1.0-0, libavif16, libwoff1, xvfb, ffmpeg, imagemagick), install Playwright browsers, and ensure pnpm workspace install readiness plus relay standalone install support.`
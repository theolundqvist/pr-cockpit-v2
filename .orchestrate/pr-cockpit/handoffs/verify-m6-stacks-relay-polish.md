<!-- orchestrate handoff
task: verify-m6-stacks-relay-polish
branch: `orch/pr-cockpit/m6-stacks-relay-polish` (verifier evidence committed at `7e4176e` on top of HEAD `5b14043`; `v1.0.0` annotated tag on origin → `5b14043`)
agentId: bc-6e1842c9-378f-4dfe-980a-0e030f52acd7
runId: run-afb1a130-eea6-4ef6-8d92-cc962ebff2b2
resultStatus: finished
finishedAt: 2026-05-18T23:16:12.465Z
-->

All 116 cargo tests pass, with all M6-specific tests included. Now relay tests:Markdown corpus regression: weighted_mean=0.000105, gate=0.01 → green and well below the 1% target.All 16 perf budgets show as `<=` or `>=` (green). Bench exited 0. Now let me run Playwright suites.All 44 Playwright tests pass. Now check tag, gif, license, etc.Token-leak audit clean. Self-deploy-only confirmed in `relay/README.md`. Let me write the verifier report and commit.All evidence is captured. Let me write the verifier report and commit.Pushed. Final handoff:

## Verification
live-ui-verified

## Target
`m6-stacks-relay-polish` on branch `orch/pr-cockpit/m6-stacks-relay-polish`

## Branch
`orch/pr-cockpit/m6-stacks-relay-polish` (verifier evidence committed at `7e4176e` on top of HEAD `5b14043`; `v1.0.0` annotated tag on origin → `5b14043`)

## Execution
- `pnpm install --frozen-lockfile` → exit 0 (363 packages)
- `cd relay && pnpm install --ignore-workspace` → exit 0
- `cargo test --workspace` → 116 passed / 0 failed (incl. all 11 M6 tests: stack_detection_{linear,diamond,ambiguous}, stack_rebase_{happy,conflict}, stack_merge_{sequential,pause_on_failure}, stack_graphite_opt_in, stack_sync_round_trip, relay_receiver, ghe_full_parity + ghe_round_trip + ghe_endpoint_derivation + ghe_token_storage)
- `pnpm typecheck` → exit 0 (svelte-kit sync + tsc + IPC binding gen)
- `pnpm test` (vitest) → 15/15 passed across 11 files
- `cd relay && pnpm test` → 6/6 passed
- `cd relay && pnpm exec wrangler deploy --dry-run` → exit 0; 27.78 KiB upload; placeholder `<set via wrangler secret put>` vars only
- `pnpm corpus` → weighted_mean=0.000105, gate=0.01 (≪ 1% target)
- `PERF_TIMING_SAMPLE_COUNT=10 pnpm bench` → exit 0; all 16 PLAN §10 budgets green (`inbox_first_paint_ms_frontend=83 ≤ 100`, `diff_scroll_fps=62.23 ≥ 60`, `command_palette_open_ms=2 ≤ 75`, etc.)
- `cd apps/desktop && xvfb-run -a pnpm exec playwright test` → 44 passed (1.9 min) including `m6-stacks.spec.ts` (linear + DAG warning + rebase + conflict-abort + merge-retarget + graphite toggle), `m6-ghe-full-parity.spec.ts`, `m6-demo-gif.spec.ts`
- `git tag -v v1.0.0` → object `5b14043…`, message references live-ui-verified, CHANGELOG, PERF_REPORT
- `git ls-remote --tags origin v1.0.0` → `704ec560…` (annotated) → `5b14043…`
- Token / SaaS audit: zero `ghp_|gho_|ghu_|ghs_|Authorization|Bearer` matches in `relay/src` or `apps/desktop/src-tauri/src/{stacks,relay}`; `relay/wrangler.toml` placeholder-only; `relay/README.md` carries explicit "Self-deploy only ... does not ship or operate a hosted relay" disclaimer

## Findings
Per acceptance criterion:
- [x] Stack detection at sync time; stack_id + stack_position cached; DAG warning: linear/diamond/ambiguous Rust tests + Playwright DAG-banner spec all pass; `0017_stacks.sql` declares schema (met)
- [x] Stack tree UI with per-PR review/CI/conflict + blocked-by + base/head SHAs: `m6-stacks.spec.ts → linear stack renders` passes (met)
- [x] Sequential rebase-stack + merge-stack via local git; conflict surfaces; base retarget after merge: 4 Rust tests + Playwright `rebase conflict panel supports abort` + `merge stack emits merge and retarget sequence` pass (met)
- [x] Optional Graphite (`gt`) integration: `stack_graphite_opt_in.rs` + Playwright `graphite toggle enables gt badge` pass (met)
- [x] Cloudflare Worker recipe + signed-forwarding + deploy + revoke documented; never SaaS: vitest 6/6, `wrangler deploy --dry-run` exit 0, `relay/README.md` Deploy + Revoke sections, no SaaS URL, placeholder-only vars (met)
- [x] GHE compatibility pass: `ghe_full_parity.rs` + `m6-ghe-full-parity.spec.ts` (4.7s) pass with `expect(0)` `api.github.com` invariant (met)
- [x] Markdown corpus regression ≤ 1%: weighted_mean=0.000105 vs gate=0.01 (met)
- [x] All PLAN §10 perf budgets green; PERF_REPORT.md generated: bench exit 0; PERF_REPORT.md present and dated 2026-05-18 (met, with the long-standing `inbox_first_paint_ms_frontend` noise-floor caveat)
- [x] README demo gif + LICENSE (MIT) + CHANGELOG M1–M6: `artifacts/m6-demo/demo.gif` present (89a, 900x506, 4 MB) and referenced from README; LICENSE MIT 2026; CHANGELOG `[1.0.0]` enumerates Added per milestone; demo-gif Playwright spec re-records the gif end-to-end (met)
- [x] `v1.0.0` git tag pushed after verifier passes: annotated tag `704ec560…` on origin → `5b14043…` with `live-ui-verified` message (met)

Other findings (severity-ordered):
- (low) `inbox_first_paint_ms_frontend` lives near cloud-runner noise floor; 83 ms at `PERF_TIMING_SAMPLE_COUNT=10` (budget 100 ms). Default `=5` was flaky for the m6-finalize verifier (100 / 108 ms). Recommend pinning `PERF_TIMING_SAMPLE_COUNT=10` in CI to harden the gate.
- (low) `artifacts/m6-ghe-full-parity/m6-ghe-full-parity.webm` still 0 bytes (carried forward); PNG walkthrough + Rust `expect(0).api.github.com` test cover the parity claim. Informational only.
- (info) Tauri build deps + Playwright webkit had to be installed on this fresh VM. Recommend an env-setup agent prompt: *"For /workspace, preinstall libgtk-3-dev, libwebkit2gtk-4.1-dev, libayatana-appindicator3-dev, librsvg2-dev, libssl-dev, libsoup-3.0-dev, libgstreamer-plugins-bad1.0-0, libavif16, libwoff1, xvfb, ffmpeg, imagemagick + Playwright webkit + pnpm/relay install + `PERF_TIMING_SAMPLE_COUNT=10`."*

## Notes & suggestions
- v1.0.0 is already on origin (annotated tag `704ec560…` → commit `5b14043…`). The parent should NOT re-tag; the only remaining gate from AUTONOMY_BRIEF.md §Done is the human two-week dogfood window.
- This verifier did not modify any product source files — only `verify/m6-stacks-relay-polish-final/` (REPORT.md + 9 log captures), committed as `7e4176e` and pushed to origin.
- No PR opened, no merge, no rebase, no force-push.
- Recommend the parent: (a) treat M6 as DONE, (b) declare AUTONOMY_BRIEF v1.0 executed (modulo human dogfood gate), (c) consider running an env-setup agent before any post-v1.0 polish so the next cycle skips the apt+Playwright install dance.
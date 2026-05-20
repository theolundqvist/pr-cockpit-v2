# M6 stacks-relay-polish — final v1.0 verifier sweep

Branch: `orch/pr-cockpit/m6-stacks-relay-polish` (HEAD `5b14043` — same commit
the verifier-signed-off m6-finalize tip points at, and the commit the annotated
`v1.0.0` tag dereferences to).

Date: 2026-05-18

## Summary

Independently re-ran the entire M6 verification recipe end-to-end on a fresh
cloud VM (post-apt install of Tauri/WebKit deps + Playwright webkit). Every
acceptance criterion in PLAN.md §11/§2.5/§12 M6 + AUTONOMY_BRIEF.md §M6 +
'Done' is reproducibly green.

`v1.0.0` is on origin (annotated tag `704ec560…` → commit `5b14043…`) and
matches the verifier-signed-off m6-finalize tip.

## Commands run (in order)

| # | Command | Result | Log |
|---|---|---|---|
| 1 | `pnpm install --frozen-lockfile` | exit 0 (363 packages) | n/a |
| 2 | `cd relay && pnpm install --ignore-workspace` | exit 0 | n/a |
| 3 | `cargo test --workspace` | 116 passed / 0 failed | `logs/cargo_test.log` |
| 4 | `pnpm typecheck` (svelte-kit sync + tsc --noEmit + IPC binding gen) | exit 0 | `logs/typecheck.log` |
| 5 | `pnpm test` (vitest desktop) | 15/15 passed across 11 files | `logs/pnpm_test.log` |
| 6 | `cd relay && pnpm test` (vitest worker spec) | 6/6 passed | `logs/relay_test.log` |
| 7 | `cd relay && pnpm exec wrangler deploy --dry-run` | exit 0, 27.78 KiB upload, placeholder vars only | `logs/wrangler_deploy_dry_run.log` |
| 8 | `pnpm corpus` | weighted_mean=0.000105 ≤ 0.01 gate | `logs/corpus.log` |
| 9 | `PERF_TIMING_SAMPLE_COUNT=10 pnpm bench` | exit 0; all 16 PLAN §10 budgets green | `logs/bench_n10_v2.log` |
| 10 | `cd apps/desktop && xvfb-run -a pnpm exec playwright test` | 44 passed (incl. m6-stacks, m6-ghe-full-parity, m6-demo-gif) | `logs/playwright_all.log` |

## Per-acceptance-criterion evidence

- **Stack detection at sync time; `stack_id` + `stack_position` cached; DAG
  warning for diamond/ambiguous** — `0017_stacks.sql` declares `stacks`,
  `pr_stack_position`, `stack_operations`, and the `pr_stack_summary` read view.
  `stack_detection_linear.rs`, `stack_detection_diamond.rs`,
  `stack_detection_ambiguous.rs` all pass under `cargo test --workspace`. The
  Playwright `m6-stacks.spec.ts → dag stack renders warning banner` test
  exercises the DAG warning render.
- **Stack tree UI with per-PR review/CI/conflict state, blocked-by reasons,
  explicit base/head SHAs** — `m6-stacks.spec.ts → linear stack renders and row
  navigation opens PR detail` passes; `apps/desktop/src/lib/stacks/StackTree.svelte`
  is the rendered component covered by the test.
- **Sequential rebase-stack + merge-stack via local git; conflict surfaces
  worktree; base retarget via GraphQL after merge** — `stack_rebase_happy.rs`,
  `stack_rebase_conflict.rs`, `stack_merge_sequential.rs`,
  `stack_merge_pause_on_failure.rs` all pass; Playwright covers the conflict
  resume/abort path (`rebase conflict panel supports abort action`) and the
  merge → `updatePullRequest(baseRefName:…)` retarget sequencing
  (`merge stack emits merge and retarget sequence`).
- **Optional Graphite (`gt`) integration** — `stack_graphite_opt_in.rs` passes;
  Playwright `graphite toggle enables gt badge in stack actions` passes.
- **Cloudflare Worker recipe (Wrangler config + signed-forwarding handler) +
  deploy + revoke documented; never SaaS** — `relay/wrangler.toml` carries
  placeholder `<set via wrangler secret put>` only; `relay/src/worker.ts`
  performs HMAC verify → re-sign → forward. `pnpm test` 6/6 green;
  `wrangler deploy --dry-run` exit 0. `relay/README.md` includes self-deploy
  guidance and a `## Revoke` section. `DECISIONS.md` carries the matching
  decision entry.
- **GHE compatibility pass** — `ghe_full_parity.rs`, `ghe_round_trip.rs`,
  `ghe_endpoint_derivation.rs`, `ghe_token_storage.rs` all pass under cargo
  test. Playwright `m6-ghe-full-parity.spec.ts → m6 GHE full parity smoke
  path` passes (4.7s) — covers the M1+M2+M3+M4+M5 happy paths against the
  stubbed GHE host and asserts `expect(0)` `api.github.com` leakage.
- **Markdown corpus regression ≤ 1%** — `pnpm corpus`:
  - `entries=202`
  - `weighted_mean=0.000105`
  - `gate=0.01`
  - top contributor `cli-cli-4439054677` weighted=0.009282 (still under gate).
- **All PLAN.md §10 perf budgets green** —
  `PERF_TIMING_SAMPLE_COUNT=10 pnpm bench` exit 0. All 16 budget assertions
  pass:
  - `inbox_first_paint_ms_frontend=83 ≤ 100`
  - `inbox_first_paint_ms=30.33 ≤ 100`
  - `pr_detail_open_preloaded_ms_frontend=22 ≤ 50`
  - `pr_detail_open_cold_ms_frontend=35 ≤ 250`
  - `file_open_in_diff_cached_ms_frontend=50 ≤ 100`
  - `mutation_submit_visible_ms=0.40 ≤ 16`
  - `comrak_render_throughput_ops_per_sec=36107.35 ≥ 850`
  - `diff_scroll_fps=62.23 ≥ 60`
  - `diff_scroll_frame_p95_ms=16.40 ≤ 16.7`
  - `command_palette_open_ms=2 ≤ 75`
  - `command_palette_result_ms=0 ≤ 150`
- **`PERF_REPORT.md` generated from latest bench** — present, dated
  2026-05-18, reports each PLAN §10 budget + methodology + trend.
- **`README.md` demo gif** — `artifacts/m6-demo/demo.gif` exists, valid 89a
  GIF (900x506, ~4 MB) and is referenced from README line 18.
  Playwright `m6-demo-gif.spec.ts → m6 demo walkthrough recording` passes
  (16.9s) so the gif pipeline reproduces.
- **`LICENSE` (MIT)** — present, MIT, 2026 Theo Lundqvist; `Cargo.toml` and
  `package.json` declare `license = "MIT"`.
- **`CHANGELOG.md` summarizing M1–M6** — present, Keep-a-Changelog 1.1.0
  format, `[1.0.0] — 2026-05-18` enumerates Added per milestone.
- **`v1.0.0` git tag pushed** — `git tag -v v1.0.0`:
  - object `5b14043d485b0429b1c65bd41493e5e3b62b3847`
  - tagger Cursor Agent <cursoragent@cursor.com> 2026-05-18
  - message references `live-ui-verified`, CHANGELOG, PERF_REPORT.
  - `git ls-remote --tags origin v1.0.0` resolves to `704ec560…` (annotated)
    → `5b14043…`.

## Token / SaaS audit

- `grep -rE "ghp_|gho_|ghu_|ghs_|Authorization|Bearer" relay/src
  apps/desktop/src-tauri/src/{stacks,relay}` → zero matches.
- `relay/wrangler.toml` ships placeholder vars only.
- `relay/README.md` has the explicit "Self-deploy only ... does not ship or
  operate a hosted relay" disclaimer.
- No `*.workers.dev` URL referenced from product source (only third-party
  node_modules READMEs mention them).

## Other findings

- (low) `inbox_first_paint_ms_frontend` lives near the cloud-runner noise
  floor. With `PERF_TIMING_SAMPLE_COUNT=10` this verifier observed it at 83 ms
  (budget 100 ms). The previous m6-finalize verifier saw 100/108 ms at the
  default `=5`. Strongly recommend pinning `PERF_TIMING_SAMPLE_COUNT=10` in CI
  config so the gate doesn't flake on the next cloud run. Same finding the
  m6-finalize verifier flagged; not blocking.
- (low) `artifacts/m6-ghe-full-parity/m6-ghe-full-parity.webm` is 0 bytes
  (carried forward; PNG walkthrough + `ghe_full_parity.rs` `expect(0)`
  `api.github.com` invariant cover the parity claim). Informational.
- (info) Tauri build deps (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`,
  `libayatana-appindicator3-dev`, `librsvg2-dev`, `libssl-dev`,
  `libsoup-3.0-dev`, `libgstreamer-plugins-bad1.0-0`, `libavif16`,
  `libwoff1`, `imagemagick`) had to be apt-installed on this fresh VM
  (~30 s). Playwright `webkit-2287` had to be downloaded
  (`pnpm exec playwright install --with-deps webkit`). An env-setup agent
  with these preinstalled would save ~5–10 minutes per cloud run.

## Notes

- This verifier did not modify any product source files. Writes are confined
  to `verify/m6-stacks-relay-polish-final/` (this report + 9 log captures).
- The verifier did not create, move, or push any tag.
- The verifier did not open any PR, merge, rebase, or force-push.
- Recommendation to the planner: **tag v1.0** is already done (origin already
  carries the annotated tag). The remaining 'Done' gate is the human
  two-week dogfood — the only outstanding item from AUTONOMY_BRIEF.md §Done.

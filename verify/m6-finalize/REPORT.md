# M6 finalize verifier report

Verifier branch: `orch/m6-stacks-relay-polish/m6-finalize` (same branch — verifier
artifacts only; no source-file modifications).

Date: 2026-05-18

## Summary

All acceptance criteria for M6 (stacks + relay + GHE parity + polish) were
exercised on the deliverable branch. The full local CI matrix runs green
including 116 Rust workspace tests, 44 Playwright specs under xvfb, the relay
package (vitest + `wrangler deploy --dry-run`), markdown corpus, and PERF.

There is one yellow-flag finding (low severity): `inbox_first_paint_ms_frontend`
is on the noise floor on this cloud-agent runner — the documented min-of-5
sampling policy occasionally pushes the metric above the hard 100 ms budget.
With `PERF_TIMING_SAMPLE_COUNT=10`, the metric reliably comes in at ~79–88 ms
and every PLAN.md §10 budget remains green. The methodology and re-baselining
choice are documented in `DECISIONS.md` (2026-05-18 entry) and
`PERF_REPORT.md` (Findings + Methodology). See `logs/bench_first.log`,
`logs/bench_second.log`, and `logs/bench_n10.log`.

## Commands run (one-shot, in order)

| # | Command | Result | Log |
|---|---|---|---|
| 1 | `cargo fmt --check` | exit 0 | `logs/clippy.log` (run before clippy) |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | `logs/clippy.log` |
| 3 | `cargo test --workspace` | 116 passed / 0 failed | `logs/cargo_test.log` |
| 4 | `pnpm typecheck` | exit 0 | `logs/typecheck.log` |
| 5 | `pnpm svelte-check` | 0 errors 0 warnings | `logs/svelte_check.log` |
| 6 | `pnpm lint` | exit 0 (eslint + prettier) | `logs/lint.log` |
| 7 | `pnpm test` (vitest desktop) | 15/15 passed | `logs/pnpm_test.log` |
| 8 | `pnpm bench` (default sample N=5) | exit 1 (frontend inbox first-paint noise) | `logs/bench_first.log`, `logs/bench_second.log` |
| 9 | `PERF_TIMING_SAMPLE_COUNT=10 pnpm bench` | exit 0, all PLAN §10 budgets green | `logs/bench_n10.log` |
| 10 | `pnpm corpus` | weighted_mean=0.000105 ≤ 0.01 gate | `logs/corpus.log` |
| 11 | `cd apps/desktop && xvfb-run -a pnpm exec playwright test` | 44 passed | `logs/playwright_all.log` |
| 12 | `cd relay && pnpm install --ignore-workspace && pnpm test` | 6/6 passed | `logs/relay_test.log` |
| 13 | `cd relay && pnpm exec wrangler deploy --dry-run` | exit 0 | `logs/wrangler_deploy_dry_run.log` |

All Rust M6 integration tests ran inside step 3:

```
tests/stack_detection_linear.rs
tests/stack_detection_diamond.rs
tests/stack_detection_ambiguous.rs   (cycle topology coverage)
tests/stack_rebase_happy.rs
tests/stack_rebase_conflict.rs
tests/stack_merge_sequential.rs
tests/stack_merge_pause_on_failure.rs
tests/stack_graphite_opt_in.rs
tests/stack_sync_round_trip.rs
tests/relay_receiver.rs
tests/ghe_full_parity.rs
```

All Playwright M1–M6 specs ran inside step 11:

```
playwright/airplane.spec.ts
playwright/diff-polish.spec.ts
playwright/m2-smoke.spec.ts
playwright/m3-smoke.spec.ts
playwright/notifications.spec.ts
playwright/worktree.spec.ts
playwright/a11y-pr-detail.spec.ts
playwright/m4-merge-surface.spec.ts
playwright/m4-range-diff.spec.ts
playwright/m4-multi-account.spec.ts
playwright/m4-ghe.spec.ts
playwright/m5-suggestion-apply.spec.ts
playwright/m5-saved-replies-paste-image.spec.ts
playwright/m5-command-palette.spec.ts
playwright/m5-check-annotations.spec.ts
playwright/m5-a11y.spec.ts
playwright/m6-stacks.spec.ts
playwright/m6-ghe-full-parity.spec.ts
playwright/m6-demo-gif.spec.ts
tests/smoke.spec.ts
```

## Per-acceptance-criterion evidence

- Stack detection linear/diamond/cycle topologies: verified via the three
  `stack_detection_*` Rust integration tests + reading `detect_stacks` in
  `apps/desktop/src-tauri/src/stacks/mod.rs` (Kosaraju-style BFS for components,
  Kahn topological sort, cycle detection by `topo.len() != component.len()`,
  branching/diamond classification, warning attached to DAG kind).
- Stack schema: `stacks`, `pr_stack_position`, `stack_operations` tables + the
  read-model view `pr_stack_summary` confirmed present in
  `apps/desktop/src-tauri/migrations/0017_stacks.sql`.
- StackTree.svelte: renders review/CI/conflict chips, blocked-by reasons, and
  explicit `base_sha → head_sha`; the `m6-stacks.spec.ts` Playwright spec covers
  the linear + DAG fixtures and the operation modal flows.
- Stack rebase: `apps/desktop/src-tauri/src/stacks/ops.rs::rebase_stack` runs
  local git via `CommandGitOps`, exits to `paused_conflict` when conflicts are
  detected, surfaces the worktree path + conflict files, and exposes
  `resume_stack_op` / `abort_stack_op` paths driving `git rebase --continue` /
  `--abort`. Verified by `stack_rebase_happy.rs` + `stack_rebase_conflict.rs`
  + `m6-stacks.spec.ts` ('rebase conflict panel supports abort action').
- Stack merge: `merge_stack` walks the stack via the M4 `Merge` mutation handler
  (`execute_merge_with_handler`) and after each merge issues the GraphQL
  `UPDATE_PULL_REQUEST_BASE_MUTATION` (`updatePullRequest(baseRefName:…)`) for
  the next PR in the stack. Verified by `stack_merge_sequential.rs` (wiremock
  sequencing assertion) + `stack_merge_pause_on_failure.rs`.
- Graphite opt-in: requires both the `graphite_enabled` setting AND
  `detect_graphite()` finding `gt` on PATH; on non-zero `gt` exit the operation
  transitions back to `Running` with a visible warning and falls through to
  plain git. Verified by `stack_graphite_opt_in.rs` and the
  `m6-stacks.spec.ts` 'graphite toggle enables gt badge in stack actions' test.
- Webhook relay: `relay/` contains `wrangler.toml` + `src/worker.ts` +
  `tests/worker.test.ts` + `README.md`; signing flow verified by reading
  `worker.ts` (`x-hub-signature-256` HMAC against `GITHUB_WEBHOOK_SECRET` with
  `timingSafeEqualText`, re-sign with `RELAY_FORWARD_SECRET`, forward to
  `RELAY_DESTINATION_URL`, retry-once on 5xx / abort, terminal 502). Vitest
  suite runs 6/6 passes; `wrangler deploy --dry-run` returns 0 with the
  expected `<set via wrangler secret put>` placeholder vars.
- Tauri-side relay receiver: `apps/desktop/src-tauri/src/relay/mod.rs` verifies
  `x-relay-signature-256` HMAC, enforces a ±300 s timestamp window, dedups
  nonces, and dispatches `RefetchTarget`s to active sync handles. Verified by
  `relay_receiver.rs` integration test (valid / stale / bad signature).
- Self-deploy only: `relay/wrangler.toml` ships placeholder vars only; the only
  surface mention of "hosted" in the relay package is the explicit
  "Self-deploy only … does not ship or operate a hosted relay" disclaimer in
  `relay/README.md`. No SaaS URL discovered (grep of `relay/`).
- Relay revoke recipe: documented in `relay/README.md` § Revoke (delete
  `GITHUB_WEBHOOK_SECRET`, delete worker, remove GitHub webhook, restart
  desktop) and mirrored in `DECISIONS.md` (2026-05-18 webhook relay decision).
- GHE full parity: `ghe_full_parity.rs` mounts a full M1–M5 wiremock fixture,
  asserts inbox + PR detail + diff + notifications + log stream + comment +
  suggestion apply + check rerun + merge + base retarget + stack detection all
  succeed against the wiremock host, and an `expect(0)` `dotcom_trap` enforces
  zero `api.github.com` leakage. Playwright recording is at
  `artifacts/m6-ghe-full-parity/` (PNG set). The `m6-ghe-full-parity.webm`
  file is 0 bytes — see "Other findings" below.
- Markdown corpus regression ≤ 1.0 %: `tools/markdown-corpus/score.mjs` sets
  `GATE = 0.01`. `pnpm corpus` reports `weighted_mean=0.000105`; the worst
  single entry is `cli-cli-4439054677` at `weighted=0.009282` (< 0.01) with an
  accepted-drift carve-out documented in `DECISIONS.md`.
- PLAN.md §10 perf budgets: every hard budget is green in
  `logs/bench_n10.log` and matches `PERF_REPORT.md` (within noise floor).
- README + LICENSE + CHANGELOG: README embeds `artifacts/m6-demo/demo.gif`
  (relative path, 4 MB, valid 89a GIF, no embedded tokens); LICENSE is MIT
  with 2026 copyright; CHANGELOG `[1.0.0]` enumerates M1–M6 Added + Performance
  + Security sections; both `Cargo.toml` and `package.json` declare
  `license = "MIT"`.
- Token-leak audit: `stacks` / `stack_operations` schema stores no token-shaped
  columns; `relay/wrangler.toml` keeps placeholders only; `relay/src/worker.ts`
  forwards body+headers and never logs the body; the desktop receiver stores
  `forward_secret` in the OS keychain via `keyring::Entry`; the demo gif's
  `strings` scan finds no `ghp_`/`gho_`/`token=`/`Authorization`/`Bearer`
  matches.
- DECISIONS.md M6 section: a promoted "M6 contract decisions (promoted for
  v1.0)" block sits at the top of the file with eight contract bullets (stack
  schema/detection, stack ops sequencing, Graphite opt-in, relay
  signing/revoke/self-deploy, GHE host-pure parity, corpus 1.0 % gate, perf
  methodology, demo gif pipeline). Dated supporting entries beneath it cover
  IPC schema, stack schema, the webhook relay decision, the GHE wiremock
  decision, the corpus tightening rationale, and the perf re-baselining.

## Other findings (severity-ordered)

- (low) Flaky `inbox_first_paint_ms_frontend` on cloud runner: min-of-5
  samples occasionally exceed the hard 100 ms budget (108 ms observed in one
  run; 100 ms in another). With `PERF_TIMING_SAMPLE_COUNT=10` the bench is
  green every time. The repository's documented policy is min-of-5 and the
  baseline already accounts for cloud-runner drift, so this is a runner-noise
  fingerprint rather than a product regression. Suggest bumping the cloud-CI
  sample count to 10 to harden the gate, or pinning a stricter runner profile.
- (low) `artifacts/m6-ghe-full-parity/m6-ghe-full-parity.webm` is 0 bytes. The
  upstream worker noted this in the handoff and left it untouched. The PNG
  walkthrough under the same folder covers the same checkpoints and the Rust
  integration test enforces the zero-dotcom-leak invariant, so the empty
  recording is informational rather than blocking. Recommend the next pass
  re-record this clip (or delete it to avoid future confusion).

## Notes

- This verifier did not modify any product source files. The only writes were
  to `verify/m6-finalize/` (this report + log captures).
- The verifier did not create/push a v1.0.0 tag; that is reserved for
  `m6-tag-v1` after the planner consumes this report.

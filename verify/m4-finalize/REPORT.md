# M4 finalize verification report

Branch verified: `orch/m4-merge-multiaccount/m4-finalize` (HEAD `98bac50`).

## Step 0 chain-integration sanity

```
$ git fetch origin orch/m4-merge-multiaccount/m4-merge-surface \
                   orch/m4-merge-multiaccount/m4-force-push-range-diff \
                   orch/m4-merge-multiaccount/m4-multi-account
$ git log --oneline origin/orch/m4-merge-multiaccount/m4-merge-surface ^HEAD          # (empty)
$ git log --oneline origin/orch/m4-merge-multiaccount/m4-force-push-range-diff ^HEAD  # (empty)
$ git log --oneline origin/orch/m4-merge-multiaccount/m4-multi-account ^HEAD          # (empty)
```

All three M4 worker branches are already fully merged into the deliverable
branch — no further integration commits required.

## Local CI matrix

| Check                                    | Result                                 |
| ---------------------------------------- | -------------------------------------- |
| `cargo fmt --check`                      | clean                                  |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (`Finished dev profile target(s) in 4m 55s`) |
| `cargo test --workspace --no-fail-fast`  | 90 tests across 44 binaries, 0 failed  |
| `pnpm typecheck`                         | clean (`tsc --noEmit` succeeded)       |
| `pnpm svelte-check`                      | `0 errors and 0 warnings`              |
| `pnpm lint`                              | ESLint + Prettier clean                |
| `pnpm test` (vitest)                     | 9 files / 11 tests passed              |
| `pnpm bench`                             | every PLAN §10 budget met (see below)  |
| `pnpm corpus`                            | weighted_mean = 0.000115 ≤ 0.015       |
| Playwright (M2 + M3 + M4 specs, xvfb)    | 18 passed / 18                         |

### Cargo test counts (`cargo-test.log`)

44 test binaries ran with `--no-fail-fast`; every `test result:` line reported
`ok`. Summed pass count from `^test result: ok\. <N> passed`: **90**, failed:
**0**. M4-specific binaries included: `airplane_drill`,
`composer_posting_identity`, `ghe_endpoint_derivation`, `ghe_round_trip`,
`ghe_token_storage`, `multi_account_inbox`, `multi_account_rate_limit`,
`mutations_harness`, `notifications_*` suite, `push_history_reconcile`,
`range_diff_fallback`, `range_diff_local_git`, `range_diff_rest_compare`,
`worktree_*` suite.

Flake observed (not a real failure): in a parallel first cargo-test run
(running concurrently with multiple other CPU-bound jobs),
`db_integration::open_fixture_loads_inbox_under_timing_budget_best_effort`
clocked 567 ms (>500 ms shared-runner regression assertion). The same test
passes in isolation and again in the second, less-loaded full workspace run.

### `pnpm bench` results

| Metric                                | Observed     | Budget        |
| ------------------------------------- | ------------ | ------------- |
| inbox_first_paint_ms                  | 27.57        | ≤ 100         |
| pr_detail_open_preloaded_ms           | 0.77         | ≤ 50          |
| pr_detail_open_cold_ms                | 28.14        | ≤ 250         |
| file_open_in_diff_cached_ms           | 0.69         | ≤ 100         |
| mutation_submit_visible_ms            | 0.39         | ≤ 16          |
| mutation_submit_visible_online_ms     | 0.38         | ≤ 16          |
| mutation_submit_visible_offline_ms    | 0.39         | ≤ 16          |
| comrak_render_throughput_ops_per_sec  | 36548.06     | ≥ 850         |
| inbox_first_paint_ms_frontend         | 98.00        | ≤ 100         |
| pr_detail_open_preloaded_ms_frontend  | 24.00        | ≤ 50          |
| pr_detail_open_cold_ms_frontend       | 56.00        | ≤ 250         |
| file_open_in_diff_cached_ms_frontend  | 48.00        | ≤ 100         |
| diff_scroll_fps                       | 62.14        | ≥ 60          |
| diff_scroll_frame_p95_ms              | 16.39        | ≤ 16.7        |

All within budget on the cloud-agent runner.

### Playwright sweep

```
18 passed (31.5s)
```

Specs:
- airplane.spec.ts
- m2-smoke.spec.ts
- m3-smoke.spec.ts  (3 cases)
- diff-polish.spec.ts
- worktree.spec.ts
- notifications.spec.ts
- m4-merge-surface.spec.ts (branch protection variations none/soft/hard;
  merge confirmation reconcile; merge + delete-branch sequencing; auto-merge
  toggle; merge queue enqueue/reorder/dequeue; update-branch transitions;
  mergeable-null backoff screenshots)
- m4-range-diff.spec.ts (local-git mode; rest mode; force-push banner)
- m4-multi-account.spec.ts (switcher, badges, composer identity, rate-meter)
- m4-ghe.spec.ts (add account, switch, inbox/detail, endpoint routing proof)

Two flake observations during interleaved runs (not real failures):
- `m4-range-diff › rest mode renders the same range-diff structure` failed
  once when running alongside another job; passed in isolation and in a clean
  rerun.
- A whole-suite rerun failed with "Connection refused" on most specs while a
  concurrent `pnpm bench` was holding port 4173 (the bench has its own
  preview-server step). The clean run with no concurrent bench shows 18/18.

## Mandatory M4 spot-reads

### Merge surface code

- `apps/desktop/src-tauri/migrations/0010_merge_surface.sql` adds all 22
  merge-surface columns (`merge_commit_allowed`, `squash_merge_allowed`,
  `rebase_merge_allowed`, `delete_branch_on_merge_default`, `viewer_can_*`,
  `auto_merge_*`, `merge_queue_entry_*`, `branch_protection_summary_json`,
  `repo_has_merge_queue`, `head_ref_state`) and rebuilds `pr_detail_summary`
  view to expose them.
- `apps/desktop/src-tauri/src/api/queries/PrDetail.graphql` queries
  `mergeCommitAllowed/squashMergeAllowed/rebaseMergeAllowed/deleteBranchOnMerge`
  + `mergeQueue` + `defaultBranchRef.branchProtectionRule` (with all required
  fields). No hardcoded mapping in the frontend.
- `apps/desktop/src-tauri/src/api/queries/mutations/` contains the three new
  merge-queue GraphQL files: `enqueuePullRequest.graphql`,
  `dequeuePullRequest.graphql`, `reorderMergeQueueEntry.graphql`.
- `apps/desktop/src-tauri/src/mutations/mod.rs` `MutationKind` enum contains
  the three new variants (`EnqueueMergeQueue`, `DequeueMergeQueue`,
  `ReorderMergeQueue`) alongside `EnableAutoMerge`, `DisableAutoMerge`,
  `UpdateBranch`, `Merge`, `DeleteHeadRef`.
- `apps/desktop/src/lib/components/merge/NoOptimismButton.svelte` implements
  the contract: click → confirmation modal → spinner → wait for
  `mutation:reconciled` (or `mutation:failed`) before resolving. No projected
  state.
- `apps/desktop/src/lib/components/merge/MergeBox.svelte` uses
  `NoOptimismButton` for `merge`, `enable_auto_merge`, `disable_auto_merge`,
  `update_branch`, `enqueue_merge_queue`, `dequeue_merge_queue`,
  `reorder_merge_queue`. The delete-after-merge chain awaits the merge
  mutation's `mutation:reconciled` before submitting `delete_head_ref` (also
  through the no-optimism reconcile-wait path).

### Mergeable-null backoff

`apps/desktop/src-tauri/src/sync/mod.rs::run_mergeable_backoff` runs the
schedule `[2, 5, 15, 45, 120]` seconds and then loops at 300 s. The bundled
unit test `mergeable_backoff_emits_tick_schedule` asserts
`observed_sleeps == [2, 5, 15, 45, 120, 300]` against a `MockClock`. Recorded
screenshot sequence `tick-001-2s.png` through `tick-007-resolved.png` lives
under `artifacts/m4-mergeable-backoff/`.

### Force-push range-diff

- `apps/desktop/src-tauri/src/range_diff/mod.rs`
  - `LocalGitRangeDiff::compute` shells out to
    `git range-diff --no-color --no-notes <base>...<old> <base>...<new>` (line 191).
  - `RestCompareRangeDiff::compute` fetches GitHub `/repos/{owner}/{name}/compare/{base}...{head}`
    and computes pairs + intra-line highlights in Rust (no shell-out).
  - Worktree-missing-commit fallback returns `WorktreeMissingCommits` to
    upstream so IPC re-routes to the REST provider.
- Test binaries: `range_diff_local_git`, `range_diff_rest_compare`,
  `range_diff_fallback` all green.
- Recordings: `artifacts/m4-range-diff/local-git.png` and
  `artifacts/m4-range-diff/rest-compare.png` (different fixtures of the same
  PR rendered in both modes).

### RateLimitBudgeter keyed by (account_id, ApiResource)

`apps/desktop/src-tauri/src/sync/mod.rs`:
```
let mut buckets = HashMap::<(String, ApiResource), BudgetState>::new();
```
Throttle check `account_is_throttled(buckets, account_id)` and the bypass
emit path (`emit_rate_limit_bypass(account_id, &snapshot)`) are both keyed by
account id. Test `multi_account_rate_limit.rs` is green.

Foreground bypass observable: `artifacts/m4-foreground-bypass/` shows
low-budget meter (red), background throttled, and bypass chip on foreground
"Refresh PR" click.

### Multi-account UI

`apps/desktop/src/lib/components/account/AccountSwitcher.svelte`,
`AccountBadge.svelte`, and `apps/desktop/src/lib/components/status/RateLimitMeter.svelte`
render switcher/badges/meter. `m4-multi-account` Playwright spec exercises
switcher → badged aggregated inbox → composer posting identity drop-down →
rate meter. `multi_account_inbox`, `multi_account_rate_limit`, and
`composer_posting_identity` Rust tests are green. Recordings:
`artifacts/m4-multi-account/inbox-aggregated.png` and
`composer-identity-dropdown-open.png`.

### GHE schema readiness

`apps/desktop/src-tauri/src/auth/mod.rs::derive_endpoint_config` selects
`https://api.github.com` for dotcom and `https://<host>/api/v3` +
`https://<host>/api/graphql` for non-dotcom; `PR_COCKPIT_HOSTS_TOML` and
platform config locations supply host overrides.

`ghe_round_trip.rs` registers a wiremock host as `ghe.local` (with overrides
pointing to the wiremock server.uri()), sets `github_api_origin` to a dead
loopback (`http://127.0.0.1:9`), then runs `InboxRefresh` + `PrDetail` +
`pulls/.../{n}` and asserts dotcom inbox stays empty — proving the GHE
account does not leak to `api.github.com`. Test is green. Recordings:
`artifacts/m4-ghe/01-add-account.png`, `02-ghe-inbox.png`,
`03-ghe-pr-detail.png`.

## Token-leak audit (rerun on M4 surface)

- `accounts` SQL table has no token column; `token_kind` and scope strings
  only (`apps/desktop/src-tauri/migrations/0004_auth_accounts.sql`).
- `rate_limit_buckets` carries `account_id`, `resource`, `remaining`, `used`,
  `limit_total`, `reset_at`, `updated_at` — no token bytes
  (`0001_initial_schema.sql` + `0012_multi_account_rate_limits.sql`).
- `pending_mutations` carries payload JSON only — no token leak path; the
  mutation handlers fetch tokens via `AuthService` on dispatch.
- Tokens reside in `KeyringTokenStore` (`auth/mod.rs`) and flow into a
  reqwest `Authorization: Bearer …` header with `set_sensitive(true)` for
  endpoint probes and via `TokenClient::request_with` for normal API calls.
  `ipc_token_safety.rs` integration test asserts tracing does not leak token
  bytes; it is green.
- Grep for `\btoken\b` / `\bsecret\b` / `bearer` in migration SQL → no
  matches.

## Artifacts summary

All scenario evidence is present and cross-linked from
`artifacts/m4-smoke/README.md`. PNG files inspected with `file(1)` are real
non-trivial RGBA images at 1280×N resolution.

## Verifier logs preserved

This directory (`verify/m4-finalize/`) holds the raw logs the verdict cites:
`cargo-clippy.log`, `cargo-test.log`, `pnpm-typecheck.log`,
`pnpm-svelte-check.log`, `pnpm-lint.log`, `pnpm-test.log`, `pnpm-bench.log`,
`pnpm-corpus.log`, `playwright-sweep.log`.

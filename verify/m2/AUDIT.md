# M2 verifier audit — `m2-finalize`

Branch verified: `orch/m2-optimistic-writes/m2-finalize` @ `5d95f01`
Verifier branch: same (no rename; only verifier artifacts added under `verify/m2/` and `artifacts/m2-verify/`).
Verifier date: 2026-05-17.

## Scope

Verify the M2 deliverable against:
- PLAN.md §3 (optimistic update model) + §12 M2 (milestone exit criteria) + §10 (perf budgets).
- AUTONOMY_BRIEF.md §M2 acceptance criteria.
- `.orchestrate/m2-optimistic-writes/plan.json` subtree-level acceptance criteria.
- The verifier-mandatory checks attached to `m2-finalize`.

## Environment

- Ubuntu 24.04 cloud-agent VM.
- Rust stable 1.95.0 (`rust-toolchain.toml`).
- Node v22.22.2 / pnpm 9.12.3.
- Tauri Linux deps installed at verifier-time via apt:
  `libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev
   libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config build-essential`.
  (These were NOT pre-baked on the agent VM — installing them is recommended
  for future M2/M3 verifiers via an env-setup agent.)
- Playwright WebKit + headless Chromium (`playwright install --with-deps webkit chromium`).
- `pnpm install --frozen-lockfile` succeeded against the pinned lockfile.
- `scripts/fetch-grammars.sh` provisioned the 8 tree-sitter grammars.

## Results table

| Step | Command | Result | Log |
| ---- | ------- | ------ | --- |
| rust fmt | `cargo fmt --check` | exit 0 | `verify/m2/logs/cargo-fmt.log` |
| rust clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0, 0 warnings, finished in 4m26s | `verify/m2/logs/cargo-clippy.log` |
| rust tests | `cargo test --workspace` | exit 0; 24 test binaries; 47 passed / 0 failed (incl. `airplane_drill::airplane_offline_queue_replay_drill`, `mutation_engine_proptest` 4 properties, `mutations_collaboration`, `mutations_comments`, `mutations_e2e`, `mutations_pr_meta`, `mutations_reviews_threads`, `canonical_queries`, `ipc_token_safety`, `renderer_isolation`, `token_safety`, `db_integration` 5, `sync_integration` 6, `render` 8, `perf_smoke`, `ipc_bindings`, `online_demo`) | `verify/m2/logs/cargo-test.log` |
| frontend typecheck | `pnpm typecheck` | exit 0 (specta IPC bindings regenerated, then `svelte-kit sync && tsc --noEmit`) | `verify/m2/logs/pnpm-typecheck.log` |
| svelte-check | `pnpm svelte-check` | exit 0, 0 errors / 0 warnings | `verify/m2/logs/pnpm-svelte-check.log` |
| eslint + prettier | `pnpm lint` | exit 0 (eslint clean, prettier all match) | `verify/m2/logs/pnpm-lint.log` |
| frontend unit tests | `pnpm test` (vitest) | exit 0, 7 files / 8 tests passed | `verify/m2/logs/pnpm-test.log` |
| markdown corpus regression | `pnpm corpus` | weighted_mean=0.015459 ≤ gate 0.02 (200 entries) | `verify/m2/logs/pnpm-corpus.log` |
| headless perf bench | `xvfb-run -a pnpm bench` (criterion + Playwright/WebKit) | all 14 PLAN §10 + M2 budgets met (see table below) | `verify/m2/logs/pnpm-bench.log` |
| Playwright airplane drill | `xvfb-run -a pnpm --filter desktop test:airplane` | 1/1 passed (2.1 s test, 7.7 s wall) | `verify/m2/logs/airplane-spec.log` |
| Playwright M2 smoke | `xvfb-run -a pnpm --filter desktop exec playwright test playwright/m2-smoke.spec.ts` | 1/1 passed (3.6 s test, 8.4 s wall); video + trace captured | `verify/m2/logs/m2-smoke.log`, `artifacts/m2-verify/m2-smoke-rerun.webm`, `artifacts/m2-verify/m2-smoke-rerun-trace.zip` |

## PLAN §10 + M2 budgets — measured this run

```
inbox_first_paint_ms                      21.22 ms   <= 100    ms      PASS
pr_detail_open_preloaded_ms                0.63 ms   <= 50     ms      PASS
pr_detail_open_cold_ms                    22.25 ms   <= 250    ms      PASS
file_open_in_diff_cached_ms                0.60 ms   <= 100    ms      PASS
mutation_submit_visible_ms                 0.38 ms   <= 16     ms      PASS
mutation_submit_visible_online_ms          0.37 ms   <= 16     ms      PASS
mutation_submit_visible_offline_ms         0.38 ms   <= 16     ms      PASS
comrak_render_throughput_ops_per_sec   36371.01 ops/s >= 850   ops/s   PASS
inbox_first_paint_ms_frontend             21.00 ms   <= 100    ms      PASS
pr_detail_open_preloaded_ms_frontend      20.00 ms   <= 50     ms      PASS
pr_detail_open_cold_ms_frontend           26.00 ms   <= 250    ms      PASS
file_open_in_diff_cached_ms_frontend      60.00 ms   <= 100    ms      PASS
diff_scroll_fps                           62.16 fps  >= 60     fps     PASS
diff_scroll_frame_p95_ms                  16.20 ms   <= 16.7   ms      PASS
```

`mutation_submit_visible_ms` is well below the < 16 ms hard budget on BOTH
online and offline paths on the cloud-agent runner (0.37 ms / 0.38 ms).
The min-of-N policy was NOT needed at this margin; raw single-run readings
are >40× under budget.

## Mandatory verifier checks

### M-1. 25-mutation-kind enumeration with predict/apply/rollback/reconcile

`MutationKind` enum + `handlers::all()` registers 27 handlers (PLAN.md
wording is "~25"; the consolidated DECISIONS top section notes the
intentional 27-kind surface). Every PLAN.md §3.1 kind listed in the brief
is present:

```
$ grep -nE "^impl Mutation for" apps/desktop/src-tauri/src/mutations/handlers/*.rs
assignees.rs:18:    SetAssignees
comments.rs:28      AddComment
comments.rs:313     EditComment
comments.rs:449     DeleteComment
labels.rs:20        AddLabel
labels.rs:127       RemoveLabel
merge_controls.rs:29   EnableAutoMerge
merge_controls.rs:68   DisableAutoMerge
merge_controls.rs:105  UpdateBranch
merge_controls.rs:186  Merge
merge_controls.rs:284  ClosePr
merge_controls.rs:299  ReopenPr
pr_meta.rs:34       UpdatePrTitle
pr_meta.rs:49       UpdatePrDescription
pr_meta.rs:64       SetMilestone
pr_meta.rs:178      SetProject
pr_meta.rs:282      ConvertToDraft
pr_meta.rs:297      MarkReadyForReview
reactions.rs:22     AddReaction
reactions.rs:72     RemoveReaction
reviewers.rs:21     RequestReview
reviewers.rs:39     RemoveReviewRequest
reviews.rs:22       SubmitReview
threads.rs:24       ResolveThread
threads.rs:42       UnresolveThread
viewed_files.rs:20  MarkFileViewed
viewed_files.rs:38  UnmarkFileViewed
```

Every impl provides `fn kind()`, `fn optimism()`, `fn predict(...)`, and
`fn apply<'a>(...)`. `rollback()` + `reconcile()` are supplied by the
`Mutation` trait default impls in `mutations/mod.rs` (rollback applies the
stored inverse patch via `projector::apply_patch(..., PatchSource::Rollback)`
inside one transaction; reconcile delegates to `reconciler::reconcile`).
`DeleteComment` overrides `reconcile` explicitly to use the shared
reconciler. This satisfies the "predict/apply/rollback/reconcile" surface.

Spot-checked impls by reading the bodies for: AddComment, DeleteComment,
ResolveThread, ReactionAdd, MarkFileViewed (covering REST + GraphQL +
add/remove/toggle/insert variants). Each constructs a `Patch` with row
mutations, returns `PredictedEffect`, calls into `api::GithubClient`
with `Idempotency-Key`, and matches the optimism level required.

`mutation_engine_proptest::dispatch_table_covers_full_kind_set` test
asserts the dispatch table covers all kinds and passes.

### M-2. Optimism level non-negotiables (PLAN.md §3.2)

PLAN.md "no optimism" list: merge, squash, rebase, force-push, branch
delete, dismiss review, enqueue merge queue.

Inspected `fn optimism()` returns:

```
merge_controls.rs:33→34   EnableAutoMerge   -> OptimismLevel::None    ✓
merge_controls.rs:72→73   DisableAutoMerge  -> OptimismLevel::None    ✓
merge_controls.rs:109→110 UpdateBranch      -> OptimismLevel::Cautious ✓ (PLAN §3.2 places UpdateBranch under Cautious)
merge_controls.rs:190→191 Merge             -> OptimismLevel::None    ✓
merge_controls.rs:288→289 ClosePr           -> OptimismLevel::Full    (PLAN does not place ClosePr in the "no optimism" set; idempotent state toggle)
merge_controls.rs:303→304 ReopenPr          -> OptimismLevel::Full    (same)
pr_meta.rs (SubmitReview is in reviews.rs)
reviews.rs:27→28          SubmitReview      -> OptimismLevel::Cautious ✓
pr_meta.rs:182→183        SetProject        -> OptimismLevel::Cautious ✓
pr_meta.rs:286→287        ConvertToDraft    -> OptimismLevel::Cautious ✓
pr_meta.rs:301→302        MarkReadyForReview-> OptimismLevel::Cautious ✓
remaining shipped M2 kinds (AddComment / EditComment / DeleteComment /
AddReaction / RemoveReaction / AddLabel / RemoveLabel / SetAssignees /
RequestReview / RemoveReviewRequest / ResolveThread / UnresolveThread /
MarkFileViewed / UnmarkFileViewed / UpdatePrTitle / UpdatePrDescription /
SetMilestone) -> OptimismLevel::Full ✓
```

`merge`, `enable_auto_merge`, `disable_auto_merge` are correctly `None`.
The remaining PLAN "no optimism" actions in the abstract list (squash /
rebase / force-push / branch delete / dismiss review / enqueue merge
queue) are NOT separate mutation kinds in the M2 deliverable (per
PLAN.md §3.1's enumerated 25-kind set). No optimism-policy bug.

### M-3. Composer Preview parity (single render_* Rust function)

Verified via reading the IPC module:

```
apps/desktop/src-tauri/src/ipc/mod.rs:1223 ipc_rendered_comment_html_impl
                                            -> render::render_comment(...)
apps/desktop/src-tauri/src/ipc/mod.rs:1536 render_preview_impl(input)
                                            -> ipc_rendered_comment_html_impl(...)
apps/desktop/src-tauri/src/ipc/mod.rs:1743 #[tauri::command] render_preview
                                            -> render_preview_impl
```

So the composer Preview tab and the timeline both go through
`render::render_comment` (comrak + ammonia, single content-hash cache).
This is parity by construction (one Rust function).

Renderer side: `apps/desktop/src/lib/components/composer-model.ts:5`
calls `renderPreview(body, repo)` which in `apps/desktop/src/lib/ipc/client.ts:295`
invokes `commands.renderPreview({ body, ctx: { repo } })` — i.e. the
`render_preview` IPC. Vitest test `Composer.parity.test.ts` exists and
passes.

ESLint guardrail at `apps/desktop/.eslintrc.cjs` bans `marked`,
`markdown-it`, `remark*`, `unified`, `showdown`, and direct
`fetch(...)` from `apps/desktop/src/**/*`. Grep confirms zero usage of
those JS markdown libraries anywhere in `apps/desktop/src`.

### M-4. Airplane drill — full 10-comments + label-churn + thread-resolve + engine reboot

`apps/desktop/src-tauri/tests/airplane_drill.rs` covers:
- 10 AddComment offline (with `@mentions`, `#refs`, fences, ` ``` `, lists, quotes)
- 3 AddLabel + 3 RemoveLabel offline across 2 PRs
- 4 ResolveThread offline
- 1 Merge submission asserted to return `requires_confirmation = true`
- 21 rows assertable as `pending` before reboot
- engine drop + reconstruct (lines 225–243) — "survives an engine reboot"
- network probe flip Online → `engine.drain().await` returns `applied=20, reconciled=20, failed=0`
- ordered wiremock request sequence: 10× `POST /issues/1/comments` then `POST/DELETE` label calls in submission order then 4× `POST /graphql`
- id_mappings: 10 `air-local-*` mappings asserted; zero remaining local-id comments
- exactly one remaining pending row = the merge mutation with `requires_connection_confirmation = 1`
- draft survival (`db.list_drafts(...).len() == 1` post-drain)
- read-model convergence (PR detail summary + review thread resolved counts)

Cargo test ran the drill in 0.15 s wall clock and passed.

Playwright UI drill `apps/desktop/playwright/airplane.spec.ts` also passed
(2.1 s test) against the headless WebKit preview build.

### M-5. mutation_submit_visible_ms < 16 ms on both paths

`mutation_submit_visible_online_ms = 0.37 ms` and
`mutation_submit_visible_offline_ms = 0.38 ms` on the cloud-agent runner.
Both ≪ 16 ms, single-run, no min-of-N needed.

### M-6. DECISIONS.md M2 section enumerates required topics

Section-by-section check:

| Topic                                         | DECISIONS.md location |
| --------------------------------------------- | --------------------- |
| Patch schema (row-level before/after + example) | line 356–387 "M2 optimistic write algebra ..." |
| body_server_adjusted semantics                | line 389–398 "`body_server_adjusted` marks normalization deltas ..." |
| retry/backoff policy                          | line 400–412 "Mutation retry/backoff policy is exponential ..." |
| Proptest deterministic seed recipe            | line 414–425 "Proptest deterministic seed reproduction ..." |
| Per-kind transport + idempotency              | line 427–465 "M2 mutation transport split, idempotency policy, and optimism tiers ..." |
| NetworkMonitor + requires_connection_confirmation + hard-conflict diff schema + airplane drill recipe | line 467–495 "M2 offline queue monitor, confirmation gating, hard-conflict payload, and airplane drill contract" |
| Composer parity strategy + sync-errors UX + offline pill + eslint guardrails | line 497–509 "M2 frontend mutation UX uses one IPC markdown renderer ..." |
| M2 contract decisions promoted for M3+        | line 1–32 (top of file) |

All seven topics demanded by the verifier brief are enumerated and
contradiction-free (the top section consolidates the long-form entries).

### M-7. Token-leak audit of M2 surface

Greps across `apps/desktop/src-tauri/src/mutations/**` and the
`0005_optimistic_writes.sql` + `0006_offline_queue_and_drafts.sql`
migrations return ZERO matches for:

- `token`, `Authorization`, `ghp_`, `gho_`, `github_pat_`, `access_token`, `secret`.

The M2 mutation engine routes every GitHub call through the existing
`api::GithubClient` (`auth::TokenClient` → keyring), the same path M1
covered with `tests/token_safety.rs` + `tests/ipc_token_safety.rs`.
Both M1 tests passed in this re-run (1+1 = 2 passes), so the keychain
invariant is preserved on the M2 surface:

- `pending_mutations` has no token column; `input_json` is per-kind
  payload (label name, body text, comment id, …) — no auth material.
- `drafts` stores body text only.
- `mutation_attempts` records outcome / latency / http_status / error_kind
  only.

No token bytes appear in any M2-side persisted row or log structure.

## Verifier-specific acceptance criteria — checklist

- [x] Verification section includes execution evidence for every PLAN.md §3 + §12 M2 + AUTONOMY_BRIEF.md §M2 + subtree-level acceptance criterion.
- [x] All 25 (in practice 27) mutation kinds verified with predict/apply/rollback/reconcile + the correct optimism level per PLAN.md §3.2.
- [x] Airplane drill executed end-to-end with concrete pass/fail evidence; mutation submit budget < 16 ms checked on the cloud-agent runner.
- [x] Markdown corpus regression rerun: weighted_mean = 0.015459 ≤ 0.02.
- [x] Token-leak check rerun on the M2 surface (pending_mutations + drafts + mutation_attempts + mutation logs); ZERO token bytes found.
- [x] No verifier-failed status raised; every criterion either green or explicitly out-of-scope (week-long human dogfood gate).

## Subtree acceptance — observed

- Optimism levels non-negotiable per PLAN.md §3.2 (Merge/EnableAutoMerge/DisableAutoMerge are `None`): ✓
- Composer + Preview through `render_preview` IPC only (one Rust function): ✓
- Inverse-patch projection on the SQLite side (`mutations::projector::apply_patch`); UI subscribes via cache-invalidation events (`mutation:*`, `pr:<id> changed`): ✓
- Renderer never opens SQLite, never talks to GitHub directly: M1 `renderer_isolation.rs` test still green; ESLint `no-restricted-syntax` bans direct `fetch(...)`: ✓
- Tokens keychain-only; no token bytes in pending_mutations / drafts / mutation logs: ✓ (greps + M1 token tests pass)
- Full local CI matrix green: cargo fmt/clippy/test, pnpm typecheck/svelte-check/lint/test, pnpm bench, pnpm corpus, Playwright airplane.spec, Playwright m2-smoke.spec: ✓

## Live UI confirmation

Headless WebKit Playwright drove the real Svelte 5 app served from
`pnpm preview` for both `airplane.spec.ts` and `m2-smoke.spec.ts`:

1. Opened `Active Fixture PR Falcon Diff Stress` at `/pr/pr_1`.
2. Composer textarea + Add-comment submit shows optimistic projection
   inside a frame; pending affordance visible.
3. Label add/remove, assignee set, review request, thread resolve, file
   viewed — all action paths painted optimistic state.
4. `window.__M2_DEBUG__.setOffline()` flipped `network:<account> changed`
   → "Offline — queued: N" pill rendered.
5. 3 offline comments + 2 offline label changes queued with pending pills.
6. `setOnline()` drained the queue; all pending pills cleared; the
   sync-errors tray reads "No failed mutations."
7. 30 s video + trace.zip captured to `artifacts/m2-verify/`.

## History sanity

```
$ git log --oneline | head -10
5d95f01 chore(desktop): finalize m2 smoke docs and stability
df5c78d feat(desktop): wire optimistic mutation UI surfaces
b78fda0 feat(desktop): add offline queue replay and airplane drill
429cb16 feat(desktop): implement m2 mutation handlers and wiremock coverage
d4e16e3 feat(desktop): add m2 optimistic mutation engine core
dc081c4 orch: m2-optimistic-writes m2-finalize pending -> running
...
```

Migrations in order: 0001 → 0002 → 0003 → 0004 → 0005_optimistic_writes.sql
(+ down) → 0006_offline_queue_and_drafts.sql. Coherent linear M2 history
on top of the M1 deliverable.

## Caveats / non-blocking observations

- **Hard-conflict UI modal** is not exercised end-to-end by an automated
  smoke. `m2-finalize` documented this in `artifacts/m2-smoke/NOTES.md`:
  the close-PR-mid-compose debug command is not exposed in the headless
  fixture harness. The engine path is covered by mutation tests
  (`mutations_collaboration`, `mutations_e2e`) and the IPC wiring
  (`MutationEvent::HardConflict` → `mutation:hard-conflict` event → diff
  modal in `apps/desktop/src/lib/components/`) is in place. Recommended
  follow-up: expose a deterministic `__M2_DEBUG__.forceHardConflict()`
  hook so a Playwright spec can exercise the diff modal.
- **No native Tauri window booted.** The cloud-agent VM only has
  headless WebKit. All UI is exercised via Playwright WebKit driving
  the same Svelte 5 app the Tauri preview build serves, but a real
  Tauri window pass on macOS/Linux desktop is still a separate
  human/manual gate.
- **Verifier-installed deps not pre-baked.** This verifier had to apt-install
  Tauri build deps (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, …) and run
  `playwright install --with-deps webkit chromium`. Future M2/M3 verifiers
  on a fresh VM will need the same env-setup. Recommended one-line env
  agent prompt: install `libgtk-3-dev libglib2.0-dev libwebkit2gtk-4.1-dev
  libayatana-appindicator3-dev librsvg2-dev patchelf xvfb` plus
  Playwright Chromium+WebKit before clippy/test/bench/playwright.
- **Week-long dogfood gates** in AUTONOMY_BRIEF.md §M2 + PLAN.md §12 M2
  ("dogfood gate: comments + reviews + labels + thread resolution as
  primary for one week") are explicitly out of scope for the cloud agent
  per the plan.json mapping. They remain human gates.

## Verdict

Every automated acceptance criterion from PLAN.md §3 + §12 M2,
AUTONOMY_BRIEF.md §M2, plan.json subtree-level criteria, and the
verifier mandatory checklist is green on a fresh cloud-agent VM:

- 27 mutation handlers (PLAN's "~25" surface) with predict + apply + rollback + reconcile.
- Optimism levels match PLAN.md §3.2 (Merge / EnableAutoMerge / DisableAutoMerge = None).
- Composer preview goes through the same `render::render_comment` Rust
  function as the timeline.
- Airplane drill: 10 comments + 6 label changes + 4 thread resolves + 1
  guarded Merge, engine reboot, then drain returns 20 applied + 20
  reconciled + 1 remaining `requires_connection_confirmation` merge.
- `mutation_submit_visible_ms = 0.37/0.38 ms` ≪ 16 ms on online + offline.
- Markdown corpus weighted_mean = 0.015459 ≤ 0.02 gate.
- Token-leak audit: zero token bytes on the M2 surface.
- Full local CI matrix green (cargo fmt/clippy/test, pnpm typecheck/
  svelte-check/lint/test/bench/corpus, Playwright airplane.spec +
  m2-smoke.spec).

The strongest verifier claim my evidence supports is `live-ui-verified`
— the optimistic flow, the offline queue, the reconciled drain, and the
sync-errors tray were all observed under a real headless WebKit driving
the Svelte 5 app over typed IPC, with the captured screen recording at
`artifacts/m2-verify/m2-smoke-rerun.webm`.

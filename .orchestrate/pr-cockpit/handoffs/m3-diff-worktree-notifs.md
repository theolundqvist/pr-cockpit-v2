<!-- orchestrate handoff
task: m3-diff-worktree-notifs
branch: orch/pr-cockpit/m3-diff-worktree-notifs
agentId: bc-f868f69e-ef2d-4534-a00f-f560772f9614
runId: run-f82b37e2-4030-4613-b55f-96bbb7c3e645
resultStatus: finished
finishedAt: 2026-05-18T00:12:39.546Z
-->

All work complete. Producing my parent-facing handoff.

## Status
success

## Branch
`orch/pr-cockpit/m3-diff-worktree-notifs`

## What my subtree did
- **Diff polish (m3-diff-polish)**: shipped `MutationKind::AddReviewComment` (OptimismLevel::Full) with predict/apply/rollback/reconcile, `addReviewComment.graphql` + `addReviewThread.graphql` under the canonical mutations dir; selection-to-comment UX in `DiffViewer.svelte` (shift-line-range + gutter affordance + inline composer); `[+ Suggestion]` toolbar button in Composer that injects a ```suggestion fence (renders via existing comrak Preview, parity-tested); head_sha-scoped viewed checkbox (read-model SQL `viewed_at_head_sha = pull_requests.head_sha`); Tauri local-asset protocol scoped to `$APPDATA/blobs/**/*` for inline images; binary-file Primer placeholder; rename detection (`previous_path` + `rename_similarity` in `0007_diff_polish.sql`); seeded fixtures extended with image + binary + renamed rows; Playwright `diff-polish.spec.ts` passes.
- **Worktree read (m3-worktree-read)**: new Rust module `worktree/{discovery,watcher,mapping,mod}.rs` — root-scoped discovery against `~/dev|~/code|~/src|~/repos` (no `$HOME` autoscan), `.github-pr-cockpit.toml` overrides, capped concurrency; `notify` watcher with 200–500 ms debounce on `.git/HEAD`/`.git/refs/`/`.git/index`/working tree, refresh via `git status --porcelain=v2 --branch` + `git rev-list --left-right --count`; multi-signal weighted mapping (remote URL 0.30, upstream 0.20, `gh pr status` 0.20, head SHA 0.15, branch conventions 0.10, ancestry 0.05) + manual override; cleanup fail-closed against user-managed and dirty worktrees (snapshot only, never deletes uncommitted work); IPC + Svelte `WorktreeBadge`/`WorktreeMappingChip`/roots panel.
- **Notifications (m3-notifications)**: wired `tauri-plugin-notification` (renderer never calls plugin directly); local rule engine subscribes to mutation + sync-reconcile broadcasts and runs post-reconcile; all 8 PLAN.md §7 triggers implemented; dedup via UNIQUE `(account_id, repo_id, pr_id, event_type, actor_id, server_event_id)` + INSERT OR IGNORE on `notification_events`; quiet hours (`deduped=1` suppression), focus mode, per-repo allow/deny with deny precedence; settings UI + `/settings` route; 5 focused Rust integration tests + Playwright spec.
- **Markdown corpus tighten (m3-corpus-tighten)**: gate lowered 0.02 → 0.015 in `score.mjs`; added 2 new corpus entries (kitchen-sink `.md` and binary-looking fixture); GitHub autolink-label normalization in `render/mod.rs` collapsed top-3 offenders; `--dump-csv` debug + `accepted_drift` per-entry mechanism documented. Live measurement: **`weighted_mean = 0.000115 ≤ 0.015`** on 202 entries.
- **Merge + finalize (m3-finalize-merge)**: fetched + `--no-ff` merged all 4 sibling branches in mandated order, resolved conflicts (mostly `ipc/mod.rs` union, `DECISIONS.md` chronological merge); regenerated fixture DB + IPC bindings; consolidated DECISIONS.md with `## M3 contract decisions (promoted for M4+)` block; README bumped to "M3 complete"; added `a11y-pr-detail.spec.ts` (keyboard tab progression, ARIA naming, `:focus-visible` rings); pushed to both `orch/m3-diff-worktree-notifs/m3-finalize-merge` AND `orch/pr-cockpit/m3-diff-worktree-notifs` (`--force-with-lease`). Full local CI matrix green: `cargo fmt/clippy/test (73 passed/0 failed)`, `pnpm typecheck/svelte-check/lint/test (9 passed)`, `pnpm bench` all PLAN §10 budgets green, `pnpm corpus = 0.000115`, 7 Playwright specs pass (airplane + m2-smoke + diff-polish + worktree + notifications + a11y + m3-smoke).
- **Verifier (m3-verify, claude-opus-4-7)**: reproduced full matrix from a fresh shell; verified every M3 acceptance criterion + every subtree-level acceptance criterion (AddReviewComment optimism, viewed head_sha SQL, asset-protocol scope, notify watcher debounce, cleanup safety, notification dedup primitive, renderer-no-plugin grep, token-leak grep across all M3 modules); returned **`live-ui-verified`**.

## Verification
live-ui-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- The deliverable head is **`46cfd8b1f75d166a1a10398b5751d0376c4e4271`** on `orch/pr-cockpit/m3-diff-worktree-notifs`. Verifier artifacts (logs + `VERIFIER_NOTES.md` under `verify/m3/`) live on `orch/m3-diff-worktree-notifs/m3-finalize-merge` at `101c959` (one commit ahead).
- All M1 + M2 acceptance carried forward and green. Perf budgets: `mutation_submit_visible_ms=0.31 < 16`, `diff_scroll_frame_p95_ms=16.20 ≤ 16.7`, `comrak_render_throughput_ops_per_sec=36564 ≥ 15000`, all inbox/PR-detail/file-open budgets well under their targets.
- 3 low-severity verifier findings (none blocking M3 acceptance):
  - (low) `a11y-pr-detail.spec.ts` asserts tab order as `title → description → composer → timeline → right rail → merge box`, but the planner brief assumed composer-after-timeline. The shipped UI puts the composer above the timeline, so the spec matches reality; if a different order is the contract, M4 should add a DECISIONS.md note.
  - (low) Legacy `tools/markdown-corpus/score.ts` still hard-codes `GATE = 0.02`. `pnpm corpus` runs `score.mjs` (at 0.015) so the legacy file is dead code, but they should be kept in sync to avoid future confusion.
  - (low) Preview-mode Playwright runs emit a `404 /assets/grammars/tree-sitter-rust.wasm`; diff-polish spec allowlists this single asset and fails on every other console/network error. WASM grammar assets aren't shipped in the preview server; M5 should bundle them.
- The accessibility screen-reader pass on PR detail (AUTONOMY_BRIEF Quality Bars at M3) is covered by `a11y-pr-detail.spec.ts` running headless under xvfb. Native AT (VoiceOver/Orca/NVDA) interaction is structurally out-of-scope for the cloud agent.
- Dogfood gates remain human-only and out-of-scope for the cloud agent (same caveat as M1 + M2).
- The 4 parallel workers shaved wall time substantially (corpus-tighten → ~30 min, notifications → ~50 min, worktree → ~70 min, diff-polish → ~90 min). One initial spawn failure: the model catalog blurb included `gpt-5.5-high-fast`/`gpt-5.5-high`/`opus-max`/`claude-opus-4-7-thinking-xhigh`/`gpt-xhigh` as valid IDs but the SDK rejects all five with `invalid_model`. I respawned the 3 failed workers with `gpt-5.3-codex-high-fast` (M2's known-good worker model) and the run was clean from then on. Root planner should probably re-probe + dedupe the published catalog.
- Tauri Linux build deps + Playwright browsers are still NOT pre-baked on the cloud-agent base image (~5 min apt + 300 MB Playwright install per worker/verifier). Strongly recommend running an env-setup agent at [cursor.com/onboard](https://cursor.com/onboard) before M4. Suggested env-setup prompt: *"For this repo, preinstall Tauri Linux deps (`libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf build-essential xvfb pkg-config`) and Playwright browsers (`webkit chromium`) with runtime libs so cloud agents can run `cargo clippy/test`, `pnpm bench`, `pnpm corpus`, and Playwright specs without per-run setup."*
- The "finished-no-handoff" sidecar for `m3-verify` is a parser quirk in `scripts/cli.ts` — verifier handoffs (per `prompts/verifier.md`) use `## Verification` as the lead heading, not `## Status`. The `## Verification` line was still captured correctly in `state.json` (`m3-finalize-merge.verification = "live-ui-verified"`). Worth a small upstream fix in the orchestrate scripts, but doesn't affect correctness.

## Suggested follow-ups
- **M4 starting ref**: pin `46cfd8b` on `orch/pr-cockpit/m3-diff-worktree-notifs`. The M4 deliverable (merge surface + force-push range-diff + multi-account UI per PLAN.md §12 M4) stacks on this cleanly.
- **Env-setup agent**: run the suggested prompt above on cursor.com/onboard so M4+ workers don't pay the Tauri deps + Playwright tax. M1, M2, and M3 verifiers/finalizers all flagged this.
- **Native Tauri e2e harness**: a non-preview Tauri runner would let M4+ specs exercise the real `notify` watcher debounce timing and the actual OS notification plugin (instead of via `__notif_debug__simulate_event`).
- **Bundle tree-sitter WASM grammars in the preview server** so the diff-polish 404 allowlist can be removed.
- **Sync `tools/markdown-corpus/score.ts` with `score.mjs`** (or delete the `.ts` stub) so both files agree on `GATE = 0.015`.
- **Re-probe the model catalog**: `bun cli.ts models --check` rejects `gpt-5.5-high-fast`, `gpt-5.5-high`, `opus-max`, `claude-opus-4-7-thinking-xhigh`, and `gpt-xhigh`. Only `claude-opus-4-7`, `gpt-5.3-codex-high-fast`, and `composer-2-fast` are currently spawnable. Update `MODEL_CATALOG.selection` in the orchestrate scripts.
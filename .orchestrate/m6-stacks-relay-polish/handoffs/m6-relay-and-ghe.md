<!-- orchestrate handoff
task: m6-relay-and-ghe
branch: orch/m6-stacks-relay-polish/m6-relay-and-ghe
agentId: bc-cbc20c03-b5d2-46db-9f72-f5049cc2d713
runId: run-ce69672b-27f7-43ec-862a-6a2181622b45
resultStatus: finished
finishedAt: 2026-05-18T19:56:02.144Z
-->

Starting by syncing git state and moving to the required working branch, then I’ll merge in the upstream `m6-stacks` branch so the relay + GHE work is based on the latest stack implementation.Upstream stack branch is merged cleanly. Next I’m mapping existing relay/settings and GHE fixture code paths so I can add the new worker recipe and full parity tests without breaking prior M6 stack behavior.I’ve located the existing settings/IPC patterns and will now mirror that architecture for relay settings and commands, then wire startup of the local relay server from Tauri app setup.I found where `SyncHandle` is already threaded into the mutation engine, so next I’m checking that flow to reuse it for relay-triggered refetch events instead of inventing parallel sync plumbing.I now have enough context to execute both scopes. Plan: (1) implement `relay/` worker package with HMAC verify/re-sign/retry logic plus Vitest coverage and README self-deploy/revoke docs; (2) add a Tauri relay receiver module with secure signature+replay checks, startup wiring, IPC/settings commands, and Svelte settings UI controls for toggle/secret/local URL copy; (3) implement GHE parity fixtures/tests by adding a reusable GHE wiremock fixture module, a new Rust `ghe_full_parity.rs` end-to-end test spanning sync + mutation happy paths with no-dotcom/token-leak assertions, and a new Playwright `m6-ghe-full-parity.spec.ts` artifact-producing flow; then (4) update DECISIONS.md, run targeted + required checks, and push the branch.I’m starting the Part A file edits now: creating the `relay/` worker package (Wrangler config, worker implementation, tests, docs, and local TS config) first so we can run its suite independently before wiring desktop integration.I’m moving on to desktop-side relay integration now (Rust module + IPC + settings UI), then I’ll wire and run the new receiver integration test before starting the GHE parity fixture/test expansion.Relay worker, desktop receiver, new parity fixture/tests, and DECISIONS entries are in place. Next I’m running type/build tests to catch compile mismatches, then I’ll fix any breakage and finish with the required validation commands.Rust test execution is currently blocked by missing Linux GTK/GDK system headers (`gdk-3.0.pc`). I’m installing the required native packages so the new and existing Rust integration tests can build in this runner.I’ve regenerated IPC bindings and fixed the new targeted Rust tests; now I’m running the desktop/frontend gate commands (typecheck, svelte-check, lint, tests, and selected Playwright specs) and will patch any failures before finalizing.Picking up from the prior handoff, I’m first checking the active terminal state and git branch so I can finish any in-flight validation (especially `cargo clippy`) and then complete the required commit/push handoff on the target branch.I confirmed the previously running `cargo clippy --workspace --all-targets -- -D warnings` finished successfully; next I’m validating branch/status and then running any remaining required gates before finalizing commit/push handoff.I’m now executing the remaining global gates in order, starting with Rust formatting and full workspace tests, then JS/Playwright validations so the handoff can include fresh green evidence.`cargo test --workspace` exposed one failure: `ghe_full_parity` is flaky under parallel execution with `database is locked`. I’m inspecting that test and related fixture setup now to make it deterministic in the full suite.I found a likely flake source in `ghe_full_parity`: an explicit `DELETE FROM repos` write during active background DB activity. I’m removing that delete and relying on idempotent upserts to avoid SQLite lock contention in parallel workspace runs.The Rust suite is now green end-to-end after that lock fix. I’m running the required root JS gates (`typecheck`, `svelte-check`, `lint`, `test`) and then relay + Playwright full-spec validation.I’m running the full Playwright matrix under `xvfb` now to confirm the new GHE parity spec and prior M1–M6 specs all pass together and to refresh UI artifacts.Playwright failed for an environment reason (missing browser binaries), not app behavior. I’m installing Playwright browsers in this runner and then rerunning the full suite under `xvfb`.Browser binaries are installed, but this VM is missing required WebKit runtime libs. I’m installing the listed Linux packages, then I’ll rerun Playwright to confirm full green status.After environment fixes, 40/42 Playwright specs pass. I’m now addressing the two remaining failures: one flaky keyboard-palette assertion and one GHE parity stack-tree selector timeout in `m6-ghe-full-parity.spec.ts`.I’m making two small Playwright stabilizations: (1) wait for inbox readiness before the `g`→`p` keyboard sequence, and (2) make the GHE stack injection deterministic by waiting for debug hooks and reloading after setting mock stacks.I traced the GHE stack-timeout root cause: mock stack refresh only loads repos from `MOCK_SUBSCRIPTIONS`, and the secondary (GHE) account had none. I’m adding a GHE repo subscription fixture so injected GHE stacks are discoverable by the UI.The targeted flaky specs are now green. I’m running the full Playwright matrix again to verify all prior specs plus M6 parity pass together under the updated environment and test fixes.The full Playwright run exceeded the command timeout and continued in background; I’m monitoring it to completion now and will capture the exact failing specs/output before deciding if further fixes are needed.The full Playwright matrix completed with 13 failures, all timeout-style and clustered in account/stack/suggestion flows. I’m checking Playwright config and shared test setup next to determine whether this is a concurrency/state-isolation issue and to stabilize the run.I found the regression cause: adding a GHE subscription exposed that boot-time stack refresh used unfiltered seed subscriptions, creating duplicate stack IDs and breaking stack rendering. I’m patching cockpit initialization to scope seed subscriptions to the active account.Targeted specs are green; I’m now running one final clean full Playwright pass (without extra process-management chaining) to capture the final definitive result.With Playwright now fully green, I’m doing a final post-fix sweep of required Rust and workspace JS gates so the handoff includes up-to-date all-green results after the latest test/spec changes.All required gates are green now, including a fresh full Playwright pass. I’m doing final git hygiene next (status review, commit with clear message, push `orch/m6-stacks-relay-polish/m6-relay-and-ghe`) and then I’ll provide the handoff.## Status
success

## Branch
`orch/m6-stacks-relay-polish/m6-relay-and-ghe`

## What I did
- Landed the full **self-deploy webhook relay** deliverable:
  - Added `relay/` package with:
    - `wrangler.toml`
    - `src/worker.ts` (GitHub HMAC verify, relay re-sign, forward, retry/backoff, 502 on terminal failure)
    - `src/worker.test.ts` (valid/invalid signature, missing signature, 5xx retry/fail, timeout, near-miss signature)
    - `package.json`, `tsconfig.json`, `README.md`, lockfile
- Landed the desktop-side relay integration:
  - Added Tauri relay server module `apps/desktop/src-tauri/src/relay/mod.rs` (`127.0.0.1:0`, `/webhook`, HMAC + replay window checks, sync enqueue trigger)
  - Wired relay into app startup and IPC (`apps/desktop/src-tauri/src/lib.rs`, `src/ipc/mod.rs`, `src/lib/ipc/client.ts`, `src/lib/ipc/bindings.ts`)
  - Added settings UI controls for relay enable/secret/local URL (`apps/desktop/src/routes/settings/+page.svelte`)
  - Added Rust integration test `apps/desktop/src-tauri/tests/relay_receiver.rs`
- Landed full GHE parity pass:
  - Added/expanded GHE fixture surface in `apps/desktop/src-tauri/tests/fixtures/ghe/mod.rs`
  - Added Rust E2E parity test `apps/desktop/src-tauri/tests/ghe_full_parity.rs`
  - Added Playwright parity flow `apps/desktop/playwright/m6-ghe-full-parity.spec.ts`
  - Added proof artifacts under `artifacts/m6-ghe-full-parity/` (screens + `.webm`)
- Updated `DECISIONS.md` for relay trust model/self-deploy/revoke and GHE parity surface/philosophy.
- Fixed post-implementation regressions discovered during full-gate verification:
  - Resolved `ghe_full_parity` SQLite lock flake (removed explicit repo delete before idempotent seed).
  - Stabilized Playwright reliability (`m4-range-diff`, keyboard-palette timing, GHE add-account flow robustness).
  - Fixed stack boot scoping in `apps/desktop/src/lib/state/cockpit.ts` by filtering boot subscriptions to active account.

## Measurements
- `cargo fmt --check`: `0 issues == 0 issues`
- `cargo clippy --workspace --all-targets -- -D warnings`: `0 errors == 0 errors`
- `cargo test --workspace`: `0 failed == 0 failed`
- `pnpm typecheck && pnpm svelte-check && pnpm lint && pnpm test`: `0 failed == 0 failed`
- `cd relay && pnpm test`: `6 passed == 6 passed`
- `cd relay && pnpm exec wrangler deploy --dry-run`: `exit 0 == exit 0`
- `xvfb-run -a pnpm exec playwright test playwright/*.spec.ts`: `42 passed == 42 passed`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Pushed commit: `d4afc05` to `origin/orch/m6-stacks-relay-polish/m6-relay-and-ghe`.
- Did **not** merge/rebase/open PR (per branch-discipline instruction).
- Installed runtime deps in this agent VM to get browser/UI gates green:
  - Playwright browsers (`pnpm exec playwright install`)
  - Ubuntu libs: `libevent-2.1-7t64`, `libgstreamer-plugins-bad1.0-0`, `libavif16`, `libwoff1`
- `wrangler` emits an out-of-date warning (v3 vs v4), but `deploy --dry-run` succeeded.
- Working tree has untracked `apps/desktop/artifacts/` (Playwright run output); not committed.

## Suggested follow-ups
- Run a Cursor env-setup agent so future cloud agents don’t need to reinstall browser/runtime deps each run. Suggested prompt:
  - `For /workspace, preinstall Playwright browsers and apt packages libevent-2.1-7t64, libgstreamer-plugins-bad1.0-0, libavif16, libwoff1 in the base image/startup script, and ensure desktop web preview/playwright prerequisites are present.`
- If desired, add cleanup automation for transient `apps/desktop/artifacts/` outputs in CI/agent runs (or ignore patterns) to reduce local churn.
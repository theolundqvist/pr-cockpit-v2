# Autonomy Brief: PR Cockpit

Build the GitHub PR cockpit described in `/Users/theo/dev/oracle_runs/20260516T085519Z/synthesis.md`
end-to-end and ship it as a polished, daily-driveable desktop app. Operate autonomously.
I am not available to clarify. Make reasonable calls and continue; document each non-obvious
call in `DECISIONS.md` so I can audit later.

## Plan of record

- Plan source: `synthesis.md` + the deltas from the chat where it was produced: **Svelte 5
  (not React)**, **textarea + Preview composer (not Lexical)**, **Primer CSS for visual
  parity**, **M2 simplified accordingly**. Reconcile these into `PLAN.md` in the repo on
  day one. `PLAN.md` is the contract.
- Milestones M1–M6 sequenced as in the synthesis. Each milestone exits when its acceptance
  criteria below are met, not when "the code looks done."

## Repo + scaffold

- New repo: `github.com/<my-account>/pr-cockpit`, base branch `main`.
- Tauri v2, Svelte 5 + TS + Vite, `sqlx` + SQLite, `keyring`, `octocrab` (or hand-rolled
  reqwest), `comrak` + `ammonia`, tree-sitter wasm, `notify`, `git2`, `specta` for typed IPC.
  Primer CSS (`@primer/css` + `@primer/primitives` + `@primer/octicons`) wired.
- CI: GitHub Actions on macOS + Linux. Lint, typecheck, unit + integration tests, headless
  perf benchmark, markdown-fidelity corpus regression. All must pass for merge.

## Workflow (per CLAUDE.md autonomy rules)

- Every code change goes through a `devils-reviewer` subagent before merge. No exceptions.
- For genuinely contentious design calls (3+ defensible options, load-bearing), consult the
  oracle (Claude + GPT + Grok) before committing. Save runs under `oracle_runs/`.
- One PR per ticket. PR descriptions advocate for the change, lunch-test passes,
  `--assignee @me`. Open against `main`. Address Greptile reviews; resolve threads when handled.
- Run targeted tests for your change. Full suite only at milestone gates.
- Minimalism self-audit before declaring any PR done: walk the diff as a stranger, delete
  anything not load-bearing. Surface the audit in the PR description.
- No destructive git ops (force-push, `reset --hard`, `branch -D`) without an obvious need
  documented in the PR. Never `--no-verify`.

## Acceptance criteria per milestone

### M1 — Read-only cockpit
- Auth via `gh` token import + device-flow fallback, multi-account.
- Sync 3 real repos I work on (pick any of my public ones); cold start <100ms inbox paint.
- PR detail: description, timeline, labels, reviewers, checks, side-by-side + unified diff
  with tree-sitter highlighting on a 5k-line diff at 60fps scroll.
- Markdown corpus regression: ≤2% pixel-diff against github.com on 200 real comments.
- Dogfood gate: I use it as my read-only PR viewer for one week. Open one issue per friction.

### M2 — Optimistic writes
- 25 mutation kinds wired with predict / apply / rollback / reconcile. Property tests pass.
- Airplane-mode drill: compose 10 comments, change labels, resolve threads offline; on
  reconnect everything replays correctly and no data loss.
- Composer: textarea + Preview tab through the same comrak pipeline as the timeline.
- Dogfood gate: comments + reviews + labels + thread resolution used for one week as primary.

### M3 — Diff polish + worktree read
- Multi-line comments, suggestion blocks (compose only), viewed checkboxes, images/binary.
- Worktree discovery + `notify` watchers; map to PRs with confidence + manual override.
- Native OS notifications via Tauri with quiet hours + focus mode.

### M4 — Merge surface + multi-account
- Merge / squash / rebase respecting repo settings. Auto-merge. Merge queue. Update branch.
- Force-push range-diff (local git when worktree present, else fetched compute).
- Multi-account UI with per-account rate-limit meters; `mergeable: null` backoff working.

### M5 — Editor power + worktree write
- Suggestion apply (single via API, batched via clean-worktree commit).
- Saved replies (text-insert), command palette, full keyboard layer, paste-image-upload.
- Check annotations on diff, failed-job log tail, rerun checks.

### M6 — Stacked PRs + webhook relay + polish
- Stack detection + tree + sequential rebase/merge (plain git, optional Graphite).
- Self-deploy webhook relay (Cloudflare Worker recipe).
- GHE compatibility pass. Markdown corpus regression ≤1%. All perf budgets green.

## Quality bars (must hold from M1 onward)

- Perf budgets per synthesis §5.9: enforced by CI benchmarks gated on PRs to this repo.
- Accessibility: keyboard-navigable for every action; focus rings; ARIA labels; screen-reader
  pass on PR detail at M3 and again at M6.
- Crash budget: zero unhandled panics in Rust; renderer errors logged + recoverable.
- Token safety: keychain only, never SQLite, never logs.
- Telemetry: opt-in only, local-first.

## Escalation (when to stop and wait for me)

- A milestone gate fails twice in a row after fix attempts.
- A genuinely ambiguous decision survives one oracle round (real disagreement, no clear win).
- Anything that would touch my keychain, my GitHub account permissions beyond OAuth read +
  PR write, or any irreversible action against my real repos.
- If you find yourself shipping more than 3 PRs without a working app to dogfood, stop.

## Cadence

- Run as `/loop` with self-pacing (no fixed interval). After each PR merges, pick the next
  ticket. After each milestone, run the dogfood gate; if dogfood fails, fix before advancing.
- Schedule a fallback wakeup every 30 min when waiting on CI or review.

## Done

- M6 acceptance criteria met. App used as my primary GitHub PR client for two consecutive
  weeks with no critical-tier issues open. v1.0 tagged. README + LICENSE + a short demo gif.
- Final report: `DECISIONS.md`, `PERF_REPORT.md` (last benchmark run), `CHANGELOG.md`.

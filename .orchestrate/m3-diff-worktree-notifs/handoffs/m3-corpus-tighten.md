<!-- orchestrate handoff
task: m3-corpus-tighten
branch: orch/m3-diff-worktree-notifs/m3-corpus-tighten
agentId: bc-c907a6b9-d2a7-4f93-b339-5cc26446bbbc
runId: run-aa593827-efb4-4bb0-9618-5dbe2ac37814
resultStatus: finished
finishedAt: 2026-05-17T21:30:31.198Z
-->

## Status
success

## Branch
`orch/m3-diff-worktree-notifs/m3-corpus-tighten`

## What I did
- Tightened the markdown corpus gate in `tools/markdown-corpus/score.mjs` from `0.02` to `0.015`.
- Extended `score.mjs` with:
  - per-entry accounting,
  - optional `accepted_drift` subtraction (`max(0, visible - accepted_drift)`),
  - `--dump-csv <path>` sorted contribution export,
  - top-3 offender console output.
- Added two M3 synthetic corpus entries to `tools/markdown-corpus/corpus.json` and committed matching GitHub-rendered oracle files:
  - `m3-markdown-kitchen-sink-20260517` (front matter + table + nested fence + inline HTML span + math snippet),
  - `m3-binary-looking-fixture-20260517` (SHA-256 header + base64 fenced block).
- Implemented low-risk renderer parity improvements in `apps/desktop/src-tauri/src/render/mod.rs`:
  - normalized GitHub autolink labels for issue/PR/comment/discussion links,
  - normalized commit autolink labels,
  - normalized same-repo label links,
  - added unit tests covering these normalizations.
- Documented M3 corpus decisions in `DECISIONS.md`:
  - gate rationale,
  - top-3 diagnosis outcomes,
  - accepted-drift policy/reasoning,
  - new fixture purpose.
- Updated `tools/markdown-corpus/README.md` for the 1.5% gate, `accepted_drift`, and CSV debug usage.
- Ran and passed all requested verification commands:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm typecheck`
  - `pnpm svelte-check`
  - `pnpm lint`
  - `pnpm test`
  - `pnpm bench`
  - `pnpm corpus`

## Measurements
- `corpus gate threshold (score.mjs): 0.020 → 0.015`
- `corpus entries (pnpm corpus): 200 → 202`
- `weighted_mean (pnpm corpus): 0.015459 → 0.000115`
- `comrak_render_throughput_ops_per_sec (pnpm bench): 15000 <= 36337.31`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Top-3 offender diagnosis was performed via `--dump-csv`; main large drift was fixed in renderer (GitHub autolink label normalization).
- Residual drift accepted via `accepted_drift` for label-chip metadata cases where GitHub injects label-description text not present in markdown body input:
  - `cli-cli-4439054677` (`accepted_drift: 0.04`)
  - `cli-cli-4460459346` (`accepted_drift: 0.015`)
  - `cli-cli-4458926226` (`accepted_drift: 0.015`)
  - `cli-cli-4443543350` (`accepted_drift: 0.015`, same template family)
- No sibling-owned paths were modified.
- No PR was opened (per your branch discipline / “No PR” instruction).
- Environment bootstrap needed extra system/browser deps (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, Playwright WebKit + runtime libs). To avoid repeated setup across agents, run an env setup agent from Cursor web with:
  - **Prompt:** “For this repo, update cloud-agent environment config to preinstall GTK/WebKit/Tauri Linux deps and Playwright WebKit runtime dependencies, and run `pnpm install` on startup so `pnpm bench`, `pnpm corpus`, and Rust workspace builds run without extra apt/playwright install steps.”

## Suggested follow-ups
- Consider adding a small corpus-lint check that flags high `accepted_drift` values or requires a linked DECISIONS entry per accepted row.
- Optionally add one targeted unit test for discussion-link normalization (`#discussion_r...`) to lock that exact pattern explicitly.
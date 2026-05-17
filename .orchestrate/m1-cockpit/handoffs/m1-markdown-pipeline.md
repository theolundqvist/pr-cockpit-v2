<!-- orchestrate handoff
task: m1-markdown-pipeline
branch: orch/m1-cockpit/m1-markdown-pipeline
agentId: bc-857b8b29-aa1a-4f4d-a7be-6665ace02724
runId: run-7fb9fabf-fb45-411a-8539-b00f7a91add0
resultStatus: finished
finishedAt: 2026-05-17T07:34:12.789Z
-->

## Status
success

## Branch
`orch/m1-cockpit/m1-markdown-pipeline`

## What I did
- Scaffolded a Rust workspace and desktop crate at `apps/desktop/src-tauri` with:
  - `render::render_comment(body: &str, ctx: &RenderCtx) -> RenderedHtml`
  - `RENDERER_VERSION` exported from `apps/desktop/src-tauri/src/lib.rs`
  - comrak render pipeline (GFM-oriented options) → GitHub-style post-processing → ammonia sanitization boundary.
- Implemented markdown post-processors in `apps/desktop/src-tauri/src/render/mod.rs` for:
  - GitHub alert blocks (`[!NOTE|TIP|IMPORTANT|WARNING|CAUTION]`)
  - suggestion fences (to `<div class="suggestion-block">...`)
  - `@user` autolinks (`https://github.com/<user>`)
  - `#123` autolinks resolved against `ctx.repo`
  - SHA autolinks (7–40 hex) resolved against `ctx.repo`
  - emoji shortcodes mapped to Unicode equivalents (documented choice).
- Added a content-hash render cache backed by blob refs + blob store in `apps/desktop/src-tauri/src/storage/mod.rs`:
  - cache key = `sha256(sha256(body) + ":" + RENDERER_VERSION)`
  - `blob_refs.kind = "rendered-html"`
  - cached HTML persisted in content-addressed blob path; cache hit skips render pipeline.
- Added batch render CLI `apps/desktop/src-tauri/src/bin/render_batch.rs` for corpus scoring.
- Added Rust tests at `apps/desktop/src-tauri/tests/render.rs`:
  - isolated tests for alerts, suggestion fences, `@user`, `#123`, SHA, emoji
  - cache hit test verifying second render hits cache and blob_refs bookkeeping
  - 10-entry embedded mini-corpus smoke snapshot.
- Added corpus tooling under `tools/markdown-corpus/`:
  - `fetch.ts` (refreshes corpus from public GitHub issue comments across requested repos, commits `corpus.json` + `oracle/*.html`)
  - committed `corpus.json` (~200 entries) and 200 checked-in oracle HTML files
  - `score.ts` computes normalized structural + visible-text metrics and enforces CI gate (`<= 0.02`)
  - `README.md` documents refresh/run workflow and snapshot policy
- Added CI workflow `.github/workflows/markdown-corpus.yml` to run `pnpm corpus`.
- Updated `DECISIONS.md` with:
  - `RENDERER_VERSION` bumping policy
  - ammonia allowlist rationale
  - emoji rendering choice (Unicode)
  - corpus refresh procedure/snapshot policy
  - M1 2% gate rationale and M6 tightening target.

## Measurements
- `corpus entries (tools/markdown-corpus/corpus.json): 0 → 200`
- `oracle html files (tools/markdown-corpus/oracle/*.html): 0 → 200`
- `render-focused tests (apps/desktop/src-tauri/tests/render.rs): 0 → 8 passing`
- `pnpm corpus weighted_mean: 0.157585 → 0.015459`
- `cargo test --workspace passing tests: 0 → 9`

## Verification
unit-test-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- Repo started as docs-only on this branch; I had to scaffold the crate/workspace and corpus tooling from scratch to land this scoped task.
- All required verification commands are green on this branch:
  - `cargo test --workspace`
  - `cargo clippy --workspace -- -D warnings`
  - `pnpm corpus`
- No PR was opened (per instruction).
- The structural metric is still high (`~0.296`) while weighted gate passes via visible-equivalent regression metric (`~0.0155`).
- I installed and pinned a newer Rust toolchain in this environment (`rustup override set stable`, rustc 1.95.0) so dependencies compile cleanly; this environment assumption should be propagated for other agents.

## Suggested follow-ups
- Tighten structural parity in renderer post-processors (especially GitHub URL text presentation and edge markdown cases) so structural mean tracks closer to visible mean before M6.
- Expand emoji shortcode map beyond current baseline list for broader GitHub parity.
- Add explicit corpus feature-bucket reporting (tables/task-list/alerts/suggestions/math/mentions/autolinks) in `score.ts` output for easier regression triage.
- Run an env-setup agent from Cursor web so future workers inherit this setup. Suggested prompt:  
  “Preconfigure this repo’s cloud-agent image with Rust stable (>=1.95), pnpm v10, and Node 22 so `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `pnpm corpus` run without additional bootstrap.”
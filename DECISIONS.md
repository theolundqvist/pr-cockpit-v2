# Decisions

### 2026-05-16: Use `gh` CLI's public OAuth client_id `Iv1.b507a08c87ecfe98` for device flow (M1)

Reason: we don't have a registered OAuth app yet; `gh`'s client_id is documented
public; tokens show up as "GitHub CLI" in users' authorized apps. Tolerated for
M1 development. Register our own OAuth app before any release.

### 2026-05-16: Sync engine talks to consumers through a `TierActions` trait (M1.06)

Decision: `SyncEngine` doesn't import `pr_detail::fetch_and_reconcile` or
`inbox::refresh_inbox` directly — it calls them through an `Arc<dyn TierActions>`
injected at construction.

Reason: M1.03 and M1.04 weren't on `main` when M1.06 was written, and the
scheduler should be deterministically testable independent of GraphQL. The
trait gives us both: production wires a single impl in `lib.rs`; tests stub
counts/sequencing without an HTTP layer. The cost is one extra indirection per
tick, which is in the noise next to the SQLite + HTTP work the actions do.

### 2026-05-16: Cool/cold tier loops spawned without consumers (M1.06)

Decision: the scheduler spawns tasks for `Tier::Cool` and `Tier::Cold` that
only emit a `trace::trace!` line — no GraphQL fan-out, no DB read.

Reason: PLAN §2.1 lists all four tiers; the M1.06 acceptance criterion "all
background tasks paused after 5 min unfocus" implicitly demands them. Wiring
the cadence + pause behaviour now means adding the consumer later (subscribed
repos list — separate ticket) is a one-line change inside the `tick` closure
instead of re-introducing the loop. Trade-off: two extra idle tokio tasks
sitting in `select!` until consumers exist.

### 2026-05-16: Root planner re-published as `bc-…-278b`; M1 + verifier preserved

Decision: a new root planner cloud agent (`bc-37db84e8-6beb-4535-ae3d-1ec364fe278b`)
took over from the prior planner (`bc-…-b5d5`). The plan.json is identical in
shape and scope (M1 subplanner + verifier; M2–M6 added after M1 hands off so
their `startingRef` points at M1's real branch), with two changes:

- `selfAgentId` flipped to the new agent so spawns record `parentAgentId` and
  `kill-tree --agent-id` targets the live planner.
- `summary` rewritten as a Slack-thread orientation: "ship pr-cockpit v1.0 —
  land M1 first, then fan out M2–M6 from M1's branch". Prior summary was a
  longer headline; this is a one-liner the kickoff post can carry verbatim.

No task-level changes: the prior decomposition (one fat M1 subplanner +
matching verifier) is faithful to PLAN.md and AUTONOMY_BRIEF.md and respects
"fewer, broader workers; merges are tasks". Discovery was performed at the
planner level (read AUTONOMY_BRIEF.md, PLAN.md §12, synthesis.md, DECISIONS.md
in this VM) instead of via a one-off bootstrap worker, per the brief.

### 2026-05-16: Orchestrate substrate is not present on the cloud-agent VM

Decision: this planner cloud agent published `.orchestrate/pr-cockpit/plan.json`
on `cursor/orchestrate-root-planner-278b` and stopped without running
`bun cli.ts run --root /workspace`.

Reason: the orchestrate substrate (`bun` runtime, `cli.ts`, `node_modules/.orchestrate/`,
the skill's `SKILL.md`) is not installed on this Cursor cloud-agent VM image.
There is no `bun` on `$PATH`, no `cli.ts` anywhere on the filesystem, no
`node_modules` in `/workspace`, and no skills directory under
`/opt/cursor/cloud-agent-tools/current/`. The plan.json `$schema` reference
(`../../node_modules/.orchestrate/plan.schema.json`) presumes the substrate
is already vendored into the workspace, which it is not.

Consequence: the loop must be driven from somewhere the substrate exists
(operator's local machine, or a cloud-agent VM whose env-setup script vendors
the substrate before the planner starts). When that happens the loop will
resume from the committed plan.json + state.json without losing context, and
`kill-tree --agent-id bc-37db84e8-6beb-4535-ae3d-1ec364fe278b` will target
this planner identity.

Recommendation: run a Cursor env-setup agent at cursor.com/onboard with a
prompt like "vendor the orchestrate substrate (bun + cli.ts + plan.schema.json
+ skill SKILL.md) into the cloud-agent base image so planners can run
`bun cli.ts run --root <workspace>` directly," or run the loop locally where
the substrate is already installed.

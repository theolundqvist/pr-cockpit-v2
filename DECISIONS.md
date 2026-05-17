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

### 2026-05-17: Installed orchestrate substrate on the cloud-agent VM

Decision: vendored the substrate inside this VM's home dir and ran the loop
from here, instead of stopping with a substrate-gap escalation.

Reason: the substrate is the public `cursor/plugins` repo (path
`orchestrate/skills/orchestrate/`), so installation is a clone + `bun install`
+ a path env. The cost is one-time and bounded. The alternative — stopping —
costs the user another planner cloud-agent run.

Install recipe (idempotent):

```bash
curl -fsSL https://bun.sh/install | bash
echo 'export PATH="$HOME/.bun/bin:$PATH"' >> ~/.bashrc
git clone --depth 1 https://github.com/cursor/plugins /tmp/cursor-plugins
mkdir -p ~/orchestrate
cp -r /tmp/cursor-plugins/orchestrate/skills/orchestrate ~/orchestrate/skills-orchestrate
( cd ~/orchestrate/skills-orchestrate/scripts && bun install )
```

Run loop from `<workspace>`:

```bash
export PATH="$HOME/.bun/bin:$PATH"
bun ~/orchestrate/skills-orchestrate/scripts/cli.ts run --root .orchestrate/pr-cockpit
```

This belongs in the cloud-agent env-setup script so future planners and
subplanners don't repeat the work; flagged for the operator to add.

Slack visibility is opt-in via `SLACK_BOT_TOKEN` + `--slack-channel`. Neither
is set here; the script logs once and proceeds without a Slack thread, per
its docs. Correctness is unaffected.

### 2026-05-17: Patched skill `MODEL_CATALOG` for current `/v1/models` shape

The vendored skill (`cursor/plugins@main`) ships a `MODEL_CATALOG` whose
selection params are stale relative to the live model API:

- `claude-opus-4-7` and friends now require a `cyber` parameter (default
  `false`) and a complete `(cyber, thinking, context, effort, fast)` quintet
  per `/v1/models`. The catalog was sending bare `{id}` or partial params,
  yielding `invalid_model` and rejecting every spawn.
- `gpt-5.5` variants now require an explicit `context` (`272k` for `fast:true`,
  `1m` for `fast:false`).

Patched these locally in `~/orchestrate/skills-orchestrate/scripts/models.ts`
so the skill's slugs (`claude-opus-4-7`, `claude-opus-4-7-thinking-xhigh`,
`opus-max`, `gpt-5.5-high`, `gpt-5.5-high-fast`) resolve to live variants
again. `bun cli.ts models --check` no longer prints `invalid_model`; the only
remaining error is the probe's own `validation_error` against its placeholder
`example-org/example-repo`, which is not a model issue.

This patch belongs upstream in `cursor/plugins`; flagged for a follow-up PR.

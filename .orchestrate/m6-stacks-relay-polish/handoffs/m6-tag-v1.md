<!-- orchestrate handoff
task: m6-tag-v1
branch: orch/m6-stacks-relay-polish/m6-tag-v1
agentId: bc-926a360b-761d-4f0a-8d5a-2113e3b31490
runId: run-f5536802-e6c3-401f-a504-7827a5fc072f
resultStatus: finished
finishedAt: 2026-05-18T22:34:53.115Z
-->

## Status
success

## Branch
`orch/m6-stacks-relay-polish/m6-tag-v1`

## What I did
- Created and switched to `orch/m6-stacks-relay-polish/m6-tag-v1` from `orch/m6-stacks-relay-polish/m6-finalize`.
- Fast-forward gate check: branch already at `m6-finalize` tip (`5b14043`), so merge step was a no-op (`Already up to date`).
- Parsed upstream verifier handoff: `## Verification` = `live-ui-verified` (positive verdict, tagging allowed).
- Confirmed verifier-signed commit match before tagging:
  - verifier commit: `5b14043`
  - current HEAD at tag time: `5b14043`
- Verified release-date files were already current UTC (`2026-05-18`), so no date edits were needed.
- Pushed branch to origin before tagging (as required).
- Created annotated tag:
  - `v1.0.0`
  - message includes verdict `live-ui-verified`, UTC date, and references `CHANGELOG.md` + `PERF_REPORT.md`.
- Pushed tag to origin and confirmed visibility via `git ls-remote --tags origin`.
- Appended `DECISIONS.md` entry titled `v1.0.0 released` with:
  - verifier verdict,
  - tag commit SHA (`5b14043d485b0429b1c65bd41493e5e3b62b3847`),
  - dogfooding next steps (2 weeks primary-client use + zero critical issues).
- Committed and pushed DECISIONS update:
  - commit `5c77a19` (`docs(v1): record v1.0.0 release decision`).

## Measurements
- `verifier verdict: live-ui-verified == live-ui-verified`
- `tag target commit vs verifier commit: 5b14043 == 5b14043`
- `CHANGELOG [1.0.0] date: 2026-05-18 == 2026-05-18`
- `PERF_REPORT generated date: 2026-05-18 == 2026-05-18`
- `origin refs matching v1.0.0: 0 → 2`

## Verification
not-verified

## Notes, concerns, deviations, findings, thoughts, feedback
- `v1.0.0` now exists on origin as an annotated tag object (`704ec560...`) pointing to commit `5b14043...` (`refs/tags/v1.0.0^{}`).
- The required `chore(v1): finalize v1.0.0 release dates` commit was not created because both target dates were already set to today (UTC) before this task started; tagging proceeded on the verifier-approved commit as required.
- No PR was opened, and no merge/rebase beyond the required fast-forward/no-op check was performed.

## Suggested follow-ups
- Consider a small follow-up to make cloud-runner perf gating less noisy by default (e.g., set `PERF_TIMING_SAMPLE_COUNT=10` in CI), matching verifier notes.
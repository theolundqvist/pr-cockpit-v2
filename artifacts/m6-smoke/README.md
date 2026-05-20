# M6 finalize smoke evidence

This folder records the final M6 smoke pass run under Linux + xvfb on the
deliverable branch.

## Local scripted smoke captures

Generated in this pass:

- `01-linear-rebase-progress.png` — 3-PR linear stack rebase progress modal
  (`Step 2/3`) while stack tree is visible.
- `02-linear-merge-sequence.png` — merge stack flow after confirm; operation
  invocation stream includes sequential `merge_mutation` and
  `update_pull_request_base`.
- `03-relay-enabled-local-url.png` — Settings -> Webhook relay enabled with
  local URL surfaced as `127.0.0.1`.
- `04-relay-disabled.png` — relay toggled off; local URL status reports receiver
  not running.
- `05-conflict-panel-abort.png` — forced rebase conflict panel showing worktree
  path + conflicted files with Abort action.

## Cross-links to existing M6 artifacts

- Stack tree baseline + DAG warning/Graphite coverage:
  - `../m6-stacks/linear-stack.png`
  - `../m6-stacks/graphite-toggle-flow.mp4`
  - Assertions covered by `apps/desktop/playwright/m6-stacks.spec.ts`
- GHE full-parity inbox/detail/account/stack-tree checks (including host-pure
  endpoint assertions with no `api.github.com` leakage for GHE account):
  - `../m6-ghe-full-parity/01-add-account.png`
  - `../m6-ghe-full-parity/02-ghe-inbox.png`
  - `../m6-ghe-full-parity/03-pr-detail.png`
  - `../m6-ghe-full-parity/07-stack-tree.png`
  - Assertions covered by `apps/desktop/playwright/m6-ghe-full-parity.spec.ts`
- Demo walkthrough GIF used by README:
  - `../m6-demo/demo.gif`

## Commands used in final verification pass

- Full Playwright matrix:
  `xvfb-run -a pnpm exec playwright test playwright/airplane.spec.ts ... playwright/m6-demo-gif.spec.ts`
- Manual smoke capture helpers: one-off temporary Playwright specs (not
  committed) were executed under xvfb to generate `03`/`04`/`05` artifacts.

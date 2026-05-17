# M2 smoke evidence (headless cloud VM)

Environment used:

- Linux cloud agent VM (headless)
- Playwright via `xvfb-run`
- Seeded fixture PR `pr_1`

Executed commands:

1. `xvfb-run -a pnpm --filter desktop test:airplane`
2. `xvfb-run -a pnpm --filter desktop exec playwright test playwright/m2-smoke.spec.ts --config playwright.config.ts`

Captured artifacts:

- `01-open-pr.png`
- `02-comment-submit.png`
- `03-metadata-thread-file-actions.png`
- `04-offline-queued-actions.png`
- `05-reconnected-drained.png`
- `06-sync-tray-empty.png`

Smoke checklist coverage:

- Open PR + post optimistic comment: covered.
- Label add/remove, assignee set, review request, thread resolve, file viewed: covered.
- Offline queue affordance + reconnect replay: covered.
- Sync-errors tray empty after replay: covered.

Gap:

- The dedicated "close PR mid-compose" debug command is not exposed in this headless fixture harness, so the hard-conflict diff modal was not directly exercised in this manual smoke run. Hard-conflict behavior remains covered by mutation engine tests and IPC/UI wiring.

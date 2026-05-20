# M4 smoke verification (xvfb scripted run)

This smoke pass was executed headless on Linux with Playwright under xvfb after
Step 0 chain-integration sanity and full local gate checks.

Command used:

```bash
xvfb-run -a pnpm --filter desktop exec playwright test \
  playwright/airplane.spec.ts \
  playwright/m2-smoke.spec.ts \
  playwright/m3-smoke.spec.ts \
  playwright/diff-polish.spec.ts \
  playwright/worktree.spec.ts \
  playwright/notifications.spec.ts \
  playwright/m4-merge-surface.spec.ts \
  playwright/m4-range-diff.spec.ts \
  playwright/m4-multi-account.spec.ts \
  playwright/m4-ghe.spec.ts
```

## Scenario evidence map

1. Merge surface with branch protection and no-optimism reconcile flow  
   Artifacts:
   - `../m4-merge-surface/branch-protection-none.png`
   - `../m4-merge-surface/branch-protection-soft.png`
   - `../m4-merge-surface/branch-protection-hard.png`
   Spec: `apps/desktop/playwright/m4-merge-surface.spec.ts`

2. `mergeable: null` backoff schedule meter tick (2s/5s/15s/45s/120s/300s + resolved)  
   Artifacts:
   - `../m4-mergeable-backoff/tick-001-2s.png`
   - `../m4-mergeable-backoff/tick-002-5s.png`
   - `../m4-mergeable-backoff/tick-003-15s.png`
   - `../m4-mergeable-backoff/tick-004-45s.png`
   - `../m4-mergeable-backoff/tick-005-120s.png`
   - `../m4-mergeable-backoff/tick-006-300s.png`
   - `../m4-mergeable-backoff/tick-007-resolved.png`
   Spec: `apps/desktop/playwright/m4-merge-surface.spec.ts` (`mergeable null backoff` case)

3. Force-push range-diff render (local-git path + REST fallback path)  
   Artifacts:
   - `../m4-range-diff/local-git.png`
   - `../m4-range-diff/rest-compare.png`
   Spec: `apps/desktop/playwright/m4-range-diff.spec.ts`

4. Multi-account aggregated inbox + composer posting identity  
   Artifacts:
   - `../m4-multi-account/inbox-aggregated.png`
   - `../m4-multi-account/composer-identity-dropdown-open.png`
   Spec: `apps/desktop/playwright/m4-multi-account.spec.ts`

5. Per-account rate-limit pressure + foreground bypass behavior  
   Artifacts:
   - `../m4-foreground-bypass/01-low-budget-meter.png`
   - `../m4-foreground-bypass/02-background-throttled.png`
   - `../m4-foreground-bypass/03-foreground-bypass.png`
   Spec coverage: `apps/desktop/playwright/m4-multi-account.spec.ts`

6. GHE schema readiness flow against stubbed host (add account, inbox, PR detail)  
   Artifacts:
   - `../m4-ghe/01-add-account.png`
   - `../m4-ghe/02-ghe-inbox.png`
   - `../m4-ghe/03-ghe-pr-detail.png`
   Spec: `apps/desktop/playwright/m4-ghe.spec.ts`

# M5 final smoke (xvfb scripted)

Environment: Linux cloud agent, fixture-backed desktop app, multi-account fixture
set (github.com + GHE wiremock), executed with Playwright under `xvfb-run`.

This smoke pass re-verified the full M5 flow end-to-end and re-used existing
artifacts captured by prior M5 workers where coverage already existed.

## Smoke checklist and evidence

1. **Single suggestion apply** (confirm flow + applied state)
   - Spec: `apps/desktop/playwright/m5-suggestion-apply.spec.ts`
   - Artifacts: `artifacts/m5-suggestion-apply/single.png`
2. **Batch suggestion apply on clean mapped worktree** (step chips through push)
   - Spec: `apps/desktop/playwright/m5-suggestion-apply.spec.ts`
   - Artifacts:
     - `artifacts/m5-suggestion-apply/batch/01.png`
     - `artifacts/m5-suggestion-apply/batch/02.png`
     - `artifacts/m5-suggestion-apply/batch/03.png`
     - `artifacts/m5-suggestion-apply/batch/04.png`
     - `artifacts/m5-suggestion-apply/batch/05.png`
3. **Dirty worktree batch guard + explicit force-with-stash affordance**
   - Spec: `apps/desktop/playwright/m5-suggestion-apply.spec.ts`
   - Artifact: `artifacts/m5-suggestion-apply/dirty-blocked.png`
4. **Check annotations inline on diff + severity rendering + log tail stream**
   - Spec: `apps/desktop/playwright/m5-check-annotations.spec.ts`
   - Artifacts:
     - `artifacts/m5-check-annotations/diff-line.png`
     - `artifacts/m5-check-annotations/log-tail.png`
     - `artifacts/m5-check-annotations/log-tail-fix.webm`
5. **Rerun check run + rerun suite pending affordances**
   - Spec: `apps/desktop/playwright/m5-check-annotations.spec.ts`
   - Artifact: `artifacts/m5-check-annotations/rerun.png`
6. **Saved reply quick insert + clipboard image upload placeholder→URL**
   - Spec: `apps/desktop/playwright/m5-saved-replies-paste-image.spec.ts`
   - Artifacts:
     - `artifacts/m5-saved-replies/composer-dropdown.png`
     - `artifacts/m5-saved-replies/palette.png`
     - `artifacts/m5-saved-replies/saved-replies-paste-image-flow.webm`
     - `artifacts/m5-paste-image/paste-success.png`
     - `artifacts/m5-paste-image/paste-failure.png`
7. **Command palette open + search + command execute**
   - Spec: `apps/desktop/playwright/m5-command-palette.spec.ts`
   - Artifacts:
     - `artifacts/m5-command-palette/open-filter-run.png`
     - `artifacts/m5-command-palette/commands-flows.png`
8. **Full keyboard-only PR cycle**
   - Spec: `apps/desktop/playwright/m5-command-palette.spec.ts`
   - Artifacts:
     - `artifacts/m5-keyboard-layer/01-open-pr.png`
     - `artifacts/m5-keyboard-layer/02-resolve-thread.png`
     - `artifacts/m5-keyboard-layer/03-mark-file-viewed.png`
     - `artifacts/m5-keyboard-layer/04-open-github.png`
     - `artifacts/m5-keyboard-layer/full-cycle.webm`
9. **M5 accessibility sweep**
   - Spec: `apps/desktop/playwright/m5-a11y.spec.ts`

## Command transcript (final smoke run)

- `xvfb-run -a pnpm exec playwright test` from `apps/desktop`
- Includes all prior specs + M5 specs:
  - `m5-suggestion-apply.spec.ts`
  - `m5-check-annotations.spec.ts`
  - `m5-saved-replies-paste-image.spec.ts`
  - `m5-command-palette.spec.ts`
  - `m5-a11y.spec.ts`

#!/usr/bin/env bash
# M3 verifier matrix. Re-run from a clean shell to reproduce.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
mkdir -p verify/m3/logs

run() {
  local label="$1"
  local logfile="$2"
  shift 2
  echo "[verifier] running ${label} -> ${logfile}"
  if "$@" >"verify/m3/logs/${logfile}" 2>&1; then
    echo "[verifier] ${label} OK"
  else
    echo "[verifier] ${label} FAILED" >&2
    return 1
  fi
}

run "cargo fmt --check"             cargo-fmt.log              cargo fmt --check
run "cargo clippy"                  cargo-clippy.log           cargo clippy --workspace --all-targets -- -D warnings
run "cargo test"                    cargo-test.log             cargo test --workspace
run "ipc:bindings"                  ipc-bindings.log           pnpm ipc:bindings
run "pnpm typecheck"                pnpm-typecheck.log         pnpm typecheck
run "pnpm svelte-check"             pnpm-svelte-check.log      pnpm svelte-check
run "pnpm lint"                     pnpm-lint.log              pnpm lint
run "pnpm test"                     pnpm-test.log              pnpm test
run "pnpm corpus"                   pnpm-corpus.log            pnpm corpus
run "pnpm bench (xvfb webkit)"      pnpm-bench.log             xvfb-run -a pnpm bench
run "playwright airplane.spec"      airplane-spec.log          xvfb-run -a pnpm --filter desktop test:airplane
run "playwright m2-smoke.spec"      m2-smoke.log               xvfb-run -a pnpm --filter desktop exec playwright test playwright/m2-smoke.spec.ts
run "playwright m3-smoke.spec"      m3-smoke.log               xvfb-run -a pnpm --filter desktop exec playwright test playwright/m3-smoke.spec.ts
run "playwright diff-polish.spec"   diff-polish.log            xvfb-run -a pnpm --filter desktop exec playwright test playwright/diff-polish.spec.ts
run "playwright worktree.spec"      worktree.log               xvfb-run -a pnpm --filter desktop exec playwright test playwright/worktree.spec.ts
run "playwright notifications.spec" notifications.log          xvfb-run -a pnpm --filter desktop exec playwright test playwright/notifications.spec.ts
run "playwright a11y-pr-detail.spec" a11y-pr-detail.log        xvfb-run -a pnpm --filter desktop exec playwright test playwright/a11y-pr-detail.spec.ts

echo "[verifier] all matrix steps green"

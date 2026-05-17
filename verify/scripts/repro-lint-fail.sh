#!/usr/bin/env bash
# Reproduces the `pnpm lint` failure introduced by tauri-build's auto-generated
# JSON schemas under apps/desktop/src-tauri/gen/schemas/ being picked up by
# prettier --check. The directory is in the *root* .gitignore, but prettier 3.x
# only reads the .gitignore that lives in the cwd it is invoked from
# (apps/desktop/), so the gen/ files are not excluded. The repo's
# apps/desktop/.prettierignore does not list src-tauri/gen either.
#
# Usage:
#   bash verify/scripts/repro-lint-fail.sh

set -euo pipefail

cd "$(dirname "$0")/../.."

echo "[1/3] removing stale generated schemas (gitignored)"
rm -rf apps/desktop/src-tauri/gen
touch apps/desktop/src-tauri/build.rs

echo "[2/3] regenerating IPC bindings (forces tauri-build to write gen/ files)"
cargo build -p desktop --bin generate-ipc-bindings >/dev/null

echo "[3/3] running pnpm lint (expected to FAIL on prettier --check on gen/schemas/*.json)"
set +e
pnpm lint
status=$?
set -e

if [[ $status -ne 0 ]]; then
  echo "REPRO OK: pnpm lint failed with exit $status (prettier flagged tauri-build outputs)" >&2
  exit 0
else
  echo "UNEXPECTED: pnpm lint passed; the underlying issue may have been mitigated" >&2
  exit 1
fi

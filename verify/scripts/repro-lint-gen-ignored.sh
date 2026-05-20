#!/usr/bin/env bash
# Verifier repro: ensure .prettierignore in apps/desktop now excludes
# Tauri-generated src-tauri/gen/** artifacts. Prior to fix-gates, prettier
# would re-check these files and fail.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Make sure the gen dir exists (created by `pnpm ipc:bindings` or
# `cargo build` of the desktop crate).
if [[ ! -d apps/desktop/src-tauri/gen/schemas ]]; then
  echo "src-tauri/gen/schemas missing; run 'pnpm ipc:bindings' first." >&2
  exit 1
fi

# Plant a syntactically intentionally bad-formatted JSON file under the
# gen tree. If .prettierignore is honoured by the lint step, prettier
# must not check this file and `pnpm lint` must still exit 0.
target="apps/desktop/src-tauri/gen/schemas/__verify_unformatted.json"
trap 'rm -f "$target"' EXIT
printf '{"badly":   "formatted","arr":[1,    2,3  ],"nested":{"a":  1}}\n' >"$target"

echo "[repro] planted: $target"
echo "[repro] running pnpm lint..."
pnpm lint
echo "[repro] pnpm lint exited 0 with gen/ ignored as expected."

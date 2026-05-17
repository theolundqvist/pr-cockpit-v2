#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${REPO_ROOT}/apps/desktop/src/assets/grammars"

mkdir -p "${TARGET_DIR}"

fetch_grammar() {
  local output_name="$1"
  local source_url="$2"
  local expected_sha="$3"
  local output_path="${TARGET_DIR}/${output_name}"

  if [[ -f "${output_path}" ]]; then
    echo "skip ${output_name} (already present)"
    return
  fi

  local tmp_file
  tmp_file="$(mktemp)"
  curl -fsSL "${source_url}" -o "${tmp_file}"

  local actual_sha
  actual_sha="$(sha256sum "${tmp_file}" | awk '{print $1}')"
  if [[ "${actual_sha}" != "${expected_sha}" ]]; then
    rm -f "${tmp_file}"
    echo "sha256 mismatch for ${output_name}: expected ${expected_sha}, got ${actual_sha}" >&2
    exit 1
  fi

  mv "${tmp_file}" "${output_path}"
  echo "fetched ${output_name}"
}

BASE_URL="https://unpkg.com/tree-sitter-wasms@0.1.13/out"

fetch_grammar "tree-sitter-rust.wasm" "${BASE_URL}/tree-sitter-rust.wasm" "4409921a70d0aa5bec7d1d7ce809a557a8ee1cf6ace901e3ac6a76e62cfea903"
fetch_grammar "tree-sitter-typescript.wasm" "${BASE_URL}/tree-sitter-typescript.wasm" "8515404dceed38e1ed86aa34b09fcf3379fff1b4ff9dd3967bcd6d1eb5ac3d8f"
fetch_grammar "tree-sitter-javascript.wasm" "${BASE_URL}/tree-sitter-javascript.wasm" "63812b9e275d26851264734868d27a1656bd44a2ef6eb3e85e6b03728c595ab5"
fetch_grammar "tree-sitter-json.wasm" "${BASE_URL}/tree-sitter-json.wasm" "fdb5219abe058369e16897aaa11eecf47ef4f546752c3ddbac339cdd89e1e667"
fetch_grammar "tree-sitter-yaml.wasm" "${BASE_URL}/tree-sitter-yaml.wasm" "5dea7cfff83d41d8f87fb8e434e1a5b292c0d670bfcdc42cb2af420ef490dde5"
fetch_grammar "tree-sitter-go.wasm" "${BASE_URL}/tree-sitter-go.wasm" "9963ca89b616eaf04b08a43bc1fb0f07b85395bec313330851f1f1ead2f755b6"
fetch_grammar "tree-sitter-python.wasm" "${BASE_URL}/tree-sitter-python.wasm" "9056d0fb0c337810d019fae350e8167786119da98f0f282aceae7ab89ee8253b"
fetch_grammar "tree-sitter-markdown.wasm" "https://github.com/tree-sitter-grammars/tree-sitter-markdown/releases/download/v0.5.3/tree-sitter-markdown.wasm" "dd9fc12ac2804d7c7da787e4774125b32e4fb3c244e5e7031a77cb7dd8036020"

# Markdown corpus harness

This directory holds the M1 markdown fidelity corpus and scorer.

## Files

- `corpus.json`: committed snapshot entries.
- `oracle/<id>.html`: committed oracle HTML per corpus entry.
- `fetch.mjs`: optional refresh script against live GitHub REST.
- `score.mjs`: renders corpus bodies through the Rust renderer, compares with oracles, and enforces the regression gate.

## Snapshot format

Each `corpus.json` entry has:

```json
{
  "id": "cli-cli-123456",
  "source_url": "https://api.github.com/repos/cli/cli/issues/comments/123456",
  "body": "markdown body",
  "oracle_html_sha": "sha256 of oracle/<id>.html",
  "repo": "cli/cli",
  "accepted_drift": 0.0
}
```

`accepted_drift` is optional and defaults to `0`. When present, it is subtracted
from the entry's visible-text distance before contributing to `weighted_mean`.

## Refresh corpus snapshot

The snapshot is committed and used by CI. Network refresh is opt-in and never required for CI.

```bash
pnpm corpus:fetch
```

Environment:

- Optional `GITHUB_TOKEN` increases API quota.
- Without a token, the script still works against public APIs with lower rate limits.

Refresh behavior:

1. Pulls public issue comment bodies from:
   - `cli/cli`
   - `microsoft/vscode`
   - `rust-lang/rust`
   - `kubernetes/kubernetes`
2. Uses GitHub REST `Accept: application/vnd.github.full+json` to obtain `body` and rendered `body_html` for each comment.
3. Writes:
   - `tools/markdown-corpus/corpus.json`
   - `tools/markdown-corpus/oracle/<id>.html`

## Run the gate

```bash
pnpm corpus
```

The scorer reports:

- Structural DOM diff ratio (normalized DOM trees).
- Visible-character mismatch ratio (text-only distance).
- Weighted score over the full corpus.

CI fails if weighted score is `> 0.015` (1.5%) for M3.

### Top-drift diagnostics

Use `--dump-csv <path>` to export per-entry weighted contribution rows sorted
by contribution descending:

```bash
pnpm corpus -- --dump-csv tools/markdown-corpus/top-drift.csv
```

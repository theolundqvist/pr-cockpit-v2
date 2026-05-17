import { createHash } from "node:crypto";
import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ORACLE_DIR = path.join(__dirname, "oracle");
const CORPUS_JSON = path.join(__dirname, "corpus.json");
const REPOS = ["cli/cli", "microsoft/vscode", "rust-lang/rust", "kubernetes/kubernetes"];
const TARGET = 200;

function isStableBodyForM1(body) {
  if (body.length < 20 || body.length > 2000) return false;
  if (body.includes("	")) return false;
  if (/[`|<>]/.test(body)) return false;
  if (/^\s*[-*]\s+\[[ xX]\]/m.test(body)) return false;
  if (/^\s*>\s*\[![A-Z]+\]/m.test(body)) return false;
  if (/!\[[^\]]*]\([^)]+\)/.test(body)) return false;
  if (/```/.test(body)) return false;
  return true;
}

function sha256(input) {
  return createHash("sha256").update(input).digest("hex");
}

async function fetchJson(url) {
  const headers = {
    Accept: "application/vnd.github.full+json",
    "User-Agent": "pr-cockpit-markdown-corpus-fetcher",
    "X-GitHub-Api-Version": "2022-11-28",
  };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  const response = await fetch(url, { headers });
  if (!response.ok) {
    throw new Error(`GitHub API ${response.status} on ${url}`);
  }
  return response.json();
}

function deriveIssueNumber(issueUrl) {
  const parts = issueUrl.split("/");
  return parts[parts.length - 1] ?? "0";
}

async function main() {
  await rm(ORACLE_DIR, { recursive: true, force: true });
  await mkdir(ORACLE_DIR, { recursive: true });

  const gathered = [];
  const perRepoTarget = Math.ceil(TARGET / REPOS.length);

  for (const repo of REPOS) {
    let page = 1;
    while (gathered.filter((entry) => entry.repo === repo).length < perRepoTarget) {
      const url = `https://api.github.com/repos/${repo}/issues/comments?per_page=100&page=${page}&sort=created&direction=desc`;
      const payload = await fetchJson(url);
      if (!payload.length) {
        break;
      }

      for (const comment of payload) {
        if (!comment.body?.trim()) continue;
        if (!isStableBodyForM1(comment.body)) continue;
        if (!comment.body_html?.trim()) continue;

        const issueNumber = deriveIssueNumber(comment.issue_url);
        const id = `${repo.replace("/", "-")}-${comment.id}`;
        const oraclePath = path.join(ORACLE_DIR, `${id}.html`);
        await writeFile(oraclePath, comment.body_html, "utf8");
        gathered.push({
          id,
          source_url: comment.url,
          body: comment.body,
          oracle_html_sha: sha256(comment.body_html),
          repo,
        });

        if (gathered.length >= TARGET) break;
        if (gathered.filter((entry) => entry.repo === repo).length >= perRepoTarget) break;

        if (comment.body.includes(`#${issueNumber}`)) {
          // Keep locally contextual bodies in the sample.
        }
      }

      if (gathered.length >= TARGET) break;
      page += 1;
    }

    if (gathered.length >= TARGET) break;
  }

  const trimmed = gathered.slice(0, TARGET);
  await writeFile(CORPUS_JSON, JSON.stringify(trimmed, null, 2) + "
", "utf8");
  console.log(`wrote ${trimmed.length} corpus entries`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});

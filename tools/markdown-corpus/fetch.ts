import { createHash } from "node:crypto";
import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";

type ApiComment = {
  id: number;
  body: string | null;
  body_html?: string;
  html_url: string;
  issue_url: string;
  pull_request_url?: string;
  url: string;
};

type CorpusEntry = {
  id: string;
  source_url: string;
  body: string;
  oracle_html_sha: string;
  repo?: string;
};

const ROOT = path.resolve(__dirname);
const ORACLE_DIR = path.join(ROOT, "oracle");
const CORPUS_JSON = path.join(ROOT, "corpus.json");
const REPOS = [
  "cli/cli",
  "microsoft/vscode",
  "rust-lang/rust",
  "kubernetes/kubernetes",
];
const TARGET = 200;

function isStableBodyForM1(body: string): boolean {
  if (body.length < 20 || body.length > 2000) {
    return false;
  }
  if (body.includes("\t")) {
    return false;
  }
  if (/[`|<>]/.test(body)) {
    return false;
  }
  if (/^\s*[-*]\s+\[[ xX]\]/m.test(body)) {
    return false;
  }
  if (/^\s*>\s*\[![A-Z]+\]/m.test(body)) {
    return false;
  }
  if (/!\[[^\]]*]\([^)]+\)/.test(body)) {
    return false;
  }
  if (/```/.test(body)) {
    return false;
  }
  return true;
}

function sha256(input: string): string {
  return createHash("sha256").update(input).digest("hex");
}

async function fetchJson(url: string): Promise<unknown> {
  const headers: Record<string, string> = {
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
  return (await response.json()) as unknown;
}

function deriveIssueNumber(issueUrl: string): string {
  const parts = issueUrl.split("/");
  return parts[parts.length - 1] ?? "0";
}

async function main() {
  await rm(ORACLE_DIR, { recursive: true, force: true });
  await mkdir(ORACLE_DIR, { recursive: true });

  const gathered: CorpusEntry[] = [];
  const perRepoTarget = Math.ceil(TARGET / REPOS.length);

  for (const repo of REPOS) {
    let page = 1;
    while (gathered.filter((entry) => entry.repo === repo).length < perRepoTarget) {
      const url = `https://api.github.com/repos/${repo}/issues/comments?per_page=100&page=${page}&sort=created&direction=desc`;
      const payload = (await fetchJson(url)) as ApiComment[];
      if (!payload.length) {
        break;
      }

      for (const comment of payload) {
        if (!comment.body?.trim()) {
          continue;
        }
        if (!isStableBodyForM1(comment.body)) {
          continue;
        }
        if (!comment.body_html?.trim()) {
          continue;
        }

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

        if (gathered.length >= TARGET) {
          break;
        }
        if (gathered.filter((entry) => entry.repo === repo).length >= perRepoTarget) {
          break;
        }

        if (comment.body.includes(`#${issueNumber}`)) {
          // Keep locally contextual bodies in the sample.
        }
      }

      if (gathered.length >= TARGET) {
        break;
      }
      page += 1;
    }

    if (gathered.length >= TARGET) {
      break;
    }
  }

  const trimmed = gathered.slice(0, TARGET);
  await writeFile(CORPUS_JSON, JSON.stringify(trimmed, null, 2) + "\n", "utf8");

  // eslint-disable-next-line no-console
  console.log(`wrote ${trimmed.length} corpus entries`);
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});

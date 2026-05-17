import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { JSDOM } from "jsdom";

type CorpusEntry = {
  id: string;
  source_url: string;
  body: string;
  oracle_html_sha: string;
  repo?: string;
};

type RenderResult = {
  id: string;
  html: string;
};

const ROOT = path.resolve(__dirname);
const CORPUS_JSON = path.join(ROOT, "corpus.json");
const ORACLE_DIR = path.join(ROOT, "oracle");
const GATE = 0.02;

function sha256(input: string): string {
  return createHash("sha256").update(input).digest("hex");
}

function normalizeHref(href: string): string {
  if (href.startsWith("https://github.com/")) {
    return href.replace("https://github.com", "");
  }
  return href;
}

function normalizeHtml(html: string): string {
  const dom = new JSDOM(`<body>${html}</body>`);
  const body = dom.window.document.body;
  const chunks: string[] = [];

  const walk = (node: Node) => {
    if (node.nodeType === dom.window.Node.TEXT_NODE) {
      const value = node.textContent?.replace(/\s+/g, " ").trim() ?? "";
      if (value) {
        chunks.push(`#text(${value})`);
      }
      return;
    }
    if (node.nodeType !== dom.window.Node.ELEMENT_NODE) {
      return;
    }
    const element = node as Element;
    const tag = element.tagName.toLowerCase();
    const attrs = [...element.attributes]
      .filter((attr) => attr.name !== "id")
      .map((attr) => {
        if (attr.name === "class") {
          const sorted = attr.value
            .split(/\s+/)
            .filter(Boolean)
            .sort()
            .join(" ");
          return `${attr.name}=${sorted}`;
        }
        if (attr.name === "href") {
          return `${attr.name}=${normalizeHref(attr.value)}`;
        }
        if (attr.name === "src" && attr.value.startsWith("https://avatars.githubusercontent.com")) {
          return "src=<avatar>";
        }
        return `${attr.name}=${attr.value}`;
      })
      .sort()
      .join(",");
    chunks.push(`<${tag}${attrs ? ` ${attrs}` : ""}>`);
    for (const child of [...element.childNodes]) {
      walk(child);
    }
    chunks.push(`</${tag}>`);
  };

  for (const child of [...body.childNodes]) {
    walk(child);
  }
  return chunks.join("");
}

function visibleText(html: string): string {
  const dom = new JSDOM(`<body>${html}</body>`);
  return (dom.window.document.body.textContent ?? "")
    .replace(/https?:\/\/\S+/g, " ")
    .replace(/\b([0-9a-f]{7})[0-9a-f]{1,33}\b/gi, "$1")
    .replace(/:[a-z0-9_+-]+:/gi, " ")
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function levenshtein(a: string, b: string): number {
  if (a === b) {
    return 0;
  }
  if (!a.length) {
    return b.length;
  }
  if (!b.length) {
    return a.length;
  }

  const prev = new Array<number>(b.length + 1);
  const curr = new Array<number>(b.length + 1);
  for (let j = 0; j <= b.length; j += 1) {
    prev[j] = j;
  }
  for (let i = 1; i <= a.length; i += 1) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(
        curr[j - 1] + 1,
        prev[j] + 1,
        prev[j - 1] + cost,
      );
    }
    for (let j = 0; j <= b.length; j += 1) {
      prev[j] = curr[j];
    }
  }
  return prev[b.length];
}

function ratioDistance(a: string, b: string): number {
  const denom = Math.max(a.length, b.length, 1);
  return levenshtein(a, b) / denom;
}

async function loadCorpus(): Promise<CorpusEntry[]> {
  const raw = await readFile(CORPUS_JSON, "utf8");
  return JSON.parse(raw) as CorpusEntry[];
}

async function loadOracle(entry: CorpusEntry): Promise<string> {
  const file = path.join(ORACLE_DIR, `${entry.id}.html`);
  const html = await readFile(file, "utf8");
  const actualSha = sha256(html);
  if (actualSha !== entry.oracle_html_sha) {
    throw new Error(`oracle sha mismatch for ${entry.id}: ${actualSha} != ${entry.oracle_html_sha}`);
  }
  return html;
}

function renderCockpit(entries: CorpusEntry[]): Map<string, string> {
  const payload = entries.map((entry) => ({
    id: entry.id,
    body: entry.body,
    repo: entry.repo ?? null,
  }));

  const command = spawnSync(
    "cargo",
    [
      "run",
      "--quiet",
      "-p",
      "pr-cockpit-desktop",
      "--bin",
      "render_batch",
    ],
    {
      cwd: path.resolve(ROOT, "..", ".."),
      input: JSON.stringify(payload),
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 50,
    },
  );

  if (command.status !== 0) {
    throw new Error(command.stderr || "render_batch failed");
  }

  const rows = JSON.parse(command.stdout) as RenderResult[];
  return new Map(rows.map((row) => [row.id, row.html]));
}

async function main() {
  const entries = await loadCorpus();
  if (!entries.length) {
    throw new Error("corpus.json is empty");
  }
  const rendered = renderCockpit(entries);

  let structuralTotal = 0;
  let visibleTotal = 0;
  let weightedTotal = 0;

  for (const entry of entries) {
    const oracle = await loadOracle(entry);
    const cockpit = rendered.get(entry.id);
    if (!cockpit) {
      throw new Error(`missing render output for ${entry.id}`);
    }

    const structural = ratioDistance(normalizeHtml(oracle), normalizeHtml(cockpit));
    const visible = ratioDistance(visibleText(oracle), visibleText(cockpit));
    const weighted = visible;

    structuralTotal += structural;
    visibleTotal += visible;
    weightedTotal += weighted;
  }

  const count = entries.length;
  const structuralMean = structuralTotal / count;
  const visibleMean = visibleTotal / count;
  const weightedMean = weightedTotal / count;

  // eslint-disable-next-line no-console
  console.log(`entries=${count}`);
  // eslint-disable-next-line no-console
  console.log(`structural_mean=${structuralMean.toFixed(6)}`);
  // eslint-disable-next-line no-console
  console.log(`visible_mean=${visibleMean.toFixed(6)}`);
  // eslint-disable-next-line no-console
  console.log(`weighted_mean=${weightedMean.toFixed(6)}`);
  // eslint-disable-next-line no-console
  console.log(`gate=${GATE}`);

  if (weightedMean > GATE) {
    throw new Error(`markdown corpus regression ${weightedMean.toFixed(6)} exceeds ${GATE}`);
  }
}

main().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { JSDOM } from "jsdom";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const CORPUS_JSON = path.join(__dirname, "corpus.json");
const ORACLE_DIR = path.join(__dirname, "oracle");
const GATE = 0.015;

function sha256(input) {
  return createHash("sha256").update(input).digest("hex");
}

function normalizeHref(href) {
  if (href.startsWith("https://github.com/")) {
    return href.replace("https://github.com", "");
  }
  return href;
}

function normalizeHtml(html) {
  const dom = new JSDOM(`<body>${html}</body>`);
  const body = dom.window.document.body;
  const chunks = [];

  const walk = (node) => {
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
    const element = node;
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

function visibleText(html) {
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

function levenshtein(a, b) {
  if (a === b) return 0;
  if (!a.length) return b.length;
  if (!b.length) return a.length;

  const prev = new Array(b.length + 1);
  const curr = new Array(b.length + 1);
  for (let j = 0; j <= b.length; j += 1) {
    prev[j] = j;
  }
  for (let i = 1; i <= a.length; i += 1) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost);
    }
    for (let j = 0; j <= b.length; j += 1) {
      prev[j] = curr[j];
    }
  }
  return prev[b.length];
}

function ratioDistance(a, b) {
  const denom = Math.max(a.length, b.length, 1);
  return levenshtein(a, b) / denom;
}

async function loadCorpus() {
  const raw = await readFile(CORPUS_JSON, "utf8");
  return JSON.parse(raw);
}

function parseArgs(argv) {
  let csvPath = null;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--dump-csv") {
      csvPath = argv[index + 1] ?? null;
      index += 1;
    }
  }
  return { csvPath };
}

function toCsv(value) {
  const text = String(value ?? "");
  if (text.includes(",") || text.includes('"') || text.includes("\n")) {
    return `"${text.replaceAll('"', '""')}"`;
  }
  return text;
}

async function loadOracle(entry) {
  const file = path.join(ORACLE_DIR, `${entry.id}.html`);
  const html = await readFile(file, "utf8");
  const actualSha = sha256(html);
  if (actualSha !== entry.oracle_html_sha) {
    throw new Error(`oracle sha mismatch for ${entry.id}: ${actualSha} != ${entry.oracle_html_sha}`);
  }
  return html;
}

function renderCockpit(entries) {
  const payload = entries.map((entry) => ({
    id: entry.id,
    body: entry.body,
    repo: entry.repo ?? null,
  }));

  const command = spawnSync(
    "cargo",
    ["run", "--quiet", "-p", "desktop", "--bin", "render_batch"],
    {
      cwd: path.resolve(__dirname, "..", ".."),
      input: JSON.stringify(payload),
      encoding: "utf8",
      maxBuffer: 1024 * 1024 * 50,
    },
  );

  if (command.status !== 0) {
    throw new Error(command.stderr || "render_batch failed");
  }

  const rows = JSON.parse(command.stdout);
  return new Map(rows.map((row) => [row.id, row.html]));
}

async function main() {
  const { csvPath } = parseArgs(process.argv.slice(2));
  const entries = await loadCorpus();
  if (!entries.length) {
    throw new Error("corpus.json is empty");
  }
  const rendered = renderCockpit(entries);

  let structuralTotal = 0;
  let visibleTotal = 0;
  let weightedTotal = 0;
  const perEntry = [];

  for (const entry of entries) {
    const oracle = await loadOracle(entry);
    const cockpit = rendered.get(entry.id);
    if (!cockpit) {
      throw new Error(`missing render output for ${entry.id}`);
    }

    const structural = ratioDistance(normalizeHtml(oracle), normalizeHtml(cockpit));
    const visible = ratioDistance(visibleText(oracle), visibleText(cockpit));
    const acceptedDrift = Number(entry.accepted_drift ?? 0);
    if (!Number.isFinite(acceptedDrift) || acceptedDrift < 0) {
      throw new Error(`invalid accepted_drift for ${entry.id}: ${entry.accepted_drift}`);
    }
    const weighted = Math.max(0, visible - acceptedDrift);

    structuralTotal += structural;
    visibleTotal += visible;
    weightedTotal += weighted;
    perEntry.push({
      id: entry.id,
      repo: entry.repo ?? "",
      source_url: entry.source_url ?? "",
      structural,
      visible,
      accepted_drift: acceptedDrift,
      weighted,
    });
  }

  const count = entries.length;
  const structuralMean = structuralTotal / count;
  const visibleMean = visibleTotal / count;
  const weightedMean = weightedTotal / count;
  const ranked = [...perEntry]
    .map((row) => ({
      ...row,
      contribution: row.weighted / count,
    }))
    .sort((a, b) => b.contribution - a.contribution);

  console.log(`entries=${count}`);
  console.log(`structural_mean=${structuralMean.toFixed(6)}`);
  console.log(`visible_mean=${visibleMean.toFixed(6)}`);
  console.log(`weighted_mean=${weightedMean.toFixed(6)}`);
  console.log(`gate=${GATE}`);
  for (const [index, row] of ranked.slice(0, 3).entries()) {
    console.log(
      `top${index + 1}=${row.id},weighted=${row.weighted.toFixed(6)},accepted_drift=${row.accepted_drift.toFixed(6)},contribution=${row.contribution.toFixed(6)}`,
    );
  }

  if (csvPath) {
    const header = [
      "rank",
      "id",
      "repo",
      "source_url",
      "structural",
      "visible",
      "accepted_drift",
      "weighted",
      "contribution",
    ].join(",");
    const lines = ranked.map((row, index) =>
      [
        index + 1,
        toCsv(row.id),
        toCsv(row.repo),
        toCsv(row.source_url),
        row.structural.toFixed(6),
        row.visible.toFixed(6),
        row.accepted_drift.toFixed(6),
        row.weighted.toFixed(6),
        row.contribution.toFixed(6),
      ].join(","),
    );
    await writeFile(csvPath, `${header}\n${lines.join("\n")}\n`, "utf8");
    console.log(`debug_csv=${csvPath}`);
  }

  if (weightedMean > GATE) {
    throw new Error(`markdown corpus regression ${weightedMean.toFixed(6)} exceeds ${GATE}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

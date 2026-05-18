import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { compareBudgets } from "./compare-budgets-lib.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");

async function readJson(relativePath) {
  const raw = await readFile(path.join(root, relativePath), "utf8");
  return JSON.parse(raw);
}

async function main() {
  const budgets = await readJson("bench/budgets.json");
  const rust = await readJson("bench/results/rust.json");
  const frontend = await readJson("bench/results/frontend.json");
  const commandPalette = await readJson("bench/results/command-palette.json");

  const { failures, reports } = compareBudgets({
    budgets,
    rustMetrics: rust.metrics,
    frontendMetrics: frontend.metrics,
    commandPaletteMetrics: commandPalette.metrics,
  });

  for (const report of reports) {
    const comparator = report.kind === "max" ? "<=" : ">=";
    console.log(`${report.metricName}: ${report.value.toFixed(2)}${report.unit} ${comparator} ${report.budget}${report.unit}`);
  }

  if (failures.length) {
    for (const failure of failures) {
      console.error(failure);
    }
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");

function applyTolerance(baseline, kind, tolerancePct) {
  if (baseline == null) {
    return null;
  }
  const tolerance = tolerancePct / 100;
  return kind === "max" ? baseline * (1 + tolerance) : baseline * (1 - tolerance);
}

async function readJson(relativePath) {
  const raw = await readFile(path.join(root, relativePath), "utf8");
  return JSON.parse(raw);
}

async function main() {
  const budgets = await readJson("bench/budgets.json");
  const rust = await readJson("bench/results/rust.json");
  const frontend = await readJson("bench/results/frontend.json");

  const aliases = {
    inbox_first_paint_ms_frontend: "inbox_first_paint_ms",
    pr_detail_open_preloaded_ms_frontend: "pr_detail_open_preloaded_ms",
    pr_detail_open_cold_ms_frontend: "pr_detail_open_cold_ms",
    file_open_in_diff_cached_ms_frontend: "file_open_in_diff_cached_ms",
  };

  const sources = {
    rust: rust.metrics,
    frontend: frontend.metrics,
  };

  const failures = [];
  for (const [metricName, spec] of Object.entries(budgets.metrics)) {
    const source = sources[spec.source];
    const sourceKey = aliases[metricName] ?? metricName;
    if (!source || source[sourceKey] == null) {
      failures.push(`${metricName}: missing measurement in ${spec.source}`);
      continue;
    }

    const value = Number(source[sourceKey]);
    if (Number.isNaN(value)) {
      failures.push(`${metricName}: NaN measurement`);
      continue;
    }

    const budget = Number(spec.budget);
    if (spec.kind === "max" && value > budget) {
      failures.push(`${metricName}: ${value}${spec.unit} exceeds hard budget ${budget}${spec.unit}`);
    }
    if (spec.kind === "min" && value < budget) {
      failures.push(`${metricName}: ${value}${spec.unit} below hard budget ${budget}${spec.unit}`);
    }

    const threshold = applyTolerance(spec.baseline ?? null, spec.kind, budgets.tolerance_pct);
    if (threshold != null) {
      if (spec.kind === "max" && value > threshold) {
        failures.push(
          `${metricName}: ${value}${spec.unit} regressed past ${budgets.tolerance_pct}% threshold (${threshold.toFixed(2)}${spec.unit})`,
        );
      }
      if (spec.kind === "min" && value < threshold) {
        failures.push(
          `${metricName}: ${value}${spec.unit} regressed past ${budgets.tolerance_pct}% threshold (${threshold.toFixed(2)}${spec.unit})`,
        );
      }
    }

    const comparator = spec.kind === "max" ? "<=" : ">=";
    console.log(`${metricName}: ${value.toFixed(2)}${spec.unit} ${comparator} ${budget}${spec.unit}`);
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

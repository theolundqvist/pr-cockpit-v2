import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");
const resultsDir = path.join(root, "bench", "results");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with ${result.status}`);
  }
}

async function readEstimate(relativePath) {
  const file = path.join(root, relativePath);
  const raw = await readFile(file, "utf8");
  const json = JSON.parse(raw);
  return Number(json.mean.point_estimate);
}

async function main() {
  await mkdir(resultsDir, { recursive: true });

  run("cargo", [
    "bench",
    "-p",
    "desktop",
    "--bench",
    "inbox_first_paint",
    "--bench",
    "pr_detail_open",
    "--bench",
    "file_open_in_diff_cached",
    "--bench",
    "comrak_render_throughput",
    "--bench",
    "mutation_submit_visible",
    "--",
    "--noplot",
  ]);

  const inboxNs = await readEstimate("target/criterion/inbox_first_paint/new/estimates.json");
  const preloadedNs = await readEstimate("target/criterion/pr_detail_open_preloaded/new/estimates.json");
  const coldNs = await readEstimate("target/criterion/pr_detail_open_cold_cache/new/estimates.json");
  const fileOpenNs = await readEstimate("target/criterion/file_open_in_diff_cached/new/estimates.json");
  const comrakNs = await readEstimate("target/criterion/comrak_render_throughput/comrak_render_throughput/new/estimates.json");
  const mutationSubmitVisibleNs = await readEstimate("target/criterion/mutation_submit_visible/new/estimates.json");

  const metrics = {
    inbox_first_paint_ms: inboxNs / 1e6,
    pr_detail_open_preloaded_ms: preloadedNs / 1e6,
    pr_detail_open_cold_ms: coldNs / 1e6,
    file_open_in_diff_cached_ms: fileOpenNs / 1e6,
    mutation_submit_visible_ms: mutationSubmitVisibleNs / 1e6,
    comrak_render_throughput_ops_per_sec: 1e9 / comrakNs,
  };

  await writeFile(
    path.join(resultsDir, "rust.json"),
    JSON.stringify({
      source: "criterion",
      captured_at: new Date().toISOString(),
      metrics,
    }, null, 2) + "\n",
    "utf8",
  );

  for (const [name, value] of Object.entries(metrics)) {
    console.log(`${name}=${value.toFixed(2)}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

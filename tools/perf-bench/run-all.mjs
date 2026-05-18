import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const root = path.resolve(__dirname, "..", "..");

function run(command, args, extraEnv = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ...extraEnv },
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with ${result.status}`);
  }
}

async function main() {
  run("pnpm", ["--filter", "desktop", "build"]);
  run("node", ["tools/perf-bench/run-rust-benches.mjs"]);
  run("node", ["tools/perf-bench/run-frontend-bench.mjs"], {
    PERF_BROWSER: process.env.PERF_BROWSER ?? "webkit",
    PERF_TARGET: process.env.PERF_TARGET ?? "preview",
  });
  run("node", ["tools/perf-bench/command-palette.mjs"], {
    PERF_BROWSER: process.env.PERF_BROWSER ?? "webkit",
    PERF_TARGET: process.env.PERF_TARGET ?? "preview",
  });
  run("node", ["tools/perf-bench/compare-budgets.mjs"]);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exit(1);
});

import assert from "node:assert/strict";
import test from "node:test";
import { compareBudgets } from "./compare-budgets-lib.mjs";

const budgets = {
  tolerance_pct: 10,
  metrics: {
    comrak_render_throughput_ops_per_sec: {
      source: "rust",
      kind: "min",
      unit: "ops/s",
      budget: 850,
      baseline: 15000,
    },
    pr_detail_open_preloaded_ms_frontend: {
      source: "frontend",
      kind: "max",
      unit: "ms",
      budget: 50,
      baseline: 40,
    },
  },
};

test("passes when metrics satisfy hard budgets and baseline threshold", () => {
  const result = compareBudgets({
    budgets,
    rustMetrics: { comrak_render_throughput_ops_per_sec: 20336.98 },
    frontendMetrics: { pr_detail_open_preloaded_ms: 40 },
  });

  assert.deepEqual(result.failures, []);
  assert.equal(result.reports.length, 2);
});

test("fails baseline regression without violating hard budget", () => {
  const result = compareBudgets({
    budgets,
    rustMetrics: { comrak_render_throughput_ops_per_sec: 13000 },
    frontendMetrics: { pr_detail_open_preloaded_ms: 45 },
  });

  assert.deepEqual(result.failures, [
    "comrak_render_throughput_ops_per_sec: 13000ops/s regressed past 10% threshold (13500.00ops/s)",
    "pr_detail_open_preloaded_ms_frontend: 45ms regressed past 10% threshold (44.00ms)",
  ]);
});

test("fails hard budget on controlled +10% drift", () => {
  const result = compareBudgets({
    budgets,
    rustMetrics: { comrak_render_throughput_ops_per_sec: 765 },
    frontendMetrics: { pr_detail_open_preloaded_ms: 55 },
  });

  assert(result.failures.some((failure) => failure.includes("comrak_render_throughput_ops_per_sec: 765ops/s below hard budget 850ops/s")));
  assert(result.failures.some((failure) => failure.includes("pr_detail_open_preloaded_ms_frontend: 55ms exceeds hard budget 50ms")));
});

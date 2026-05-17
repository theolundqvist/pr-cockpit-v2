export const aliases = {
  inbox_first_paint_ms_frontend: "inbox_first_paint_ms",
  pr_detail_open_preloaded_ms_frontend: "pr_detail_open_preloaded_ms",
  pr_detail_open_cold_ms_frontend: "pr_detail_open_cold_ms",
  file_open_in_diff_cached_ms_frontend: "file_open_in_diff_cached_ms",
};

export function applyTolerance(baseline, kind, tolerancePct) {
  if (baseline == null) {
    return null;
  }
  const tolerance = tolerancePct / 100;
  return kind === "max" ? baseline * (1 + tolerance) : baseline * (1 - tolerance);
}

export function compareBudgets({ budgets, rustMetrics, frontendMetrics }) {
  const sources = {
    rust: rustMetrics,
    frontend: frontendMetrics,
  };

  const failures = [];
  const reports = [];

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

    reports.push({
      metricName,
      kind: spec.kind,
      unit: spec.unit,
      budget,
      value,
    });
  }

  return { failures, reports };
}

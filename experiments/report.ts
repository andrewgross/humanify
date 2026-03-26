/**
 * Human-readable reporting for experiment results.
 */
import type { ExperimentResult, PerFileBreakdown } from "./types.js";

function pct(n: number): string {
  return `${(n * 100).toFixed(1)}%`;
}

function fixed(n: number, decimals = 3): string {
  return n.toFixed(decimals);
}

function pad(s: string, width: number): string {
  return s.padEnd(width);
}

function rpad(s: string, width: number): string {
  return s.padStart(width);
}

function divider(char = "─", width = 60): string {
  return char.repeat(width);
}

export function formatExperimentResult(result: ExperimentResult): string {
  const lines: string[] = [];
  const { metrics, timing, perFileBreakdown } = result;

  lines.push(`\n${divider("═")}`);
  lines.push(`  Experiment: ${result.fixture}`);
  lines.push(divider("═"));

  // Overview
  lines.push(
    `\n  Functions: ${metrics.totalFunctions} total, ${metrics.functionsMatched} matched to ground truth`
  );
  lines.push(
    `  Files: ${metrics.originalFileCount} original → ${metrics.splitFileCount} output (${fixed(metrics.fileCountRatio, 2)}x)`
  );
  lines.push(`  MQ Score: ${fixed(metrics.mqScore)}`);
  if (metrics.treeSimilarity !== undefined) {
    lines.push(`  Tree Similarity: ${fixed(metrics.treeSimilarity)}`);
  }

  // Clustering quality
  lines.push(`\n${divider()}`);
  lines.push("  Clustering Quality");
  lines.push(divider());
  lines.push(
    `  ARI:              ${rpad(fixed(metrics.ari), 8)}  (1.0 = perfect match)`
  );
  lines.push(
    `  V-Measure:        ${rpad(fixed(metrics.vMeasure), 8)}  (harmonic mean of H & C)`
  );
  lines.push(
    `    Homogeneity:    ${rpad(pct(metrics.homogeneity), 8)}  (each output file → few originals)`
  );
  lines.push(
    `    Completeness:   ${rpad(pct(metrics.completeness), 8)}  (each original → few outputs)`
  );
  lines.push(
    `  Purity:           ${rpad(pct(metrics.purity), 8)}  (avg dominant-class fraction)`
  );
  lines.push(
    `  Inverse Purity:   ${rpad(pct(metrics.inversePurity), 8)}  (avg dominant-output fraction)`
  );

  // Interpretation
  lines.push(`\n${divider()}`);
  lines.push("  Interpretation");
  lines.push(divider());

  if (metrics.completeness < 0.5) {
    lines.push(
      "  [!] Low completeness: functions from same source file are scattered across many output files."
    );
    lines.push(
      "      → The splitter is OVER-SPLITTING. Consider merging more clusters."
    );
  }
  if (metrics.homogeneity < 0.5) {
    lines.push(
      "  [!] Low homogeneity: output files contain functions from many different source files."
    );
    lines.push(
      "      → The splitter is UNDER-SPLITTING. Consider splitting clusters further."
    );
  }
  if (metrics.completeness >= 0.7 && metrics.homogeneity >= 0.7) {
    lines.push("  [OK] Good balance between homogeneity and completeness.");
  }

  // Timing
  lines.push(`\n${divider()}`);
  lines.push("  Timing");
  lines.push(divider());
  lines.push(`  Parse:    ${rpad(`${timing.parseMs}ms`, 10)}`);
  lines.push(`  Split:    ${rpad(`${timing.splitMs}ms`, 10)}`);
  lines.push(`  Metrics:  ${rpad(`${timing.metricsMs}ms`, 10)}`);
  lines.push(`  Total:    ${rpad(`${timing.totalMs}ms`, 10)}`);

  // Per-file breakdown (worst cases)
  lines.push(`\n${divider()}`);
  lines.push("  Most Fragmented Source Files (worst completeness)");
  lines.push(divider());

  const worstFiles = perFileBreakdown
    .filter((f) => f.functionCount >= 2)
    .slice(0, 15);

  if (worstFiles.length > 0) {
    lines.push(
      `  ${pad("Original File", 40)} ${rpad("Fns", 5)} ${rpad("Split→", 8)} ${rpad("Compl.", 8)}`
    );
    for (const f of worstFiles) {
      const file =
        f.originalFile.length > 38
          ? "..." + f.originalFile.slice(-35)
          : f.originalFile;
      lines.push(
        `  ${pad(file, 40)} ${rpad(String(f.functionCount), 5)} ${rpad(String(f.splitIntoFiles.length) + " files", 8)} ${rpad(pct(f.completeness), 8)}`
      );
    }
  }

  // Best cases
  const bestFiles = [...perFileBreakdown]
    .filter((f) => f.functionCount >= 2)
    .sort((a, b) => b.completeness - a.completeness)
    .slice(0, 5);

  if (bestFiles.length > 0) {
    lines.push(`\n  Best Split Source Files (highest completeness)`);
    lines.push(
      `  ${pad("Original File", 40)} ${rpad("Fns", 5)} ${rpad("Split→", 8)} ${rpad("Compl.", 8)}`
    );
    for (const f of bestFiles) {
      const file =
        f.originalFile.length > 38
          ? "..." + f.originalFile.slice(-35)
          : f.originalFile;
      lines.push(
        `  ${pad(file, 40)} ${rpad(String(f.functionCount), 5)} ${rpad(String(f.splitIntoFiles.length) + " files", 8)} ${rpad(pct(f.completeness), 8)}`
      );
    }
  }

  lines.push(`\n${divider("═")}\n`);

  return lines.join("\n");
}

/**
 * Format a comparison of two experiment results.
 */
export function formatComparison(
  baseline: ExperimentResult,
  improved: ExperimentResult
): string {
  const lines: string[] = [];
  const b = baseline.metrics;
  const i = improved.metrics;

  lines.push(`\n${divider("═")}`);
  lines.push(`  Comparison: ${baseline.fixture}`);
  lines.push(divider("═"));

  const delta = (val: number, base: number) => {
    const d = val - base;
    const sign = d >= 0 ? "+" : "";
    return `${sign}${fixed(d)}`;
  };

  lines.push(
    `\n  ${pad("Metric", 20)} ${rpad("Baseline", 10)} ${rpad("Improved", 10)} ${rpad("Delta", 10)}`
  );
  lines.push(`  ${divider("-", 50)}`);
  lines.push(
    `  ${pad("ARI", 20)} ${rpad(fixed(b.ari), 10)} ${rpad(fixed(i.ari), 10)} ${rpad(delta(i.ari, b.ari), 10)}`
  );
  lines.push(
    `  ${pad("V-Measure", 20)} ${rpad(fixed(b.vMeasure), 10)} ${rpad(fixed(i.vMeasure), 10)} ${rpad(delta(i.vMeasure, b.vMeasure), 10)}`
  );
  lines.push(
    `  ${pad("Homogeneity", 20)} ${rpad(pct(b.homogeneity), 10)} ${rpad(pct(i.homogeneity), 10)} ${rpad(delta(i.homogeneity, b.homogeneity), 10)}`
  );
  lines.push(
    `  ${pad("Completeness", 20)} ${rpad(pct(b.completeness), 10)} ${rpad(pct(i.completeness), 10)} ${rpad(delta(i.completeness, b.completeness), 10)}`
  );
  lines.push(
    `  ${pad("Purity", 20)} ${rpad(pct(b.purity), 10)} ${rpad(pct(i.purity), 10)} ${rpad(delta(i.purity, b.purity), 10)}`
  );
  lines.push(
    `  ${pad("File Count", 20)} ${rpad(String(b.splitFileCount), 10)} ${rpad(String(i.splitFileCount), 10)} ${rpad(String(i.splitFileCount - b.splitFileCount), 10)}`
  );
  lines.push(
    `  ${pad("File Ratio", 20)} ${rpad(fixed(b.fileCountRatio, 2) + "x", 10)} ${rpad(fixed(i.fileCountRatio, 2) + "x", 10)}`
  );

  lines.push(`\n${divider("═")}\n`);

  return lines.join("\n");
}

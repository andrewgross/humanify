/**
 * Post-run coverage diagnostics.
 *
 * Aggregates per-target rename reports into a summary showing
 * how many identifiers were renamed vs skipped and why, broken
 * down by strategy (LLM, library-prefix, fallback).
 */

import type { RenameReport } from "../analysis/types.js";
import type { ProcessingMetrics } from "../llm/metrics.js";

interface SkipReasons {
  zeroBindings: number;
  allPreserved: number;
  error: number;
}

export interface RenameCounts {
  total: number;
  llm: number;
  libraryPrefix: number;
  fallback: number;
  notRenamed: number;
  /** Functions that genuinely had nothing to rename (zero bindings + all descriptive + library-no-minified) */
  nothingToRename: number;
  /** Functions that failed (errors, LLM failures, unaccounted) */
  failed: number;
}

export interface CoverageSummary {
  functions: RenameCounts;
  moduleBindings: RenameCounts;
  identifiers: RenameCounts & { skippedBySkipList: number };
  llm?: {
    totalCalls: number;
    retries: number;
    avgResponseTimeMs: number;
    totalTokens?: number;
    inputTokens?: number;
    outputTokens?: number;
  };
  elapsedMs?: number;
}

function strategyKey(
  strategy: RenameReport["strategy"]
): "llm" | "libraryPrefix" | "fallback" {
  switch (strategy) {
    case "llm":
      return "llm";
    case "library-prefix":
      return "libraryPrefix";
    case "fallback":
      return "fallback";
  }
}

function emptyRenameCounts(): RenameCounts {
  return {
    total: 0,
    llm: 0,
    libraryPrefix: 0,
    fallback: 0,
    notRenamed: 0,
    nothingToRename: 0,
    failed: 0
  };
}

/** Count a function report: 1 function per report, bucketed by strategy. */
function countFunctionReport(counts: RenameCounts, report: RenameReport): void {
  if (report.renamedCount > 0) {
    counts[strategyKey(report.strategy)] += 1;
  }
}

/** Count a module-binding report: per-identifier counts. */
function countModuleBindingReport(
  counts: RenameCounts,
  report: RenameReport
): void {
  counts.total += report.totalIdentifiers;
  counts[strategyKey(report.strategy)] += report.renamedCount;
  counts.notRenamed += report.totalIdentifiers - report.renamedCount;
}

/** Count identifiers from a single report, bucketed by the report's strategy. */
function countIdentifiers(
  counts: RenameCounts & { skippedBySkipList: number },
  report: RenameReport
): void {
  const key = strategyKey(report.strategy);
  counts.total += report.totalIdentifiers;
  for (const outcome of Object.values(report.outcomes)) {
    if (outcome.status === "renamed") {
      counts[key] += 1;
    } else if (outcome.status !== "not-collected") {
      counts.notRenamed += 1;
    }
  }
}

/**
 * Build a coverage summary from rename reports.
 *
 * @param reports All rename reports collected during processing
 * @param totalFunctions Total function nodes in the graph
 * @param metrics Processing metrics from the LLM tracker
 * @param skippedBySkipList Number of identifiers skipped by skip-list (not eligible for rename)
 * @param skipReasons Why functions were skipped (zero bindings, all descriptive, errors)
 * @param libraryNoMinified Count of library functions with no minified bindings
 */
export function buildCoverageSummary(
  reports: ReadonlyArray<RenameReport>,
  totalFunctions: number,
  metrics?: ProcessingMetrics,
  skippedBySkipList?: number,
  skipReasons?: SkipReasons,
  libraryNoMinified?: number
): CoverageSummary {
  const functions: RenameCounts = {
    ...emptyRenameCounts(),
    total: totalFunctions
  };
  const moduleBindings = emptyRenameCounts();
  const identifiers = {
    ...emptyRenameCounts(),
    skippedBySkipList: skippedBySkipList ?? 0
  };

  for (const report of reports) {
    if (report.type === "module-binding") {
      countModuleBindingReport(moduleBindings, report);
    } else {
      countFunctionReport(functions, report);
    }
    countIdentifiers(identifiers, report);
  }

  // notRenamed = total - all strategy counts (functions without reports)
  functions.notRenamed = Math.max(
    0,
    totalFunctions -
      functions.llm -
      functions.libraryPrefix -
      functions.fallback
  );

  // Break down notRenamed into nothingToRename vs failed
  const nothingToRename =
    (skipReasons?.zeroBindings ?? 0) +
    (skipReasons?.allPreserved ?? 0) +
    (libraryNoMinified ?? 0);
  functions.nothingToRename = Math.min(nothingToRename, functions.notRenamed);
  functions.failed = Math.max(
    0,
    functions.notRenamed - functions.nothingToRename
  );

  const summary: CoverageSummary = { functions, moduleBindings, identifiers };

  if (metrics) {
    summary.llm = {
      totalCalls: metrics.llm.completedCalls,
      retries: metrics.llm.retries,
      avgResponseTimeMs: metrics.llm.avgResponseTimeMs,
      totalTokens: metrics.llm.totalTokens,
      inputTokens: metrics.llm.inputTokens,
      outputTokens: metrics.llm.outputTokens
    };
    summary.elapsedMs = metrics.elapsedMs;
  }

  return summary;
}

/** Push a sub-line with count and percentage if count > 0. */
function pushCountLine(
  lines: string[],
  sublabel: string,
  count: number,
  total: number,
  labelWidth: number
): void {
  if (count <= 0) return;
  const pct = total > 0 ? ((count / total) * 100).toFixed(1) : "0.0";
  lines.push(`   ${sublabel.padEnd(labelWidth - 2)}${fmt(count)}  (${pct}%)`);
}

function formatSection(
  label: string,
  counts: RenameCounts,
  labelWidth: number
): string[] {
  const lines: string[] = [];
  lines.push(` ${label.padEnd(labelWidth)}${fmt(counts.total)} total`);

  pushCountLine(lines, "LLM:", counts.llm, counts.total, labelWidth);
  pushCountLine(
    lines,
    "Library prefix:",
    counts.libraryPrefix,
    counts.total,
    labelWidth
  );
  pushCountLine(lines, "Fallback:", counts.fallback, counts.total, labelWidth);
  pushCountLine(
    lines,
    "Nothing to rename:",
    counts.nothingToRename,
    counts.total,
    labelWidth
  );
  pushCountLine(lines, "Failed:", counts.failed, counts.total, labelWidth);

  if (
    counts.notRenamed > 0 &&
    counts.nothingToRename === 0 &&
    counts.failed === 0
  ) {
    // Fallback for sections without skip-reason data (module bindings, identifiers)
    lines.push(
      `   ${"Not renamed:".padEnd(labelWidth - 2)}${fmt(counts.notRenamed)}`
    );
  }
  return lines;
}

/**
 * Format a coverage summary as a human-readable block.
 */
export function formatCoverageSummary(summary: CoverageSummary): string {
  const lines: string[] = [];
  const sep = "\u2500";
  const labelWidth = 18;

  lines.push(` ${sep}${sep} Coverage Summary ${sep.repeat(60)}`);

  if (summary.functions.total > 0) {
    for (const line of formatSection(
      "Functions:",
      summary.functions,
      labelWidth
    )) {
      lines.push(line);
    }
  }

  if (summary.moduleBindings.total > 0) {
    for (const line of formatSection(
      "Module bindings:",
      summary.moduleBindings,
      labelWidth
    )) {
      lines.push(line);
    }
  }

  if (summary.identifiers.total > 0) {
    for (const line of formatSection(
      "Identifiers:",
      summary.identifiers,
      labelWidth
    )) {
      lines.push(line);
    }
    if (summary.identifiers.skippedBySkipList > 0) {
      lines.push(
        `   ${"Skipped (skip-list):".padEnd(labelWidth - 2)}${fmt(summary.identifiers.skippedBySkipList)}`
      );
    }
  }

  if (summary.llm) {
    for (const line of formatLlmSection(summary.llm)) {
      lines.push(line);
    }
  }

  if (summary.elapsedMs) {
    lines.push(` Time:             ${fmtDuration(summary.elapsedMs)} elapsed`);
  }

  return lines.join("\n");
}

function formatLlmSection(llm: NonNullable<CoverageSummary["llm"]>): string[] {
  const lines: string[] = [];
  const llmParts = [`${fmt(llm.totalCalls).trim()} calls`];
  if (llm.retries > 0) llmParts.push(`${llm.retries} retries`);
  llmParts.push(`avg ${llm.avgResponseTimeMs}ms`);
  lines.push(` LLM:              ${llmParts.join(", ")}`);

  if (llm.totalTokens) {
    if (llm.inputTokens && llm.outputTokens) {
      lines.push(
        ` Tokens:           ${fmtTokens(llm.totalTokens)} total (${fmtTokens(llm.inputTokens)} input / ${fmtTokens(llm.outputTokens)} output)`
      );
    } else {
      lines.push(` Tokens:           ${fmtTokens(llm.totalTokens)} total`);
    }
  }
  return lines;
}

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function fmtDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.round((ms % 60000) / 1000);
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remainMins = mins % 60;
  return `${hours}h ${remainMins}m`;
}

function fmt(n: number): string {
  return n.toLocaleString("en-US").padStart(8);
}

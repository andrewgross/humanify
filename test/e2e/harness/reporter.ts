import type { ValidationResult, ValidationFailure } from "./validate.js";

export interface ReportOptions {
  /** Output debug directory path (for debug hints) */
  debugDir?: string;
  /** Verbose mode - show all details */
  verbose?: boolean;
}

export function reportResults(result: ValidationResult, options: ReportOptions = {}): void {
  const { metrics } = result;

  console.log("");
  printHeader(`E2E Validation: ${result.fixture} ${result.v1} → ${result.v2} (${result.minifierConfig})`);
  console.log("");

  // Ground truth summary
  printSection("Ground Truth");
  console.log(`  ${result.v1FunctionCount} functions in v1, ${result.v2FunctionCount} functions in v2`);

  const unchanged = metrics.unchangedFunctions.total;
  const modified = metrics.modifiedFunctions.total;
  const added = metrics.addedFunctions.total;
  const removed = metrics.removedFunctions.total;
  console.log(`  ${unchanged} unchanged, ${modified} modified, ${added} added, ${removed} removed`);
  console.log("");

  // Fingerprint matching results
  printSection("Fingerprint Matching");

  const unchangedPct = unchanged > 0
    ? `${metrics.unchangedFunctions.fingerprintsMatched}/${unchanged} matched (${pct(result.cacheReuseAccuracy)})`
    : "N/A";
  console.log(`  Unchanged: ${unchangedPct}`);

  const modifiedPct = modified > 0
    ? `${metrics.modifiedFunctions.fingerprintsDiffered}/${modified} detected (${pct(result.changeDetectionAccuracy)})`
    : "N/A";
  console.log(`  Modified:  ${modifiedPct}`);

  const addedPct = added > 0
    ? `${metrics.addedFunctions.noMatchFound}/${added} no false match (${pct(metrics.addedFunctions.noMatchFound / added)})`
    : "N/A";
  console.log(`  Added:     ${addedPct}`);

  console.log("");

  // Overall result
  const passed = result.failures.length === 0;
  const status = passed ? "\x1b[32mPASS\x1b[0m" : "\x1b[31mFAIL\x1b[0m";
  console.log(`Overall: ${status} (${pct(result.overallAccuracy)})`);

  // List failures if any
  if (result.failures.length > 0) {
    console.log("");
    printSection(`Failures (${result.failures.length})`);

    for (const failure of result.failures) {
      printFailure(failure, options.verbose);
    }

    if (options.debugDir) {
      console.log("");
      console.log(`Debug artifacts written to: ${options.debugDir}`);
    }
  }

  console.log("");
}

/**
 * Print a detailed failure summary.
 */
function printFailure(failure: ValidationFailure, verbose?: boolean): void {
  const typeLabel = formatFailureType(failure.type);
  console.log("");
  console.log(`  ${typeLabel}`);
  console.log(`  Function: ${failure.sourceName} (${failure.sourceFile})`);
  console.log(`  Expected: ${failure.expected}`);
  console.log(`  Actual:   ${failure.actual}`);

  if (verbose) {
    console.log("");
    console.log(`  Hint: ${getFailureHint(failure.type)}`);
  }
}

/**
 * Format failure type with color.
 */
function formatFailureType(type: string): string {
  const colors: Record<string, string> = {
    "unchanged-but-fingerprint-mismatch": "\x1b[33m", // yellow
    "modified-but-fingerprint-match": "\x1b[35m",     // magenta
    "added-but-false-match": "\x1b[31m",              // red
  };
  const color = colors[type] || "\x1b[0m";
  return `${color}[${type}]\x1b[0m`;
}

/**
 * Get a brief hint for a failure type.
 */
function getFailureHint(type: string): string {
  switch (type) {
    case "unchanged-but-fingerprint-mismatch":
      return "Function unchanged in source but fingerprints differ - check for minifier variance or source map issues";
    case "modified-but-fingerprint-match":
      return "Function modified but fingerprints match - fingerprinting may be too loose for this change type";
    case "added-but-false-match":
      return "New function matched to old one - fingerprinting may be too loose or functions are structurally similar";
    default:
      return "Check debug artifacts for details";
  }
}

/**
 * Print a section header.
 */
function printSection(title: string): void {
  console.log(`\x1b[1m${title}:\x1b[0m`);
}

/**
 * Print a major header.
 */
function printHeader(title: string): void {
  const line = "─".repeat(title.length + 4);
  console.log(`┌${line}┐`);
  console.log(`│  ${title}  │`);
  console.log(`└${line}┘`);
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`;
}

/**
 * Generate a CI-friendly report (no colors, machine-parseable).
 */
export function reportResultsCI(result: ValidationResult): { passed: boolean; summary: string } {
  const { metrics } = result;
  const passed = result.failures.length === 0;

  const lines: string[] = [];
  lines.push(`E2E: ${result.fixture} ${result.v1}->${result.v2} (${result.minifierConfig})`);
  lines.push(`  Unchanged: ${metrics.unchangedFunctions.fingerprintsMatched}/${metrics.unchangedFunctions.total}`);
  lines.push(`  Modified: ${metrics.modifiedFunctions.fingerprintsDiffered}/${metrics.modifiedFunctions.total}`);
  lines.push(`  Added: ${metrics.addedFunctions.noMatchFound}/${metrics.addedFunctions.total}`);
  lines.push(`  Overall: ${passed ? "PASS" : "FAIL"} (${pct(result.overallAccuracy)})`);

  if (!passed) {
    lines.push(`  Failures: ${result.failures.length}`);
    for (const failure of result.failures) {
      lines.push(`    - [${failure.type}] ${failure.sourceName}`);
    }
  }

  return { passed, summary: lines.join("\n") };
}

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
  console.log(`  ${result.v1SourceFunctionCount} source functions in v1, ${result.v2SourceFunctionCount} in v2`);

  const unchanged = metrics.unchangedFunctions.total;
  const modified = metrics.modifiedFunctions.total;
  const added = metrics.addedFunctions.total;
  const removed = metrics.removedFunctions.total;
  console.log(`  ${unchanged} unchanged, ${modified} modified, ${added} added, ${removed} removed`);

  // Show fingerprint counts if different from source counts
  if (result.v1FingerprintCount !== result.v1SourceFunctionCount ||
      result.v2FingerprintCount !== result.v2SourceFunctionCount) {
    console.log(`  (${result.v1FingerprintCount} fingerprints in v1, ${result.v2FingerprintCount} in v2 — includes wrappers)`);
  }
  console.log("");

  // Fingerprint matching results
  printSection("Fingerprint Matching");

  const unchangedPct = unchanged > 0
    ? `${metrics.unchangedFunctions.fingerprintsMatched}/${unchanged} matched (${pct(result.cacheReuseAccuracy)})`
    : "N/A";
  console.log(`  Unchanged: ${unchangedPct}`);

  if (modified > 0) {
    const syntactic = metrics.modifiedFunctions.syntacticOnly;
    const detected = metrics.modifiedFunctions.fingerprintsDiffered;
    let modifiedStr = `${detected}/${modified} detected`;
    if (syntactic > 0) {
      modifiedStr += `, ${syntactic} syntactic-only`;
    }
    console.log(`  Modified:  ${modifiedStr} (${pct(result.changeDetectionAccuracy)})`);
  } else {
    console.log(`  Modified:  N/A`);
  }

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

export interface AggregateEntry {
  pair: string;
  minifier: string;
  accuracy: number;
  passed: boolean;
}

/**
 * Print an aggregate summary table across all minifiers for a fixture.
 */
export function reportAggregateSummary(
  fixture: string,
  entries: AggregateEntry[]
): void {
  // Collect unique pairs and minifiers preserving order
  const pairs: string[] = [];
  const minifiers: string[] = [];
  for (const e of entries) {
    if (!pairs.includes(e.pair)) pairs.push(e.pair);
    if (!minifiers.includes(e.minifier)) minifiers.push(e.minifier);
  }

  // Build lookup
  const lookup = new Map<string, AggregateEntry>();
  for (const e of entries) {
    lookup.set(`${e.pair}::${e.minifier}`, e);
  }

  // Column widths
  const pairColWidth = Math.max(4, ...pairs.map(p => p.length));
  const minColWidth = Math.max(7, ...minifiers.map(m => m.length));
  const overallColWidth = 7;

  const padR = (s: string, w: number) => s + " ".repeat(Math.max(0, w - s.length));
  const padC = (s: string, w: number) => {
    const total = Math.max(0, w - s.length);
    const left = Math.floor(total / 2);
    const right = total - left;
    return " ".repeat(left) + s + " ".repeat(right);
  };

  // Header
  console.log("");
  const title = `Aggregate: ${fixture} (${pairs.length} pairs \u00d7 ${minifiers.length} minifiers)`;
  const titleLine = "\u2500".repeat(title.length + 4);
  console.log(`\u250c${titleLine}\u2510`);
  console.log(`\u2502  ${title}  \u2502`);

  // Column header row
  const sep = (cols: string[], edge: [string, string, string]) =>
    edge[0] + cols.join(edge[1]) + edge[2];

  const colWidths = [pairColWidth, ...minifiers.map(() => minColWidth), overallColWidth];
  const headerCells = [
    padR("Pair", pairColWidth),
    ...minifiers.map(m => padC(m, minColWidth)),
    padC("Overall", overallColWidth),
  ];

  const hRule = (l: string, m: string, r: string) =>
    l + colWidths.map(w => "\u2500".repeat(w + 2)).join(m) + r;

  console.log(hRule("\u251c", "\u252c", "\u2524"));
  console.log("\u2502 " + headerCells.join(" \u2502 ") + " \u2502");
  console.log(hRule("\u251c", "\u253c", "\u2524"));

  // Data rows
  for (const pair of pairs) {
    const cells: string[] = [padR(pair, pairColWidth)];
    let allPassed = true;

    for (const min of minifiers) {
      const entry = lookup.get(`${pair}::${min}`);
      if (entry) {
        cells.push(padC(pct(entry.accuracy), minColWidth));
        if (!entry.passed) allPassed = false;
      } else {
        cells.push(padC("-", minColWidth));
        allPassed = false;
      }
    }

    const overallStr = allPassed ? "\x1b[32mPASS\x1b[0m" : "\x1b[31mFAIL\x1b[0m";
    // Pad accounting for ANSI escape codes (9 chars of escape, 4 chars visible)
    const overallPadded = padC(overallStr, overallColWidth + 9);
    cells.push(overallPadded);

    console.log("\u2502 " + cells.join(" \u2502 ") + " \u2502");
  }

  console.log(hRule("\u2514", "\u2534", "\u2518"));
  console.log("");
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

import type { ValidationResult } from "./validate.js";

export function reportResults(result: ValidationResult): void {
  const { metrics } = result;

  console.log("");
  console.log(`E2E Validation: ${result.fixture} ${result.v1} → ${result.v2} (${result.minifierConfig})`);
  console.log("");

  // Ground truth summary
  console.log("Ground Truth:");
  console.log(`  ${result.v1FunctionCount} functions in v1, ${result.v2FunctionCount} functions in v2`);

  const unchanged = metrics.unchangedFunctions.total;
  const modified = metrics.modifiedFunctions.total;
  const added = metrics.addedFunctions.total;
  const removed = metrics.removedFunctions.total;
  console.log(`  ${unchanged} unchanged, ${modified} modified, ${added} added, ${removed} removed`);
  console.log("");

  // Fingerprint matching results
  console.log("Fingerprint Matching:");

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
  console.log(`Overall: ${passed ? "PASS" : "FAIL"} (${pct(result.overallAccuracy)})`);

  // List failures if any
  if (result.failures.length > 0) {
    console.log("");
    console.log("Failures:");
    for (const failure of result.failures) {
      console.log(`  [${failure.type}] ${failure.sourceName} (${failure.sourceFile})`);
      console.log(`    Expected: ${failure.expected}`);
      console.log(`    Actual:   ${failure.actual}`);
    }
  }

  console.log("");
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`;
}

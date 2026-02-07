import assert from "node:assert";
import { compareToSnapshot } from "./snapshot.js";
import type { ValidationResult } from "./validate.js";

/**
 * Assert that a ValidationResult matches its stored snapshot.
 *
 * Fails if no snapshot exists (run `--update-snapshot` first) or if any
 * metrics have drifted from the baseline.
 */
export function assertSnapshotMatch(result: ValidationResult): void {
  const comparison = compareToSnapshot(result);

  assert.ok(
    comparison.exists,
    `Snapshot not found for ${result.fixture} ${result.v1}→${result.v2} (${result.minifierConfig}). ` +
      `Run: npm run e2e -- validate ${result.fixture} ${result.v1} ${result.v2} --minifier ${result.minifierConfig} --update-snapshot`
  );

  assert.strictEqual(
    comparison.diffs.length,
    0,
    `Snapshot drift:\n${comparison.diffs.map((d) => `  ${d.field}: ${d.expected} → ${d.actual}`).join("\n")}`
  );
}

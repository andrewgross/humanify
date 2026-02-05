import { existsSync, readFileSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import type { ValidationResult } from "./validate.js";

/**
 * Snapshot data structure - serializable subset of ValidationResult.
 */
export interface Snapshot {
  fixture: string;
  v1: string;
  v2: string;
  minifierConfig: string;
  timestamp: string;

  v1FingerprintCount: number;
  v2FingerprintCount: number;
  v1SourceFunctionCount: number;
  v2SourceFunctionCount: number;
  groundTruthCorrespondences: number;

  metrics: ValidationResult["metrics"];

  cacheReuseAccuracy: number;
  changeDetectionAccuracy: number;
  overallAccuracy: number;

  // Store failure types and counts, not full details
  failureSummary: {
    total: number;
    byType: Record<string, number>;
  };
}

/**
 * Diff between two snapshots.
 */
export interface SnapshotDiff {
  field: string;
  expected: string | number;
  actual: string | number;
}

/**
 * Get the snapshot file path for a validation run.
 */
export function getSnapshotPath(
  fixture: string,
  v1: string,
  v2: string,
  minifierConfig: string
): string {
  const snapshotsDir = join(import.meta.dirname, "..", "snapshots", fixture);
  return join(snapshotsDir, `v${v1}-v${v2}-${minifierConfig}.snapshot.json`);
}

/**
 * Convert a ValidationResult to a Snapshot (stable, serializable format).
 */
export function resultToSnapshot(result: ValidationResult): Snapshot {
  // Count failures by type
  const byType: Record<string, number> = {};
  for (const failure of result.failures) {
    byType[failure.type] = (byType[failure.type] || 0) + 1;
  }

  return {
    fixture: result.fixture,
    v1: result.v1,
    v2: result.v2,
    minifierConfig: result.minifierConfig,
    timestamp: new Date().toISOString(),

    v1FingerprintCount: result.v1FingerprintCount,
    v2FingerprintCount: result.v2FingerprintCount,
    v1SourceFunctionCount: result.v1SourceFunctionCount,
    v2SourceFunctionCount: result.v2SourceFunctionCount,
    groundTruthCorrespondences: result.groundTruthCorrespondences,

    metrics: result.metrics,

    cacheReuseAccuracy: result.cacheReuseAccuracy,
    changeDetectionAccuracy: result.changeDetectionAccuracy,
    overallAccuracy: result.overallAccuracy,

    failureSummary: {
      total: result.failures.length,
      byType,
    },
  };
}

/**
 * Save a snapshot to disk.
 */
export function saveSnapshot(result: ValidationResult): string {
  const snapshotPath = getSnapshotPath(
    result.fixture,
    result.v1,
    result.v2,
    result.minifierConfig
  );

  mkdirSync(join(snapshotPath, ".."), { recursive: true });

  const snapshot = resultToSnapshot(result);
  writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2));

  return snapshotPath;
}

/**
 * Load a snapshot from disk.
 */
export function loadSnapshot(
  fixture: string,
  v1: string,
  v2: string,
  minifierConfig: string
): Snapshot | null {
  const snapshotPath = getSnapshotPath(fixture, v1, v2, minifierConfig);

  if (!existsSync(snapshotPath)) {
    return null;
  }

  return JSON.parse(readFileSync(snapshotPath, "utf-8"));
}

/**
 * Compare a result against a saved snapshot.
 * Returns null if no snapshot exists, or list of differences.
 */
export function compareToSnapshot(result: ValidationResult): {
  exists: boolean;
  diffs: SnapshotDiff[];
  snapshotPath: string;
} {
  const snapshotPath = getSnapshotPath(
    result.fixture,
    result.v1,
    result.v2,
    result.minifierConfig
  );

  const snapshot = loadSnapshot(
    result.fixture,
    result.v1,
    result.v2,
    result.minifierConfig
  );

  if (!snapshot) {
    return { exists: false, diffs: [], snapshotPath };
  }

  const diffs: SnapshotDiff[] = [];
  const currentSnapshot = resultToSnapshot(result);

  // Compare key metrics
  if (snapshot.v1SourceFunctionCount !== currentSnapshot.v1SourceFunctionCount) {
    diffs.push({
      field: "v1SourceFunctionCount",
      expected: snapshot.v1SourceFunctionCount,
      actual: currentSnapshot.v1SourceFunctionCount,
    });
  }

  if (snapshot.v2SourceFunctionCount !== currentSnapshot.v2SourceFunctionCount) {
    diffs.push({
      field: "v2SourceFunctionCount",
      expected: snapshot.v2SourceFunctionCount,
      actual: currentSnapshot.v2SourceFunctionCount,
    });
  }

  if (snapshot.v1FingerprintCount !== currentSnapshot.v1FingerprintCount) {
    diffs.push({
      field: "v1FingerprintCount",
      expected: snapshot.v1FingerprintCount,
      actual: currentSnapshot.v1FingerprintCount,
    });
  }

  if (snapshot.v2FingerprintCount !== currentSnapshot.v2FingerprintCount) {
    diffs.push({
      field: "v2FingerprintCount",
      expected: snapshot.v2FingerprintCount,
      actual: currentSnapshot.v2FingerprintCount,
    });
  }

  // Compare metrics
  const compareMetric = (
    path: string,
    expected: number,
    actual: number
  ) => {
    if (expected !== actual) {
      diffs.push({ field: path, expected, actual });
    }
  };

  compareMetric(
    "metrics.unchangedFunctions.fingerprintsMatched",
    snapshot.metrics.unchangedFunctions.fingerprintsMatched,
    currentSnapshot.metrics.unchangedFunctions.fingerprintsMatched
  );
  compareMetric(
    "metrics.unchangedFunctions.fingerprintsMismatched",
    snapshot.metrics.unchangedFunctions.fingerprintsMismatched,
    currentSnapshot.metrics.unchangedFunctions.fingerprintsMismatched
  );
  compareMetric(
    "metrics.modifiedFunctions.fingerprintsDiffered",
    snapshot.metrics.modifiedFunctions.fingerprintsDiffered,
    currentSnapshot.metrics.modifiedFunctions.fingerprintsDiffered
  );
  compareMetric(
    "metrics.modifiedFunctions.fingerprintsMatched",
    snapshot.metrics.modifiedFunctions.fingerprintsMatched,
    currentSnapshot.metrics.modifiedFunctions.fingerprintsMatched
  );
  compareMetric(
    "metrics.addedFunctions.noMatchFound",
    snapshot.metrics.addedFunctions.noMatchFound,
    currentSnapshot.metrics.addedFunctions.noMatchFound
  );
  compareMetric(
    "metrics.addedFunctions.falseMatchFound",
    snapshot.metrics.addedFunctions.falseMatchFound,
    currentSnapshot.metrics.addedFunctions.falseMatchFound
  );

  // Compare accuracy scores
  if (Math.abs(snapshot.overallAccuracy - currentSnapshot.overallAccuracy) > 0.001) {
    diffs.push({
      field: "overallAccuracy",
      expected: snapshot.overallAccuracy,
      actual: currentSnapshot.overallAccuracy,
    });
  }

  // Compare failure counts
  if (snapshot.failureSummary.total !== currentSnapshot.failureSummary.total) {
    diffs.push({
      field: "failureSummary.total",
      expected: snapshot.failureSummary.total,
      actual: currentSnapshot.failureSummary.total,
    });
  }

  return { exists: true, diffs, snapshotPath };
}

/**
 * Print snapshot comparison results.
 */
export function reportSnapshotComparison(comparison: {
  exists: boolean;
  diffs: SnapshotDiff[];
  snapshotPath: string;
}): boolean {
  if (!comparison.exists) {
    console.log(`\nSnapshot not found: ${comparison.snapshotPath}`);
    console.log("Run with --update-snapshot to create it.");
    return false;
  }

  if (comparison.diffs.length === 0) {
    console.log("\nSnapshot comparison: \x1b[32mMATCH\x1b[0m");
    return true;
  }

  console.log("\nSnapshot comparison: \x1b[31mDRIFT DETECTED\x1b[0m");
  console.log(`Snapshot: ${comparison.snapshotPath}`);
  console.log("");
  console.log("Differences:");
  for (const diff of comparison.diffs) {
    console.log(`  ${diff.field}:`);
    console.log(`    expected: ${diff.expected}`);
    console.log(`    actual:   ${diff.actual}`);
  }
  console.log("");
  console.log("Run with --update-snapshot to update the snapshot.");

  return false;
}

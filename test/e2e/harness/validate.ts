import { parseSync } from "@babel/core";
import { SourceMapConsumer } from "source-map";
import { buildFunctionGraph } from "../../../src/analysis/function-graph.js";
import { buildFingerprintIndex, matchFunctions, findNewFunctions } from "../../../src/analysis/fingerprint-index.js";
import type { FunctionNode, FingerprintIndex, MatchResult } from "../../../src/analysis/types.js";
import type { GroundTruth, FunctionCorrespondence, SourceFunction } from "./ground-truth.js";

export interface ValidationFailure {
  type:
    | "unchanged-but-fingerprint-mismatch"
    | "modified-but-fingerprint-match"
    | "added-but-false-match";
  sourceName: string;
  sourceFile: string;
  expected: string;
  actual: string;
}

export interface ValidationMetrics {
  unchangedFunctions: {
    total: number;
    fingerprintsMatched: number;
    fingerprintsMismatched: number;
  };
  modifiedFunctions: {
    total: number;
    fingerprintsDiffered: number;
    fingerprintsMatched: number;
  };
  addedFunctions: {
    total: number;
    noMatchFound: number;
    falseMatchFound: number;
  };
  removedFunctions: {
    total: number;
  };
}

export interface ValidationResult {
  fixture: string;
  v1: string;
  v2: string;
  minifierConfig: string;
  // Fingerprint counts (all functions found in minified code)
  v1FingerprintCount: number;
  v2FingerprintCount: number;
  // Ground truth counts (source functions we're tracking)
  v1SourceFunctionCount: number;
  v2SourceFunctionCount: number;
  groundTruthCorrespondences: number;
  metrics: ValidationMetrics;
  cacheReuseAccuracy: number;
  changeDetectionAccuracy: number;
  overallAccuracy: number;
  failures: ValidationFailure[];
}

/**
 * Parse minified code and build fingerprint structures.
 */
export function buildFingerprintData(code: string, filePath: string): {
  functions: Map<string, FunctionNode>;
  index: FingerprintIndex;
} {
  const ast = parseSync(code, {
    filename: filePath,
    sourceType: "module",
    plugins: [],
  });

  if (!ast) {
    throw new Error(`Failed to parse minified code from ${filePath}`);
  }

  const functionNodes = buildFunctionGraph(ast, filePath);
  const functions = new Map<string, FunctionNode>();
  for (const fn of functionNodes) {
    functions.set(fn.sessionId, fn);
  }

  const index = buildFingerprintIndex(functions);
  return { functions, index };
}

/**
 * Link minified functions back to source functions using source maps.
 */
export async function linkMinifiedToSource(
  minifiedFunctions: Map<string, FunctionNode>,
  sourceFunctions: SourceFunction[],
  rawSourceMap: object
): Promise<Map<string, string>> {
  const links = new Map<string, string>();

  const consumer = await new SourceMapConsumer(rawSourceMap as any);

  try {
    for (const [sessionId, fn] of minifiedFunctions) {
      const loc = fn.path.node.loc;
      if (!loc) continue;

      const originalPos = consumer.originalPositionFor({
        line: loc.start.line,
        column: loc.start.column,
      });

      if (originalPos.line !== null) {
        // Find the most specific source function at this original position
        // (smallest line range that contains the position)
        const candidates = sourceFunctions.filter(
          (sFn) =>
            sFn.location.startLine <= originalPos.line! &&
            sFn.location.endLine >= originalPos.line!
        );

        if (candidates.length > 0) {
          // Sort by range size (smallest first) to get most specific function
          candidates.sort(
            (a, b) =>
              (a.location.endLine - a.location.startLine) -
              (b.location.endLine - b.location.startLine)
          );
          links.set(sessionId, candidates[0].id);
        }
      }
    }
  } finally {
    consumer.destroy();
  }

  return links;
}

/**
 * Run the full validation pipeline: compare fingerprint matching results
 * against ground truth.
 */
export function validate(
  fixture: string,
  v1Version: string,
  v2Version: string,
  minifierConfigId: string,
  groundTruth: GroundTruth,
  v1Index: FingerprintIndex,
  v2Index: FingerprintIndex,
  matchResult: MatchResult,
  v1Links: Map<string, string>, // minifiedId → sourceId
  v2Links: Map<string, string>  // minifiedId → sourceId
): ValidationResult {
  const failures: ValidationFailure[] = [];

  const metrics: ValidationMetrics = {
    unchangedFunctions: { total: 0, fingerprintsMatched: 0, fingerprintsMismatched: 0 },
    modifiedFunctions: { total: 0, fingerprintsDiffered: 0, fingerprintsMatched: 0 },
    addedFunctions: { total: 0, noMatchFound: 0, falseMatchFound: 0 },
    removedFunctions: { total: 0 },
  };

  // Build reverse map: sourceId → minifiedId for both versions
  const v1SourceToMinified = invertMap(v1Links);
  const v2SourceToMinified = invertMap(v2Links);

  // Also build reverse match map: newMinifiedId → oldMinifiedId
  const reverseMatches = new Map<string, string>();
  for (const [oldId, newId] of matchResult.matches) {
    reverseMatches.set(newId, oldId);
  }

  // Find new functions (in v2 but not matched to any v1)
  const newV2Functions = new Set(findNewFunctions(v1Index, v2Index, matchResult));

  for (const corr of groundTruth.correspondence) {
    switch (corr.changeType) {
      case "unchanged": {
        metrics.unchangedFunctions.total++;

        const v1MinId = v1SourceToMinified.get(`${corr.sourceFile}::${corr.sourceName}`);
        const v2MinId = v2SourceToMinified.get(`${corr.sourceFile}::${corr.sourceName}`);

        if (!v1MinId || !v2MinId) {
          // Can't validate — function not linked to minified output
          metrics.unchangedFunctions.fingerprintsMismatched++;
          failures.push({
            type: "unchanged-but-fingerprint-mismatch",
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: "Fingerprints should match (function unchanged)",
            actual: `Could not link to minified output (v1: ${v1MinId ? "found" : "missing"}, v2: ${v2MinId ? "found" : "missing"})`,
          });
          break;
        }

        // Check if v1 matched to v2 in the fingerprint matching
        const matchedNewId = matchResult.matches.get(v1MinId);
        if (matchedNewId === v2MinId) {
          metrics.unchangedFunctions.fingerprintsMatched++;
        } else {
          metrics.unchangedFunctions.fingerprintsMismatched++;
          failures.push({
            type: "unchanged-but-fingerprint-mismatch",
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: "Fingerprints should match (function unchanged)",
            actual: matchedNewId
              ? `Matched to wrong function: ${matchedNewId}`
              : "No fingerprint match found",
          });
        }
        break;
      }

      case "modified": {
        metrics.modifiedFunctions.total++;

        const v1MinId = v1SourceToMinified.get(`${corr.sourceFile}::${corr.sourceName}`);
        const v2MinId = v2SourceToMinified.get(`${corr.sourceFile}::${corr.sourceName}`);

        if (!v1MinId || !v2MinId) {
          // Can't link — count as "differed" since we can't confirm match
          metrics.modifiedFunctions.fingerprintsDiffered++;
          break;
        }

        const matchedNewId = matchResult.matches.get(v1MinId);
        if (matchedNewId === v2MinId) {
          // Fingerprints matched despite modification
          metrics.modifiedFunctions.fingerprintsMatched++;
          if (corr.changeDetails?.bodyChanged) {
            failures.push({
              type: "modified-but-fingerprint-match",
              sourceName: corr.sourceName,
              sourceFile: corr.sourceFile,
              expected: "Fingerprints should differ (function body modified)",
              actual: "Fingerprints matched despite modification",
            });
          }
        } else {
          metrics.modifiedFunctions.fingerprintsDiffered++;
        }
        break;
      }

      case "added": {
        metrics.addedFunctions.total++;

        const v2MinId = v2SourceToMinified.get(`${corr.sourceFile}::${corr.sourceName}`);

        if (!v2MinId) {
          metrics.addedFunctions.noMatchFound++;
          break;
        }

        // Check if this v2 function was falsely matched to some v1 function
        if (newV2Functions.has(v2MinId)) {
          metrics.addedFunctions.noMatchFound++;
        } else {
          // It got matched to something in v1 — false match
          const matchedFrom = reverseMatches.get(v2MinId);
          metrics.addedFunctions.falseMatchFound++;
          failures.push({
            type: "added-but-false-match",
            sourceName: corr.sourceName,
            sourceFile: corr.sourceFile,
            expected: "No match (function is new in v2)",
            actual: `Falsely matched to v1 function: ${matchedFrom ?? "unknown"}`,
          });
        }
        break;
      }

      case "removed": {
        metrics.removedFunctions.total++;
        break;
      }
    }
  }

  // Compute summary scores
  const cacheReuseAccuracy =
    metrics.unchangedFunctions.total > 0
      ? metrics.unchangedFunctions.fingerprintsMatched / metrics.unchangedFunctions.total
      : 1;

  const changeDetectionAccuracy =
    metrics.modifiedFunctions.total > 0
      ? metrics.modifiedFunctions.fingerprintsDiffered / metrics.modifiedFunctions.total
      : 1;

  const totalChecked =
    metrics.unchangedFunctions.total +
    metrics.modifiedFunctions.total +
    metrics.addedFunctions.total;

  const totalCorrect =
    metrics.unchangedFunctions.fingerprintsMatched +
    metrics.modifiedFunctions.fingerprintsDiffered +
    metrics.addedFunctions.noMatchFound;

  const overallAccuracy = totalChecked > 0 ? totalCorrect / totalChecked : 1;

  return {
    fixture,
    v1: v1Version,
    v2: v2Version,
    minifierConfig: minifierConfigId,
    v1FingerprintCount: v1Index.fingerprints.size,
    v2FingerprintCount: v2Index.fingerprints.size,
    v1SourceFunctionCount: groundTruth.v1Functions.length,
    v2SourceFunctionCount: groundTruth.v2Functions.length,
    groundTruthCorrespondences: groundTruth.correspondence.length,
    metrics,
    cacheReuseAccuracy,
    changeDetectionAccuracy,
    overallAccuracy,
    failures,
  };
}

function invertMap(map: Map<string, string>): Map<string, string> {
  const inverted = new Map<string, string>();
  for (const [key, value] of map) {
    inverted.set(value, key);
  }
  return inverted;
}

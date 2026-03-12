import { parseSync } from "@babel/core";
import { SourceMapConsumer } from "source-map";
import { buildFunctionGraph } from "../../../src/analysis/function-graph.js";
import {
  buildFingerprintIndex,
  findNewFunctions
} from "../../../src/analysis/fingerprint-index.js";
import type {
  FunctionNode,
  FingerprintIndex,
  MatchResult
} from "../../../src/analysis/types.js";
import type { GroundTruth, SourceFunction } from "./ground-truth.js";

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
    /** Source-level modifications that produce identical minified output (expected match). */
    syntacticOnly: number;
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
export function buildFingerprintData(
  code: string,
  filePath: string
): {
  functions: Map<string, FunctionNode>;
  index: FingerprintIndex;
} {
  const ast = parseSync(code, {
    filename: filePath,
    sourceType: "module",
    plugins: []
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

export interface LinkResult {
  /** minifiedId → sourceId */
  links: Map<string, string>;
  /** minifiedId → original source line number (from source map) */
  originalLines: Map<string, number>;
}

/**
 * Link minified functions back to source functions using source maps.
 */
export async function linkMinifiedToSource(
  minifiedFunctions: Map<string, FunctionNode>,
  sourceFunctions: SourceFunction[],
  rawSourceMap: object
): Promise<LinkResult> {
  const links = new Map<string, string>();
  const originalLines = new Map<string, number>();

  const consumer = await new SourceMapConsumer(rawSourceMap as any);

  try {
    for (const [sessionId, fn] of minifiedFunctions) {
      const loc = fn.path.node.loc;
      if (!loc) continue;

      const originalPos = consumer.originalPositionFor({
        line: loc.start.line,
        column: loc.start.column
      });

      if (originalPos.line !== null) {
        const origLine = originalPos.line;
        originalLines.set(sessionId, origLine);

        // Find the most specific source function at this original position
        // (smallest line range that contains the position)
        const candidates = sourceFunctions.filter(
          (sFn) =>
            sFn.location.startLine <= origLine &&
            sFn.location.endLine >= origLine
        );

        if (candidates.length > 0) {
          // Sort by range size (smallest first) to get most specific function
          candidates.sort(
            (a, b) =>
              a.location.endLine -
              a.location.startLine -
              (b.location.endLine - b.location.startLine)
          );
          links.set(sessionId, candidates[0].id);
        }
      }
    }
  } finally {
    consumer.destroy();
  }

  return { links, originalLines };
}

interface ValidationContext {
  matchResult: MatchResult;
  v1SourceToMinified: Map<string, string>;
  v2SourceToMinified: Map<string, string>;
  reverseMatches: Map<string, string>;
  newV2Functions: Set<string>;
  expectedMatches: Set<string>;
}

function sourceKey(corr: { sourceFile: string; sourceName: string }): string {
  return `${corr.sourceFile}::${corr.sourceName}`;
}

/**
 * Validate an unchanged function correspondence.
 */
function validateUnchanged(
  corr: GroundTruth["correspondence"][number],
  ctx: ValidationContext,
  metrics: ValidationMetrics,
  failures: ValidationFailure[]
): void {
  metrics.unchangedFunctions.total++;

  const v1MinId = ctx.v1SourceToMinified.get(sourceKey(corr));
  const v2MinId = ctx.v2SourceToMinified.get(sourceKey(corr));

  if (!v1MinId || !v2MinId) {
    metrics.unchangedFunctions.fingerprintsMismatched++;
    failures.push({
      type: "unchanged-but-fingerprint-mismatch",
      sourceName: corr.sourceName,
      sourceFile: corr.sourceFile,
      expected: "Fingerprints should match (function unchanged)",
      actual: `Could not link to minified output (v1: ${v1MinId ? "found" : "missing"}, v2: ${v2MinId ? "found" : "missing"})`
    });
    return;
  }

  const matchedNewId = ctx.matchResult.matches.get(v1MinId);
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
        : "No fingerprint match found"
    });
  }
}

/**
 * Validate a modified function correspondence.
 */
function validateModified(
  corr: GroundTruth["correspondence"][number],
  ctx: ValidationContext,
  metrics: ValidationMetrics,
  failures: ValidationFailure[]
): void {
  metrics.modifiedFunctions.total++;

  const v1MinId = ctx.v1SourceToMinified.get(sourceKey(corr));
  const v2MinId = ctx.v2SourceToMinified.get(sourceKey(corr));

  if (!v1MinId || !v2MinId) {
    metrics.modifiedFunctions.fingerprintsDiffered++;
    return;
  }

  const matchedNewId = ctx.matchResult.matches.get(v1MinId);
  if (matchedNewId !== v2MinId) {
    metrics.modifiedFunctions.fingerprintsDiffered++;
    return;
  }

  // Fingerprints matched despite modification
  if (ctx.expectedMatches.has(corr.sourceName)) {
    metrics.modifiedFunctions.syntacticOnly++;
  } else {
    metrics.modifiedFunctions.fingerprintsMatched++;
    if (corr.changeDetails?.bodyChanged) {
      failures.push({
        type: "modified-but-fingerprint-match",
        sourceName: corr.sourceName,
        sourceFile: corr.sourceFile,
        expected: "Fingerprints should differ (function body modified)",
        actual: "Fingerprints matched despite modification"
      });
    }
  }
}

/**
 * Validate an added function correspondence.
 */
function validateAdded(
  corr: GroundTruth["correspondence"][number],
  ctx: ValidationContext,
  metrics: ValidationMetrics,
  failures: ValidationFailure[]
): void {
  metrics.addedFunctions.total++;

  const v2MinId = ctx.v2SourceToMinified.get(sourceKey(corr));

  if (!v2MinId || ctx.newV2Functions.has(v2MinId)) {
    metrics.addedFunctions.noMatchFound++;
    return;
  }

  // It got matched to something in v1 — false match
  const matchedFrom = ctx.reverseMatches.get(v2MinId);
  metrics.addedFunctions.falseMatchFound++;
  failures.push({
    type: "added-but-false-match",
    sourceName: corr.sourceName,
    sourceFile: corr.sourceFile,
    expected: "No match (function is new in v2)",
    actual: `Falsely matched to v1 function: ${matchedFrom ?? "unknown"}`
  });
}

/**
 * Compute accuracy scores from validation metrics.
 */
function computeAccuracyScores(metrics: ValidationMetrics): {
  cacheReuseAccuracy: number;
  changeDetectionAccuracy: number;
  overallAccuracy: number;
} {
  const cacheReuseAccuracy =
    metrics.unchangedFunctions.total > 0
      ? metrics.unchangedFunctions.fingerprintsMatched /
        metrics.unchangedFunctions.total
      : 1;

  const changeDetectionAccuracy =
    metrics.modifiedFunctions.total > 0
      ? (metrics.modifiedFunctions.fingerprintsDiffered +
          metrics.modifiedFunctions.syntacticOnly) /
        metrics.modifiedFunctions.total
      : 1;

  const totalChecked =
    metrics.unchangedFunctions.total +
    metrics.modifiedFunctions.total +
    metrics.addedFunctions.total;

  const totalCorrect =
    metrics.unchangedFunctions.fingerprintsMatched +
    metrics.modifiedFunctions.fingerprintsDiffered +
    metrics.modifiedFunctions.syntacticOnly +
    metrics.addedFunctions.noMatchFound;

  const overallAccuracy = totalChecked > 0 ? totalCorrect / totalChecked : 1;

  return { cacheReuseAccuracy, changeDetectionAccuracy, overallAccuracy };
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
  v1LinkResult: LinkResult,
  v2LinkResult: LinkResult,
  expectMatchDespiteModification?: Array<{ function: string; reason: string }>
): ValidationResult {
  const failures: ValidationFailure[] = [];

  const metrics: ValidationMetrics = {
    unchangedFunctions: {
      total: 0,
      fingerprintsMatched: 0,
      fingerprintsMismatched: 0
    },
    modifiedFunctions: {
      total: 0,
      fingerprintsDiffered: 0,
      fingerprintsMatched: 0,
      syntacticOnly: 0
    },
    addedFunctions: { total: 0, noMatchFound: 0, falseMatchFound: 0 },
    removedFunctions: { total: 0 }
  };

  const reverseMatches = new Map<string, string>();
  for (const [oldId, newId] of matchResult.matches) {
    reverseMatches.set(newId, oldId);
  }

  const ctx: ValidationContext = {
    matchResult,
    v1SourceToMinified: invertLinks(
      v1LinkResult.links,
      v1LinkResult.originalLines,
      groundTruth.v1Functions
    ),
    v2SourceToMinified: invertLinks(
      v2LinkResult.links,
      v2LinkResult.originalLines,
      groundTruth.v2Functions
    ),
    reverseMatches,
    newV2Functions: new Set(findNewFunctions(v1Index, v2Index, matchResult)),
    expectedMatches: new Set(
      (expectMatchDespiteModification ?? []).map((o) => o.function)
    )
  };

  for (const corr of groundTruth.correspondence) {
    switch (corr.changeType) {
      case "unchanged":
        validateUnchanged(corr, ctx, metrics, failures);
        break;
      case "modified":
        validateModified(corr, ctx, metrics, failures);
        break;
      case "added":
        validateAdded(corr, ctx, metrics, failures);
        break;
      case "removed":
        metrics.removedFunctions.total++;
        break;
    }
  }

  const scores = computeAccuracyScores(metrics);

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
    ...scores,
    failures
  };
}

/**
 * Invert the links map (minifiedId → sourceId) to (sourceId → minifiedId).
 *
 * When multiple minified functions link to the same source function (e.g. a
 * nested callback inside setState also maps to setState via source maps),
 * pick the minified function whose original source line is closest to the
 * source function's startLine — that's the actual function, not a nested helper.
 */
function invertLinks(
  links: Map<string, string>,
  originalLines: Map<string, number>,
  sourceFunctions: SourceFunction[]
): Map<string, string> {
  const sourceStartLines = new Map<string, number>();
  for (const sf of sourceFunctions) {
    sourceStartLines.set(sf.id, sf.location.startLine);
  }

  const inverted = new Map<string, string>();
  for (const [minifiedId, sourceId] of links) {
    const existing = inverted.get(sourceId);
    if (!existing) {
      inverted.set(sourceId, minifiedId);
      continue;
    }

    // Collision: pick the minified function whose original line is
    // closest to the source function's start line.
    const startLine = sourceStartLines.get(sourceId);
    if (startLine === undefined) {
      continue; // keep existing
    }

    const existingLine = originalLines.get(existing);
    const newLine = originalLines.get(minifiedId);
    if (existingLine === undefined || newLine === undefined) {
      continue; // keep existing
    }

    if (Math.abs(newLine - startLine) < Math.abs(existingLine - startLine)) {
      inverted.set(sourceId, minifiedId);
    }
  }
  return inverted;
}

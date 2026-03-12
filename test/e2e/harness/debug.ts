import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { GroundTruth } from "./ground-truth.js";
import type { ValidationResult } from "./validate.js";
import type {
  FingerprintIndex,
  FunctionFingerprint,
  MatchResult
} from "../../../src/analysis/types.js";

/**
 * Extended failure info with debug data for artifact generation.
 */
export interface DebugFailure {
  type: string;
  sourceName: string;
  sourceFile: string;
  expected: string;
  actual: string;

  // Fingerprint details
  v1Fingerprint?: FunctionFingerprint;
  v2Fingerprint?: FunctionFingerprint;

  // Code snippets
  v1SourceCode?: string;
  v2SourceCode?: string;
  v1MinifiedCode?: string;
  v2MinifiedCode?: string;
}

/**
 * All data needed for debug output generation.
 */
export interface DebugContext {
  fixture: string;
  v1: string;
  v2: string;
  minifierConfig: string;

  groundTruth: GroundTruth;
  v1Index: FingerprintIndex;
  v2Index: FingerprintIndex;
  matchResult: MatchResult;

  // Links from minified → source
  v1Links: Map<string, string>;
  v2Links: Map<string, string>;

  // Source code maps: sourceId → code
  v1SourceCode: Map<string, string>;
  v2SourceCode: Map<string, string>;

  // Minified code maps: minifiedId → code
  v1MinifiedCode: Map<string, string>;
  v2MinifiedCode: Map<string, string>;

  // Validation result with failures
  result: ValidationResult;
}

/**
 * Get the output directory for a validation run.
 */
export function getOutputDir(
  fixture: string,
  v1: string,
  v2: string,
  minifierConfig: string
): string {
  return join(
    import.meta.dirname,
    "..",
    "output",
    fixture,
    `v${v1}-v${v2}-${minifierConfig}`
  );
}

/**
 * Get the snapshot directory for a validation run.
 */
export function getSnapshotDir(fixture: string): string {
  return join(import.meta.dirname, "..", "snapshots", fixture);
}

/**
 * Generate all debug artifacts for a validation run.
 */
export function generateDebugArtifacts(ctx: DebugContext): void {
  const outputDir = getOutputDir(
    ctx.fixture,
    ctx.v1,
    ctx.v2,
    ctx.minifierConfig
  );
  const debugDir = join(outputDir, "debug");

  mkdirSync(debugDir, { recursive: true });

  // Write main result file
  writeFileSync(
    join(outputDir, "results.json"),
    JSON.stringify(ctx.result, null, 2)
  );

  // Write ground truth
  writeFileSync(
    join(debugDir, "ground-truth.json"),
    JSON.stringify(ctx.groundTruth, null, 2)
  );

  // Write fingerprints (convert Maps to objects)
  writeFileSync(
    join(debugDir, "v1-fingerprints.json"),
    JSON.stringify(fingerprintIndexToObject(ctx.v1Index), null, 2)
  );
  writeFileSync(
    join(debugDir, "v2-fingerprints.json"),
    JSON.stringify(fingerprintIndexToObject(ctx.v2Index), null, 2)
  );

  // Write matching log
  writeFileSync(
    join(debugDir, "matching-log.json"),
    JSON.stringify(matchResultToObject(ctx.matchResult), null, 2)
  );

  // Generate per-failure debug artifacts
  if (ctx.result.failures.length > 0) {
    const failuresDir = join(debugDir, "failures");
    mkdirSync(failuresDir, { recursive: true });

    for (const failure of ctx.result.failures) {
      generateFailureArtifacts(ctx, failure, failuresDir);
    }
  }
}

/**
 * Generate debug artifacts for a single failure.
 */
function generateFailureArtifacts(
  ctx: DebugContext,
  failure: {
    type: string;
    sourceName: string;
    sourceFile: string;
    expected: string;
    actual: string;
  },
  failuresDir: string
): void {
  // Create a safe directory name
  const safeName = `${failure.sourceName}-${failure.type}`.replace(
    /[^a-zA-Z0-9_-]/g,
    "_"
  );
  const failureDir = join(failuresDir, safeName);
  mkdirSync(failureDir, { recursive: true });

  // Find source functions
  const sourceId = `${failure.sourceFile}::${failure.sourceName}`;
  const v1SourceFn = ctx.groundTruth.v1Functions.find((f) => f.id === sourceId);
  const v2SourceFn = ctx.groundTruth.v2Functions.find((f) => f.id === sourceId);

  // Find minified function IDs via reverse lookup
  const v1MinId = findMinifiedId(sourceId, ctx.v1Links);
  const v2MinId = findMinifiedId(sourceId, ctx.v2Links);

  // Get fingerprints
  const v1Fingerprint = v1MinId
    ? ctx.v1Index.fingerprints.get(v1MinId)
    : undefined;
  const v2Fingerprint = v2MinId
    ? ctx.v2Index.fingerprints.get(v2MinId)
    : undefined;

  // Get code snippets
  const v1SourceCode = ctx.v1SourceCode.get(sourceId);
  const v2SourceCode = ctx.v2SourceCode.get(sourceId);
  const v1MinifiedCode = v1MinId ? ctx.v1MinifiedCode.get(v1MinId) : undefined;
  const v2MinifiedCode = v2MinId ? ctx.v2MinifiedCode.get(v2MinId) : undefined;

  // Write summary.txt
  const summary = generateFailureSummary(
    failure,
    v1Fingerprint,
    v2Fingerprint,
    v1SourceCode,
    v2SourceCode,
    v1MinifiedCode,
    v2MinifiedCode
  );
  writeFileSync(join(failureDir, "summary.txt"), summary);

  // Write individual files
  if (v1SourceCode) {
    writeFileSync(join(failureDir, "v1-source.js"), v1SourceCode);
  }
  if (v2SourceCode) {
    writeFileSync(join(failureDir, "v2-source.js"), v2SourceCode);
  }
  if (v1MinifiedCode) {
    writeFileSync(join(failureDir, "v1-minified.js"), v1MinifiedCode);
  }
  if (v2MinifiedCode) {
    writeFileSync(join(failureDir, "v2-minified.js"), v2MinifiedCode);
  }
  if (v1Fingerprint) {
    writeFileSync(
      join(failureDir, "v1-fingerprint.json"),
      JSON.stringify(v1Fingerprint, null, 2)
    );
  }
  if (v2Fingerprint) {
    writeFileSync(
      join(failureDir, "v2-fingerprint.json"),
      JSON.stringify(v2Fingerprint, null, 2)
    );
  }

  // Write fingerprint diff
  if (v1Fingerprint && v2Fingerprint) {
    const diff = generateFingerprintDiff(v1Fingerprint, v2Fingerprint);
    writeFileSync(join(failureDir, "fingerprint-diff.txt"), diff);
  }
}

/**
 * Describe the source diff status between v1 and v2.
 */
function describeSourceDiff(
  v1SourceCode?: string,
  v2SourceCode?: string
): string {
  if (v1SourceCode && v2SourceCode && v1SourceCode === v2SourceCode) {
    return "  (no changes - function is identical in v1 and v2 source)";
  }
  if (v1SourceCode && v2SourceCode)
    return "  (source differs between v1 and v2)";
  if (!v1SourceCode && v2SourceCode) return "  (function only exists in v2)";
  if (v1SourceCode && !v2SourceCode) return "  (function only exists in v1)";
  return "  (source code not available)";
}

/**
 * Format truncated minified code lines for the summary.
 */
function formatMinifiedLines(
  v1MinifiedCode?: string,
  v2MinifiedCode?: string
): string[] {
  const lines: string[] = [];
  if (!v1MinifiedCode && !v2MinifiedCode) return lines;

  lines.push("MINIFIED CODE:");
  if (v1MinifiedCode) {
    const truncated =
      v1MinifiedCode.length > 200
        ? `${v1MinifiedCode.slice(0, 200)}...`
        : v1MinifiedCode;
    lines.push(`  v1: ${truncated}`);
  }
  if (v2MinifiedCode) {
    const truncated =
      v2MinifiedCode.length > 200
        ? `${v2MinifiedCode.slice(0, 200)}...`
        : v2MinifiedCode;
    lines.push(`  v2: ${truncated}`);
  }
  lines.push("");
  return lines;
}

/**
 * Generate a human-readable summary of a failure.
 */
function generateFailureSummary(
  failure: {
    type: string;
    sourceName: string;
    sourceFile: string;
    expected: string;
    actual: string;
  },
  v1Fingerprint?: FunctionFingerprint,
  v2Fingerprint?: FunctionFingerprint,
  v1SourceCode?: string,
  v2SourceCode?: string,
  v1MinifiedCode?: string,
  v2MinifiedCode?: string
): string {
  const lines: string[] = [];
  const sep = "=".repeat(60);

  lines.push(sep);
  lines.push(`FAILURE: ${failure.type}`);
  lines.push(`Function: ${failure.sourceName} (${failure.sourceFile})`);
  lines.push(sep);
  lines.push("");
  lines.push(`EXPECTED: ${failure.expected}`);
  lines.push(`ACTUAL:   ${failure.actual}`);
  lines.push("");

  lines.push("SOURCE DIFF:");
  lines.push(describeSourceDiff(v1SourceCode, v2SourceCode));
  lines.push("");

  if (v1Fingerprint && v2Fingerprint) {
    lines.push("FINGERPRINT COMPARISON:");
    lines.push(formatFingerprintComparison(v1Fingerprint, v2Fingerprint));
    lines.push("");
  }

  lines.push("ROOT CAUSE ANALYSIS:");
  lines.push(generateRootCauseHints(failure, v1Fingerprint, v2Fingerprint));
  lines.push("");

  lines.push(...formatMinifiedLines(v1MinifiedCode, v2MinifiedCode));

  return lines.join("\n");
}

/**
 * Format a side-by-side fingerprint comparison.
 */
function formatFingerprintComparison(
  v1: FunctionFingerprint,
  v2: FunctionFingerprint
): string {
  const lines: string[] = [];
  const pad = (s: string, len: number) => s.padEnd(len);
  const mark = (match: boolean) => (match ? "\u2713" : "\u2717 MISMATCH");

  // exactHash
  const hashMatch = v1.exactHash === v2.exactHash;
  lines.push(
    `  exactHash:       ${pad(`${v1.exactHash.slice(0, 12)}...`, 16)}  vs  ${pad(`${v2.exactHash.slice(0, 12)}...`, 16)}  ${mark(hashMatch)}`
  );

  // Features comparison
  if (v1.features && v2.features) {
    const f1 = v1.features;
    const f2 = v2.features;

    const arityMatch = f1.arity === f2.arity;
    lines.push(
      `  arity:           ${pad(String(f1.arity), 16)}  vs  ${pad(String(f2.arity), 16)}  ${mark(arityMatch)}`
    );

    const complexityMatch = f1.complexity === f2.complexity;
    lines.push(
      `  complexity:      ${pad(String(f1.complexity), 16)}  vs  ${pad(String(f2.complexity), 16)}  ${mark(complexityMatch)}`
    );

    const loopMatch = f1.loopCount === f2.loopCount;
    lines.push(
      `  loopCount:       ${pad(String(f1.loopCount), 16)}  vs  ${pad(String(f2.loopCount), 16)}  ${mark(loopMatch)}`
    );

    const branchMatch = f1.branchCount === f2.branchCount;
    lines.push(
      `  branchCount:     ${pad(String(f1.branchCount), 16)}  vs  ${pad(String(f2.branchCount), 16)}  ${mark(branchMatch)}`
    );

    const cfgMatch = f1.cfgShape === f2.cfgShape;
    lines.push(
      `  cfgShape:        ${pad(f1.cfgShape || "(none)", 16)}  vs  ${pad(f2.cfgShape || "(none)", 16)}  ${mark(cfgMatch)}`
    );

    const stringsMatch =
      JSON.stringify(f1.stringLiterals) === JSON.stringify(f2.stringLiterals);
    const strV1 = JSON.stringify(f1.stringLiterals.slice(0, 3));
    const strV2 = JSON.stringify(f2.stringLiterals.slice(0, 3));
    lines.push(
      `  stringLiterals:  ${pad(strV1, 16)}  vs  ${pad(strV2, 16)}  ${mark(stringsMatch)}`
    );
  }

  // Callee shapes comparison
  if (v1.calleeShapes && v2.calleeShapes) {
    const shapesMatch =
      JSON.stringify(v1.calleeShapes) === JSON.stringify(v2.calleeShapes);
    lines.push(
      `  calleeShapes:    ${pad(`[${v1.calleeShapes.length} shapes]`, 16)}  vs  ${pad(`[${v2.calleeShapes.length} shapes]`, 16)}  ${mark(shapesMatch)}`
    );
  }

  return lines.join("\n");
}

/**
 * Generate a detailed fingerprint diff.
 */
function generateFingerprintDiff(
  v1: FunctionFingerprint,
  v2: FunctionFingerprint
): string {
  const lines: string[] = [];
  lines.push("FINGERPRINT DIFF");
  lines.push("================");
  lines.push("");

  // exactHash
  lines.push(`exactHash:`);
  lines.push(`  v1: ${v1.exactHash}`);
  lines.push(`  v2: ${v2.exactHash}`);
  lines.push(`  match: ${v1.exactHash === v2.exactHash}`);
  lines.push("");

  // Features
  if (v1.features && v2.features) {
    lines.push("features:");
    const keys = Object.keys(v1.features) as (keyof typeof v1.features)[];
    for (const key of keys) {
      const val1 = JSON.stringify(v1.features[key]);
      const val2 = JSON.stringify(v2.features[key]);
      const match = val1 === val2;
      lines.push(`  ${key}:`);
      lines.push(`    v1: ${val1}`);
      lines.push(`    v2: ${val2}`);
      lines.push(`    match: ${match}`);
    }
    lines.push("");
  }

  // Callee shapes
  if (v1.calleeShapes || v2.calleeShapes) {
    lines.push("calleeShapes:");
    lines.push(
      `  v1: ${JSON.stringify(v1.calleeShapes ?? [], null, 2)
        .split("\n")
        .join("\n      ")}`
    );
    lines.push(
      `  v2: ${JSON.stringify(v2.calleeShapes ?? [], null, 2)
        .split("\n")
        .join("\n      ")}`
    );
    lines.push(
      `  match: ${JSON.stringify(v1.calleeShapes) === JSON.stringify(v2.calleeShapes)}`
    );
    lines.push("");
  }

  // Callee hashes
  if (v1.calleeHashes || v2.calleeHashes) {
    lines.push("calleeHashes:");
    lines.push(`  v1: ${JSON.stringify(v1.calleeHashes ?? [])}`);
    lines.push(`  v2: ${JSON.stringify(v2.calleeHashes ?? [])}`);
    lines.push(
      `  match: ${JSON.stringify(v1.calleeHashes) === JSON.stringify(v2.calleeHashes)}`
    );
    lines.push("");
  }

  return lines.join("\n");
}

/**
 * Generate root cause hints based on failure type and fingerprint data.
 */
function generateRootCauseHints(
  failure: {
    type: string;
    sourceName: string;
    sourceFile: string;
    expected: string;
    actual: string;
  },
  v1Fingerprint?: FunctionFingerprint,
  v2Fingerprint?: FunctionFingerprint
): string {
  const lines: string[] = [];

  switch (failure.type) {
    case "unchanged-but-fingerprint-mismatch":
      if (v1Fingerprint && v2Fingerprint) {
        const hashMatch = v1Fingerprint.exactHash === v2Fingerprint.exactHash;
        if (!hashMatch) {
          lines.push("  The exactHash differs between v1 and v2.");
          lines.push("  This could be caused by:");
          lines.push("    - Different minifier optimization choices");
          lines.push("    - AST normalization issues");
          lines.push("    - Whitespace or comment handling differences");
        } else if (v1Fingerprint.calleeShapes && v2Fingerprint.calleeShapes) {
          lines.push("  The exactHash matches, but callee shapes differ.");
          lines.push(
            "  This suggests changes in called functions affected the match."
          );
        }
      } else {
        lines.push("  Could not link function to minified output.");
        lines.push(
          "  Check if the source map correctly maps to the source location."
        );
      }
      break;

    case "modified-but-fingerprint-match":
      lines.push(
        "  The function was modified in source, but fingerprints matched."
      );
      lines.push("  This could indicate:");
      lines.push(
        "    - Changes that don't affect structural hash (e.g., comments)"
      );
      lines.push("    - Fingerprinting is too loose for this type of change");
      lines.push("    - Ground truth incorrectly classified as modified");
      break;

    case "added-but-false-match":
      lines.push(
        "  A new function was incorrectly matched to an old function."
      );
      lines.push("  This suggests the fingerprinting may be too loose,");
      lines.push(
        "  matching structurally similar but semantically different functions."
      );
      break;

    default:
      lines.push("  No specific analysis available for this failure type.");
  }

  return lines.join("\n");
}

/**
 * Find minified ID from source ID by inverting the links map.
 */
function findMinifiedId(
  sourceId: string,
  links: Map<string, string>
): string | undefined {
  for (const [minId, srcId] of links) {
    if (srcId === sourceId) {
      return minId;
    }
  }
  return undefined;
}

/**
 * Convert FingerprintIndex to a JSON-serializable object.
 */
function fingerprintIndexToObject(index: FingerprintIndex): object {
  return {
    byExactHash: Object.fromEntries(index.byExactHash),
    byResolution1: Object.fromEntries(index.byResolution1),
    fingerprints: Object.fromEntries(index.fingerprints)
  };
}

/**
 * Convert MatchResult to a JSON-serializable object.
 */
function matchResultToObject(result: MatchResult): object {
  return {
    matches: Object.fromEntries(result.matches),
    ambiguous: Object.fromEntries(
      [...result.ambiguous].map(([k, v]) => [k, v])
    ),
    unmatched: result.unmatched
  };
}

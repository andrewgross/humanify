/**
 * Analyze real Claude Code minified bundles with our fingerprinting pipeline.
 *
 * Usage:
 *   npx tsx experiments/012-minifier-sensitivity/analyze-claude-code.ts <file1> [file2]
 *
 * With one file: reports function count, unique exactHashes, collision rate.
 * With two files: also runs cross-version matchFunctions comparison.
 *
 * NOTE: For cross-version comparison of 50K+ function files, run with:
 *   node --max-old-space-size=8192 --import tsx/esm analyze-claude-code.ts f1 f2
 */

import { readFileSync } from "node:fs";
import { basename, dirname } from "node:path";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import {
  matchFunctions,
  getMatchStats
} from "../../src/analysis/fingerprint-index.js";
import type { FingerprintIndex } from "../../src/analysis/types.js";

function getLabel(filePath: string): string {
  return (
    basename(dirname(dirname(dirname(filePath)))) + "/" + basename(filePath)
  );
}

function analyzeFile(filePath: string): void {
  const label = getLabel(filePath);
  const fileSize = readFileSync(filePath).length;
  console.log(`\nParsing ${label} (${(fileSize / 1e6).toFixed(1)}MB)...`);

  const code = readFileSync(filePath, "utf-8");
  const start = performance.now();
  const data = buildFingerprintData(code, filePath);
  const parseTimeMs = performance.now() - start;

  const totalFunctions = data.index.fingerprints.size;
  const uniqueHashes = data.index.byExactHash.size;

  // Count collisions
  let collisionCount = 0;
  const collisionGroups: Array<{ hash: string; count: number }> = [];
  for (const [hash, ids] of data.index.byExactHash) {
    if (ids.length > 1) {
      collisionCount += ids.length;
      collisionGroups.push({ hash, count: ids.length });
    }
  }
  collisionGroups.sort((a, b) => b.count - a.count);

  // memberKey coverage
  let withMemberKey = 0;
  for (const fp of data.index.fingerprints.values()) {
    if (fp.memberKey) withMemberKey++;
  }

  console.log(`\n── ${label} ──`);
  console.log(`  Functions:       ${totalFunctions}`);
  console.log(`  Unique hashes:   ${uniqueHashes}`);
  console.log(
    `  Colliding fns:   ${collisionCount} (${((collisionCount / totalFunctions) * 100).toFixed(1)}%)`
  );
  console.log(
    `  Non-colliding:   ${totalFunctions - collisionCount} (${(((totalFunctions - collisionCount) / totalFunctions) * 100).toFixed(1)}%)`
  );
  console.log(
    `  With memberKey:  ${withMemberKey} (${((withMemberKey / totalFunctions) * 100).toFixed(1)}%)`
  );
  console.log(`  Parse time:      ${(parseTimeMs / 1000).toFixed(1)}s`);

  if (collisionGroups.length > 0) {
    console.log(`  Top collision groups:`);
    for (const { hash, count } of collisionGroups.slice(0, 10)) {
      console.log(`    ${hash}: ${count} functions`);
    }
  }
}

/**
 * Build an index that retains the call-graph topology (internalCallees,
 * callers, scopeParent) but drops the heavy AST NodePath references.
 * This allows propagation to work while fitting two 53K-function bundles
 * in memory.
 */
function buildLightweightIndex(filePath: string): FingerprintIndex {
  const code = readFileSync(filePath, "utf-8");
  const data = buildFingerprintData(code, filePath);

  // Strip NodePath references from each FunctionNode to free the AST.
  // Propagation only needs sessionId, internalCallees, callers, scopeParent.
  if (data.index.functions) {
    for (const fn of data.index.functions.values()) {
      // @ts-expect-error — intentionally nulling out the heavy field
      fn.path = null;
    }
  }

  return data.index;
}

function compareVersions(
  fileA: string,
  fileB: string,
  enablePropagation = false
): void {
  const labelA = getLabel(fileA);
  const labelB = getLabel(fileB);
  console.log(`\n── Cross-version: ${labelA} → ${labelB} ──`);

  console.log("  Building index A...");
  const indexA = buildLightweightIndex(fileA);
  const sizeA = indexA.fingerprints.size;

  // Force GC of file A's AST before parsing B
  global.gc?.();

  console.log("  Building index B...");
  const indexB = buildLightweightIndex(fileB);
  const sizeB = indexB.fingerprints.size;

  console.log(
    `  Running matchFunctions${enablePropagation ? " (with propagation)" : ""}...`
  );
  const start = performance.now();
  const result = matchFunctions(indexA, indexB, { enablePropagation });
  const matchTimeMs = performance.now() - start;
  const stats = getMatchStats(result);

  console.log(`  Match time:      ${(matchTimeMs / 1000).toFixed(1)}s`);
  console.log(`  v1 functions:    ${sizeA}`);
  console.log(`  v2 functions:    ${sizeB}`);
  console.log(
    `  Matched:         ${stats.matched} (${(stats.matchRate * 100).toFixed(1)}%)`
  );
  console.log(`  Ambiguous:       ${stats.ambiguous}`);
  console.log(`  Unmatched:       ${stats.unmatched}`);

  // Resolution stats
  const rs = result.resolutionStats;
  console.log(`  Resolution breakdown:`);
  console.log(`    exactHash unique:  ${rs.exactHashUnique}`);
  console.log(`    memberKey:         ${rs.memberKeyResolved}`);
  console.log(`    calleeShapes:      ${rs.calleeShapesResolved}`);
  console.log(`    callerShapes:      ${rs.callerShapesResolved}`);
  console.log(`    calleeHashes:      ${rs.calleeHashesResolved}`);
  console.log(`    twoHopShapes:      ${rs.twoHopShapesResolved}`);
  console.log(`    shingleSimilarity: ${rs.shingleSimilarityResolved}`);
  console.log(`    propagation:       ${rs.propagationResolved}`);
  console.log(`    stillAmbiguous:    ${rs.stillAmbiguous}`);
  console.log(`    unmatched:         ${rs.unmatched}`);

  // Compute what % would be handled by exactHash alone vs needing cascade
  const exactOnly = rs.exactHashUnique;
  const cascadeResolved =
    rs.memberKeyResolved +
    rs.calleeShapesResolved +
    rs.callerShapesResolved +
    rs.calleeHashesResolved +
    rs.twoHopShapesResolved +
    rs.shingleSimilarityResolved +
    rs.propagationResolved;
  const total = stats.matched + stats.ambiguous + stats.unmatched;

  console.log(`  Summary:`);
  console.log(
    `    Exact hash alone: ${exactOnly}/${total} (${((exactOnly / total) * 100).toFixed(1)}%)`
  );
  console.log(
    `    Cascade resolved: ${cascadeResolved}/${total} (${((cascadeResolved / total) * 100).toFixed(1)}%)`
  );
  console.log(
    `    Total matched:    ${stats.matched}/${total} (${((stats.matched / total) * 100).toFixed(1)}%)`
  );
  console.log(
    `    Still ambiguous:  ${rs.stillAmbiguous}/${total} (${((rs.stillAmbiguous / total) * 100).toFixed(1)}%)`
  );
  console.log(
    `    Unmatched (new/removed): ${rs.unmatched}/${total} (${((rs.unmatched / total) * 100).toFixed(1)}%)`
  );
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const enablePropagation = args.includes("--propagation");
  const files = args.filter((a) => !a.startsWith("--"));

  if (files.length === 0) {
    console.error(
      "Usage: npx tsx analyze-claude-code.ts <file1> [file2] [--propagation]"
    );
    process.exit(1);
  }

  // Analyze each file individually
  for (const f of files) {
    analyzeFile(f);
  }

  // Cross-version comparison if two files given
  if (files.length === 2) {
    compareVersions(files[0], files[1], enablePropagation);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

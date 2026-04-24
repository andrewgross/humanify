/**
 * Inspect remaining ambiguous functions after matching + propagation.
 *
 * Usage:
 *   node --max-old-space-size=8192 --expose-gc --import tsx/esm \
 *     experiments/012-minifier-sensitivity/inspect-ambiguous.ts <v1> <v2>
 */

import { readFileSync } from "node:fs";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import { matchFunctions } from "../../src/analysis/fingerprint-index.js";
import type {
  FingerprintIndex,
  FunctionNode
} from "../../src/analysis/types.js";

function buildLightweightIndex(filePath: string): FingerprintIndex {
  const code = readFileSync(filePath, "utf-8");
  const data = buildFingerprintData(code, filePath);
  if (data.index.functions) {
    for (const fn of data.index.functions.values()) {
      // @ts-expect-error — intentionally nulling out the heavy field
      fn.path = null;
    }
  }
  return data.index;
}

function main(): void {
  const [fileA, fileB] = process.argv
    .slice(2)
    .filter((a) => !a.startsWith("--"));
  if (!fileA || !fileB) {
    console.error("Usage: inspect-ambiguous.ts <v1> <v2>");
    process.exit(1);
  }

  console.log("Building indices...");
  const indexA = buildLightweightIndex(fileA);
  global.gc?.();
  const indexB = buildLightweightIndex(fileB);

  console.log("Running matchFunctions with propagation...");
  const result = matchFunctions(indexA, indexB, { enablePropagation: true });

  const oldFns = indexA.functions!;
  const newFns = indexB.functions!;

  // Categorize ambiguous by scope parent status
  let parentMatched = 0;
  let parentAmbiguous = 0;
  let noParent = 0;
  let parentUnmatched = 0;

  // Track hash group sizes
  const hashGroupSizes = new Map<number, number>(); // candidateCount → how many ambiguous
  // Track scope-ordinal failure reasons
  let countMismatch = 0;
  let sameSiblingCount = 0;

  for (const [oldId, candidates] of result.ambiguous) {
    const fn = oldFns.get(oldId);
    if (!fn) continue;

    // Candidate count distribution
    const sz = candidates.length;
    hashGroupSizes.set(sz, (hashGroupSizes.get(sz) ?? 0) + 1);

    if (!fn.scopeParent) {
      noParent++;
    } else {
      const parentNewId = result.matches.get(fn.scopeParent.sessionId);
      if (parentNewId) {
        parentMatched++;

        // Why didn't ordinal work? Check sibling counts
        const oldHash = fn.fingerprint.structuralHash;
        const oldSiblings = [...oldFns.values()].filter(
          (f) =>
            f.scopeParent?.sessionId === fn.scopeParent!.sessionId &&
            f.fingerprint.structuralHash === oldHash
        );
        const newSiblings = [...newFns.values()].filter(
          (f) =>
            f.scopeParent?.sessionId === parentNewId &&
            f.fingerprint.structuralHash === oldHash
        );

        if (oldSiblings.length !== newSiblings.length) {
          countMismatch++;
        } else {
          sameSiblingCount++;
        }
      } else if (result.ambiguous.has(fn.scopeParent.sessionId)) {
        parentAmbiguous++;
      } else {
        parentUnmatched++;
      }
    }
  }

  console.log(`\n── Ambiguous breakdown (${result.ambiguous.size} total) ──`);
  console.log(`  No parent (top-level):    ${noParent}`);
  console.log(`  Parent matched:           ${parentMatched}`);
  console.log(`    count mismatch:         ${countMismatch}`);
  console.log(`    same count (unexpected):${sameSiblingCount}`);
  console.log(`  Parent ambiguous:         ${parentAmbiguous}`);
  console.log(`  Parent unmatched:         ${parentUnmatched}`);

  console.log(`\n── Candidate count distribution ──`);
  const sortedSizes = [...hashGroupSizes.entries()].sort((a, b) => b[1] - a[1]);
  for (const [sz, count] of sortedSizes.slice(0, 15)) {
    console.log(`  ${sz} candidates: ${count} functions`);
  }

  // Top hashes among remaining ambiguous
  const hashCounts = new Map<string, number>();
  for (const [oldId] of result.ambiguous) {
    const fn = oldFns.get(oldId);
    if (!fn) continue;
    const h = fn.fingerprint.structuralHash;
    hashCounts.set(h, (hashCounts.get(h) ?? 0) + 1);
  }
  const topHashes = [...hashCounts.entries()].sort((a, b) => b[1] - a[1]);

  console.log(`\n── Top ambiguous hash groups ──`);
  for (const [hash, count] of topHashes.slice(0, 15)) {
    // Get a sample function to show features
    let sampleFn: FunctionNode | undefined;
    for (const [oldId] of result.ambiguous) {
      const fn = oldFns.get(oldId);
      if (fn?.fingerprint.structuralHash === hash) {
        sampleFn = fn;
        break;
      }
    }
    const feat = sampleFn?.fingerprint.features;
    const mk = sampleFn?.fingerprint.memberKey ?? "(none)";
    const callees = sampleFn?.internalCallees.size ?? 0;
    const callers = sampleFn?.callers.size ?? 0;
    const parent = sampleFn?.scopeParent ? "yes" : "no";
    console.log(
      `  ${hash}: ${count} fns | arity=${feat?.arity ?? "?"} complexity=${feat?.complexity ?? "?"} ` +
        `callees=${callees} callers=${callers} parent=${parent} memberKey=${mk}`
    );
  }

  // How many no-parent ambiguous have any distinguishing features at all?
  let noParentNoCallees = 0;
  let noParentNoCallers = 0;
  let noParentNoMemberKey = 0;
  let noParentFeatureless = 0;
  for (const [oldId] of result.ambiguous) {
    const fn = oldFns.get(oldId);
    if (!fn || fn.scopeParent) continue;
    const hasCallees = fn.internalCallees.size > 0;
    const hasCallers = fn.callers.size > 0;
    const hasMemberKey = !!fn.fingerprint.memberKey;
    if (!hasCallees) noParentNoCallees++;
    if (!hasCallers) noParentNoCallers++;
    if (!hasMemberKey) noParentNoMemberKey++;
    if (!hasCallees && !hasCallers && !hasMemberKey) noParentFeatureless++;
  }
  console.log(
    `\n── No-parent ambiguous signal availability (${noParent} fns) ──`
  );
  console.log(`  No callees:    ${noParentNoCallees}`);
  console.log(`  No callers:    ${noParentNoCallers}`);
  console.log(`  No memberKey:  ${noParentNoMemberKey}`);
  console.log(
    `  Fully featureless (no callees, callers, or memberKey): ${noParentFeatureless}`
  );
}

main();

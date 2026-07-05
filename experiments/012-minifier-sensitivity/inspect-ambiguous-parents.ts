/**
 * Inspect the parents of "parent ambiguous" functions to understand
 * what kinds of functions serve as ambiguous scope parents.
 *
 * Usage:
 *   node --max-old-space-size=8192 --expose-gc --import tsx/esm \
 *     experiments/012-minifier-sensitivity/inspect-ambiguous-parents.ts <v1> <v2>
 */

import { readFileSync } from "node:fs";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import { matchFunctions } from "../../src/analysis/fingerprint-index.js";
import type { FingerprintIndex } from "../../src/analysis/types.js";

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
    console.error("Usage: inspect-ambiguous-parents.ts <v1> <v2>");
    process.exit(1);
  }

  console.log("Building indices...");
  const indexA = buildLightweightIndex(fileA);
  global.gc?.();
  const indexB = buildLightweightIndex(fileB);

  console.log("Running matchFunctions with propagation...");
  const result = matchFunctions(indexA, indexB, { enablePropagation: true });

  const oldFns = indexA.functions!;

  // Collect the parents of "parent ambiguous" functions
  const parentHashCounts = new Map<string, number>();
  const parentIds = new Set<string>();

  for (const [oldId] of result.ambiguous) {
    const fn = oldFns.get(oldId);
    if (!fn?.scopeParent) continue;
    if (!result.ambiguous.has(fn.scopeParent.sessionId)) continue;

    const parent = fn.scopeParent;
    parentIds.add(parent.sessionId);
    const h = parent.fingerprint.structuralHash;
    parentHashCounts.set(h, (parentHashCounts.get(h) ?? 0) + 1);
  }

  console.log(`\n── Parents of "parent ambiguous" functions ──`);
  console.log(`  Unique parent functions: ${parentIds.size}`);

  const topParentHashes = [...parentHashCounts.entries()].sort(
    (a, b) => b[1] - a[1]
  );
  console.log(
    `\n  Top parent hashes (by how many ambiguous children reference them):`
  );
  for (const [hash, count] of topParentHashes.slice(0, 20)) {
    // Get a sample parent
    let sampleParent = undefined;
    for (const pid of parentIds) {
      const fn = oldFns.get(pid);
      if (fn?.fingerprint.structuralHash === hash) {
        sampleParent = fn;
        break;
      }
    }
    const feat = sampleParent?.fingerprint.features;
    const mk = sampleParent?.fingerprint.memberKey ?? "(none)";
    const callees = sampleParent?.internalCallees.size ?? 0;
    const callers = sampleParent?.callers.size ?? 0;
    const hasParent = sampleParent?.scopeParent ? "yes" : "no";
    console.log(
      `    ${hash}: ${count} children | arity=${feat?.arity ?? "?"} complexity=${feat?.complexity ?? "?"} ` +
        `callees=${callees} callers=${callers} parent=${hasParent} memberKey=${mk}`
    );
  }

  // Are the parents themselves top-level or nested?
  let parentsTopLevel = 0;
  let parentsNested = 0;
  let parentsWithCallees = 0;
  let parentsWithCallers = 0;
  let parentsWithMemberKey = 0;

  for (const pid of parentIds) {
    const fn = oldFns.get(pid);
    if (!fn) continue;
    if (fn.scopeParent) parentsNested++;
    else parentsTopLevel++;
    if (fn.internalCallees.size > 0) parentsWithCallees++;
    if (fn.callers.size > 0) parentsWithCallers++;
    if (fn.fingerprint.memberKey) parentsWithMemberKey++;
  }

  console.log(`\n  Parent characteristics (${parentIds.size} unique parents):`);
  console.log(`    Top-level (no parent): ${parentsTopLevel}`);
  console.log(`    Nested (has parent):   ${parentsNested}`);
  console.log(`    Has callees:           ${parentsWithCallees}`);
  console.log(`    Has callers:           ${parentsWithCallers}`);
  console.log(`    Has memberKey:         ${parentsWithMemberKey}`);

  // How deep is the ambiguity chain?
  // For each "parent ambiguous" fn, walk up scopeParent chain to find the root
  const chainDepths = new Map<number, number>();
  for (const [oldId] of result.ambiguous) {
    const fn = oldFns.get(oldId);
    if (!fn?.scopeParent) continue;
    if (!result.ambiguous.has(fn.scopeParent.sessionId)) continue;

    let depth = 0;
    let current = fn.scopeParent;
    while (current && result.ambiguous.has(current.sessionId)) {
      depth++;
      current = current.scopeParent!;
    }
    chainDepths.set(depth, (chainDepths.get(depth) ?? 0) + 1);
  }

  console.log(`\n  Ambiguous chain depths (how many ambiguous ancestors):`);
  const sortedDepths = [...chainDepths.entries()].sort((a, b) => a[0] - b[0]);
  for (const [depth, count] of sortedDepths) {
    const rootStatus = depth === 1 ? " ← root is matched/top-level" : "";
    console.log(`    depth ${depth}: ${count} functions${rootStatus}`);
  }
}

main();

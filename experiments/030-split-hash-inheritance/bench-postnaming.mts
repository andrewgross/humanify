/**
 * Micro-benchmark the post-naming passes that stall the 2.1.207->208 hop,
 * WITHOUT the ~15-min naming phase. Uses the archived humanified outputs
 * (already fully renamed) as stand-ins for the post-naming AST/code:
 *
 *   - computeStructuralSignature  (the hermetic structural invariant)
 *   - runPriorDiffReconciliation  (the reconcile pass, parses both bundles)
 *
 * Each is timed cold (fresh caches) and warm (second call), so we see which
 * pass owns the wall-clock and whether cache state is the lever.
 *
 *   VERSIONS_ROOT=<archived>/versions npx tsx bench-postnaming.mts
 */
import fs from "node:fs";
import * as t from "@babel/types";
import { resetAnalysisNodeCaches } from "../../src/analysis/node-caches.js";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import { parseFileAst, traverse } from "../../src/babel-utils.js";
import {
  computeNormalDiff,
  parseNormalDiff,
  reconcileDiffNoise
} from "../../src/rename/diff-reconcile.js";

const V =
  process.env.VERSIONS_ROOT ??
  "/Users/andrewgross/Development/unpacked-claude-code-run-2026-07-17/versions";

const code208 = fs.readFileSync(
  `${V}/claude-code-2.1.208/.humanify/humanified.js`,
  "utf8"
);
console.log(`208 output: ${(code208.length / 1e6).toFixed(1)}MB`);

function time<T>(label: string, fn: () => T): T {
  const t0 = performance.now();
  const r = fn();
  console.log(`  ${label}: ${Math.round(performance.now() - t0)}ms`);
  return r;
}

// --- structural invariant cost: full-program binding resolution ---
console.log("\n[structural signature = the hermetic invariant's core]");
const parseT = performance.now();
const ast208 = parseFileAst(code208);
if (!ast208) throw new Error("parse failed");
console.log(`  parse 208: ${Math.round(performance.now() - parseT)}ms`);

function sig(): string {
  let out = "";
  traverse(ast208 as t.File, {
    Program(path) {
      out = computeStructuralSignature(path);
      path.stop();
    }
  });
  return out;
}

resetAnalysisNodeCaches();
time("structural-signature COLD (fresh caches)", sig);
time("structural-signature WARM (cache hit)", sig);
resetAnalysisNodeCaches();
time("structural-signature COLD again (post-reset)", sig);

// --- reconcile pass: system diff + reconcileDiffNoise over the AST ---
console.log("\n[reconcile pass = system diff + reconcileDiffNoise]");
const code207 = fs.readFileSync(
  `${V}/claude-code-2.1.207/.humanify/humanified.js`,
  "utf8"
);
const diffText = time("computeNormalDiff(207,208) [system diff]", () =>
  computeNormalDiff(code207, code208)
);
const hunks = parseNormalDiff(diffText);
console.log(
  `  diff: ${(diffText.length / 1e6).toFixed(1)}MB, ${hunks.length} hunks`
);

resetAnalysisNodeCaches();
const reconAst = parseFileAst(code208) as t.File;
const priorLineCount = code207.split("\n").length;
time("reconcileDiffNoise COLD (fresh caches)", () =>
  reconcileDiffNoise(reconAst, diffText, {
    apply: true,
    descriptiveTier: true,
    isEligible: () => true,
    priorLineCount
  })
);

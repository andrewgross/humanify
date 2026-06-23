/**
 * Phase A baseline: measure cross-version factory-naming noise on Bun bundles.
 *
 * Question this answers: when humanify unpacks two adjacent versions of a
 * Bun-bundled app today, how many third-party library files appear with
 * DIFFERENT filenames despite having identical content? That's the diff
 * noise the proposed fix would eliminate.
 *
 * The current BunUnpackAdapter names each factory file by its obfuscated
 * factory var. Bun shuffles these names between builds, so a library that
 * didn't change can land at `Zx8.js` in one version and `Tnq.js` in the
 * next, producing maximum diff churn.
 *
 * Usage:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/cross-version-baseline.ts \
 *     <bundle-v1.js> <bundle-v2.js>
 */

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  classifyBunModules,
  nameCjsFactories
} from "../../src/analysis/bun-module-classification.js";
import { computeStructuralHash } from "../../src/analysis/structural-hash.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

interface FactoryInfo {
  factoryVar: string;
  /** Raw sha256(body source) — varies with minifier identifier rolls. */
  contentHash: string;
  /** Structural hash with identifier names normalized to positional placeholders. */
  structuralHash: string;
  byteSize: number;
  bannerPackage?: string;
  proposedName: string;
  nameSource: string;
}

function fmt(n: number): string {
  return n.toLocaleString();
}

function pct(part: number, whole: number): string {
  if (whole === 0) return "0.0%";
  return `${((part / whole) * 100).toFixed(1)}%`;
}

function analyze(label: string, bundlePath: string): FactoryInfo[] {
  console.log(`\n=== ${label}: ${bundlePath} ===`);
  const source = readFileSync(bundlePath, "utf-8");
  console.log(`Size: ${fmt(source.length)} bytes`);

  const t0 = performance.now();
  const ast = parseSync(source, {
    sourceType: "unambiguous",
    parserOpts: { errorRecovery: true }
  });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  console.log(`Parse: ${((performance.now() - t0) / 1000).toFixed(1)}s`);

  const wrapper = findWrapperFunction(ast as t.File);
  const classification = classifyBunModules(ast as t.File, source, wrapper);
  if (!classification) {
    console.log("No Bun CJS factory helper detected. Aborting.");
    process.exit(1);
  }
  console.log(`Factories: ${fmt(classification.factories.length)}`);

  nameCjsFactories(classification, source);

  // Compute a structural hash on each factory's body (the arrow/function
  // passed as the first argument to the helper). This normalizes
  // identifier names away, so it's stable across minified-name rolls.
  const result: FactoryInfo[] = [];
  for (const f of classification.factories) {
    const init = f.factoryPath.node.init;
    let structuralHash = "<no-init>";
    if (t.isCallExpression(init) && init.arguments.length > 0) {
      const arg0 = init.arguments[0];
      if (t.isArrowFunctionExpression(arg0) || t.isFunctionExpression(arg0)) {
        structuralHash = computeStructuralHash(arg0);
      }
    }
    result.push({
      factoryVar: f.factoryVar,
      contentHash: f.contentHash,
      structuralHash,
      byteSize: f.byteRange[1] - f.byteRange[0],
      bannerPackage: f.bannerPackage,
      proposedName: f.name ?? "<unnamed>",
      nameSource: f.nameSource ?? "?"
    });
  }
  return result;
}

const [, , path1, path2] = process.argv;
if (!path1 || !path2) {
  console.error(
    "usage: cross-version-baseline.ts <bundle-v1.js> <bundle-v2.js>"
  );
  process.exit(1);
}

const v1 = analyze("v1", path1);
const v2 = analyze("v2", path2);

// --- Cross-version comparison ----------------------------------------

console.log("\n=== Cross-version comparison ===");
console.log(
  `v1: ${fmt(v1.length)} factories | v2: ${fmt(v2.length)} factories`
);

const byVarV1 = new Map(v1.map((f) => [f.factoryVar, f]));
const byVarV2 = new Map(v2.map((f) => [f.factoryVar, f]));

const bucketBy = (
  arr: FactoryInfo[],
  key: keyof FactoryInfo
): Map<string, FactoryInfo[]> => {
  const m = new Map<string, FactoryInfo[]>();
  for (const f of arr) {
    const k = String(f[key]);
    const list = m.get(k) ?? [];
    list.push(f);
    m.set(k, list);
  }
  return m;
};

const byContentV1 = bucketBy(v1, "contentHash");
const byContentV2 = bucketBy(v2, "contentHash");
const byStructV1 = bucketBy(v1, "structuralHash");
const byStructV2 = bucketBy(v2, "structuralHash");

// (a) How the EXISTING adapter (factoryVar-based filename) makes pairs.
let varOverlap = 0;
let varSameContent = 0;
let varDifferentContent = 0;
for (const f1 of v1) {
  const f2 = byVarV2.get(f1.factoryVar);
  if (!f2) continue;
  varOverlap++;
  if (f1.contentHash === f2.contentHash) varSameContent++;
  else varDifferentContent++;
}

console.log("\n[BASELINE: filename = obfuscated factoryVar]");
console.log(
  `  factoryVar overlap v1∩v2: ${fmt(varOverlap)} ` +
    `(${pct(varOverlap, Math.min(v1.length, v2.length))} of the smaller set)`
);
console.log(`    of which content matches: ${fmt(varSameContent)}`);
console.log(`    of which content differs: ${fmt(varDifferentContent)}`);

const intersect = (
  a: Map<string, FactoryInfo[]>,
  b: Map<string, FactoryInfo[]>
): Set<string> => {
  const out = new Set<string>();
  for (const k of a.keys()) if (b.has(k)) out.add(k);
  return out;
};

const sumBytes = (arr: FactoryInfo[]) =>
  arr.reduce((acc, f) => acc + f.byteSize, 0);

const overlapContent = intersect(byContentV1, byContentV2);
const overlapStruct = intersect(byStructV1, byStructV2);

console.log("\n[OPTION A: filename = content-hash (sha256 of raw body)]");
console.log(
  `  unique content hashes: v1=${fmt(byContentV1.size)}, v2=${fmt(byContentV2.size)}`
);
console.log(
  `  cross-version matches: ${fmt(overlapContent.size)} ` +
    `(${pct(overlapContent.size, byContentV1.size)} of v1's unique hashes)`
);

console.log(
  "\n[OPTION B: filename = structural hash (identifier names normalized away)]"
);
console.log(
  `  unique structural hashes: v1=${fmt(byStructV1.size)}, v2=${fmt(byStructV2.size)}`
);
console.log(
  `  cross-version matches: ${fmt(overlapStruct.size)} ` +
    `(${pct(overlapStruct.size, byStructV1.size)} of v1's unique hashes)`
);
const unchangedBytesV1 = sumBytes(
  [...byStructV1.entries()]
    .filter(([h]) => overlapStruct.has(h))
    .flatMap(([, arr]) => arr)
);
console.log(
  `  bytes covered by unchanged libs: ${fmt(unchangedBytesV1)} ` +
    `(${pct(unchangedBytesV1, sumBytes(v1))} of v1's factory bytes)`
);

console.log("\n[THE NOISE GAP]");
const noisyFiles = overlapStruct.size - varSameContent;
console.log(
  `  libraries identical (by structure) but TODAY have different filenames: ${fmt(noisyFiles)}`
);
console.log(
  `    → diff -r currently shows them as ${fmt(noisyFiles)} deleted + ${fmt(noisyFiles)} added,` +
    `\n      when they should not appear in the diff at all.`
);

// (b) Diff of unique libraries between versions using STRUCTURAL hash.
const v1HashesByName = new Map(
  [...byStructV1.entries()].map(([h, arr]) => [
    arr[0].proposedName,
    { hash: h, ...arr[0] }
  ])
);
const v2HashesByName = new Map(
  [...byStructV2.entries()].map(([h, arr]) => [
    arr[0].proposedName,
    { hash: h, ...arr[0] }
  ])
);

const onlyV1: string[] = [];
const onlyV2: string[] = [];
const changedContent: string[] = [];
const allNames = new Set([...v1HashesByName.keys(), ...v2HashesByName.keys()]);
for (const name of allNames) {
  const a = v1HashesByName.get(name);
  const b = v2HashesByName.get(name);
  if (a && !b) onlyV1.push(name);
  else if (b && !a) onlyV2.push(name);
  else if (a && b && a.hash !== b.hash) changedContent.push(name);
}

console.log("\n[Library inventory (by proposed name)]");
console.log(
  `  unchanged: ${allNames.size - onlyV1.length - onlyV2.length - changedContent.length}`
);
console.log(`  changed content (same library, new version):`);
for (const n of changedContent.slice(0, 10)) console.log(`    ${n}`);
if (changedContent.length > 10)
  console.log(`    ... +${changedContent.length - 10} more`);
console.log(`  only in v1 (${onlyV1.length}):`);
for (const n of onlyV1.slice(0, 10)) console.log(`    ${n}`);
if (onlyV1.length > 10) console.log(`    ... +${onlyV1.length - 10} more`);
console.log(`  only in v2 (${onlyV2.length}):`);
for (const n of onlyV2.slice(0, 10)) console.log(`    ${n}`);
if (onlyV2.length > 10) console.log(`    ... +${onlyV2.length - 10} more`);

// Sanity: sha of the full bundles
console.log("\n[Full-bundle hashes]");
const sha = (p: string) =>
  createHash("sha256").update(readFileSync(p)).digest("hex").slice(0, 16);
console.log(`  v1: ${sha(path1)}`);
console.log(`  v2: ${sha(path2)}`);

/**
 * Smoke test for Bun CJS factory classification.
 *
 * Validates Levels 1 and 2 of the testing strategy without any LLM calls:
 *   1. classifyBunModules + nameCjsFactories: factory count, banner packages,
 *      naming-source distribution, bytes covered.
 *   2. buildUnifiedGraph: read thirdPartyClassification.bindingsSkipped and
 *      functionsSkipped to confirm the skip path fires.
 *
 * Usage:
 *   npx tsx experiments/013-bun-cjs-classification/smoke.ts <bundle.js>
 *
 * Memory: parsing a 14MB bundle benefits from a larger heap:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/smoke.ts <bundle.js>
 */

import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { parseSync } from "@babel/core";
import {
  classifyBunModules,
  nameCjsFactories
} from "../../src/analysis/bun-module-classification.js";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

function fmt(n: number): string {
  return n.toLocaleString();
}

function pct(part: number, whole: number): string {
  if (whole === 0) return "0.0%";
  return `${((part / whole) * 100).toFixed(1)}%`;
}

const bundlePath = process.argv[2];
if (!bundlePath) {
  console.error("usage: smoke.ts <bundle.js>");
  process.exit(1);
}

const t0 = performance.now();
const source = readFileSync(bundlePath, "utf-8");
const tLoad = performance.now();

console.log(`File: ${bundlePath}`);
console.log(
  `Size: ${fmt(source.length)} bytes (${(source.length / 1024 / 1024).toFixed(2)} MB)`
);
console.log(`Load: ${(tLoad - t0).toFixed(0)}ms\n`);

const tParseStart = performance.now();
const ast = parseSync(source, {
  sourceType: "unambiguous",
  parserOpts: { errorRecovery: true }
});
if (!ast || ast.type !== "File") throw new Error("parse failed");
const tParse = performance.now();
console.log(`Parse: ${((tParse - tParseStart) / 1000).toFixed(1)}s\n`);

// Level 1: classification
const tClassifyStart = performance.now();
const wrapper = findWrapperFunction(ast);
console.log(`Wrapper IIFE: ${wrapper ? "detected" : "none"}`);

const classification = classifyBunModules(ast, source, wrapper);
const tClassify = performance.now();
if (!classification) {
  console.log("\nNo Bun CJS factory helper detected — bundle is not Bun-CJS.");
  process.exit(0);
}
console.log(`Classify: ${((tClassify - tClassifyStart) / 1000).toFixed(2)}s`);
console.log(`CJS factory helper var: ${classification.cjsFactoryHelperVar}`);
console.log(`Factories detected: ${fmt(classification.factories.length)}`);

let coveredBytes = 0;
for (const factory of classification.factories) {
  coveredBytes += factory.byteRange[1] - factory.byteRange[0];
}
console.log(
  `Bytes covered by factories: ${fmt(coveredBytes)} (${pct(coveredBytes, source.length)} of source)`
);

const banners = classification.factories.filter((f) => f.bannerPackage);
const uniqueBanners = new Set(banners.map((f) => f.bannerPackage));
console.log(
  `\nBang banners: ${banners.length} factories, ${uniqueBanners.size} unique packages`
);
for (const pkg of [...uniqueBanners].sort()) {
  const count = banners.filter((f) => f.bannerPackage === pkg).length;
  console.log(`  ${pkg}: ${count}`);
}

// Apply naming cascade
const namedBy = nameCjsFactories(classification, source);
console.log(`\nNaming cascade:`);
console.log(`  banner:     ${fmt(namedBy.banner)}`);
console.log(`  url:        ${fmt(namedBy.url)}`);
console.log(`  carry-over: ${fmt(namedBy.carryOver)}`);
console.log(`  llm:        ${fmt(namedBy.llm)}  (stub)`);
console.log(`  fallback:   ${fmt(namedBy.fallback)}`);

// Sample 5 factory names from each source
console.log(`\nSample factory names:`);
const samples: Record<string, string[]> = {
  banner: [],
  url: [],
  fallback: []
};
for (const factory of classification.factories) {
  const arr = samples[factory.nameSource ?? "fallback"];
  if (arr && arr.length < 5) arr.push(factory.name ?? "<unnamed>");
}
for (const [src, names] of Object.entries(samples)) {
  if (names.length > 0) console.log(`  ${src}: ${names.join(", ")}`);
}

// Level 2: graph build, read skip counts
console.log(`\n--- Level 2: buildUnifiedGraph skip counts ---`);
const tGraphStart = performance.now();
const graph = buildUnifiedGraph(ast, bundlePath, undefined, undefined, source);
const tGraph = performance.now();
console.log(`Graph build: ${((tGraph - tGraphStart) / 1000).toFixed(2)}s`);

let fnNodes = 0;
let mbNodes = 0;
for (const node of graph.nodes.values()) {
  if (node.type === "function") fnNodes++;
  else if (node.type === "module-binding") mbNodes++;
}
console.log(`FunctionNodes in graph: ${fmt(fnNodes)}`);
console.log(`ModuleBindingNodes in graph: ${fmt(mbNodes)}`);

console.log(
  `\nIssue baseline (claude-code 2.1.120 pre-fix):` +
    `\n  ~59,537 FunctionNodes` +
    `\n  ~31,140 module bindings` +
    `\nExpect post-fix: FunctionNodes ↓ (factories' inner fns removed), ` +
    `\n                 module bindings → ~15K (factory-internal vars removed).`
);

console.log(
  `\nTotal wall-clock: ${((performance.now() - t0) / 1000).toFixed(1)}s`
);

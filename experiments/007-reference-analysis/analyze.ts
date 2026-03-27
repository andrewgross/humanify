/**
 * Experiment 007: Reference Analysis
 *
 * Measures whether identifier reference overlap correlates with same-file
 * membership. For each pair of top-level functions, computes Jaccard similarity
 * of their referenced name sets and checks whether they came from the same
 * original source file.
 *
 * Also catalogs bundler-specific structural patterns (esbuild __name, __export).
 *
 * Usage:
 *   tsx experiments/007-reference-analysis/analyze.ts <fixture-name>
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { collectReferencedNames } from "../../src/split/emitter.js";
import { parseFile } from "../../src/split/index.js";
import { extractGroundTruth } from "../ground-truth.js";

const FIXTURES_DIR = join(import.meta.dirname, "..", "fixtures");

interface RefStats {
  /** Total top-level functions analyzed */
  totalFunctions: number;
  /** Same-file pairs: mean Jaccard similarity */
  sameFileMeanJaccard: number;
  /** Cross-file pairs: mean Jaccard similarity */
  crossFileMeanJaccard: number;
  /** Separation ratio: sameFile / crossFile */
  separationRatio: number;
  /** Number of discriminative names (referenced by 1-5 functions) */
  discriminativeNames: number;
  /** Number of global names (referenced by 10+ functions) */
  globalNames: number;
  /** Total unique referenced names */
  totalNames: number;
}

interface BundlerSignals {
  /** __name(fn, "OriginalName") calls found */
  esbuildNameCalls: number;
  /** __export(exports, { ... }) blocks found */
  esbuildExportBlocks: number;
  /** Unique original names from __name calls */
  originalNames: string[];
  /** Export block groupings: mapping from export namespace to exported names */
  exportGroups: Map<string, string[]>;
}

/** Compute Jaccard similarity of two sets. */
function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 && b.size === 0) return 0;
  let intersection = 0;
  for (const item of a) {
    if (b.has(item)) intersection++;
  }
  const union = a.size + b.size - intersection;
  return union > 0 ? intersection / union : 0;
}

/** Convert a function node to a statement for collectReferencedNames. */
function fnToStatement(node: t.Function): t.Statement {
  if (t.isFunctionDeclaration(node)) return node;
  if ("body" in node && t.isBlockStatement(node.body)) return node.body;
  return t.expressionStatement(node as unknown as t.Expression);
}

/** Compute per-name frequency: how many functions reference each name. */
function computeNameFrequencies(
  refSets: Map<string, Set<string>>
): Map<string, number> {
  const freq = new Map<string, number>();
  for (const refs of refSets.values()) {
    for (const name of refs) {
      freq.set(name, (freq.get(name) ?? 0) + 1);
    }
  }
  return freq;
}

/** Analyze reference overlap between same-file and cross-file function pairs. */
function analyzeReferenceOverlap(
  refSets: Map<string, Set<string>>,
  groundTruth: Map<string, string>
): { sameFilePairs: number[]; crossFilePairs: number[] } {
  const sameFilePairs: number[] = [];
  const crossFilePairs: number[] = [];

  const ids = Array.from(refSets.keys()).filter((id) => groundTruth.has(id));

  // Sample pairs to avoid O(n^2) blowup on large fixtures
  const maxPairs = 50000;
  let pairCount = 0;

  for (let i = 0; i < ids.length && pairCount < maxPairs; i++) {
    for (let j = i + 1; j < ids.length && pairCount < maxPairs; j++) {
      const refsA = refSets.get(ids[i])!;
      const refsB = refSets.get(ids[j])!;
      const sim = jaccard(refsA, refsB);

      const fileA = groundTruth.get(ids[i])!;
      const fileB = groundTruth.get(ids[j])!;

      if (fileA === fileB) {
        sameFilePairs.push(sim);
      } else {
        crossFilePairs.push(sim);
      }
      pairCount++;
    }
  }

  return { sameFilePairs, crossFilePairs };
}

/** Detect esbuild-specific structural patterns in the source. */
function detectBundlerSignals(source: string): BundlerSignals {
  const signals: BundlerSignals = {
    esbuildNameCalls: 0,
    esbuildExportBlocks: 0,
    originalNames: [],
    exportGroups: new Map()
  };

  // __name(fn, "OriginalName") pattern
  const namePattern = /__name\(\s*\w+\s*,\s*"([^"]+)"\s*\)/g;
  let match: RegExpExecArray | null;
  while ((match = namePattern.exec(source)) !== null) {
    signals.esbuildNameCalls++;
    signals.originalNames.push(match[1]);
  }

  // __export(x_exports, { name1: () => name1, ... }) pattern
  const exportPattern = /__export\(\s*(\w+)\s*,\s*\{([^}]+)\}\s*\)/g;
  while ((match = exportPattern.exec(source)) !== null) {
    signals.esbuildExportBlocks++;
    const namespace = match[1];
    const body = match[2];
    const names: string[] = [];
    const nameExtractor = /(\w+)\s*:/g;
    let nameMatch: RegExpExecArray | null;
    while ((nameMatch = nameExtractor.exec(body)) !== null) {
      names.push(nameMatch[1]);
    }
    signals.exportGroups.set(namespace, names);
  }

  return signals;
}

/** Compute IDF-weighted Jaccard for a pair. */
function weightedJaccard(
  a: Set<string>,
  b: Set<string>,
  idf: Map<string, number>
): number {
  let intersectionWeight = 0;
  let unionWeight = 0;

  const allNames = new Set([...a, ...b]);
  for (const name of allNames) {
    const w = idf.get(name) ?? 0;
    if (a.has(name) && b.has(name)) {
      intersectionWeight += w;
    }
    unionWeight += w;
  }

  return unionWeight > 0 ? intersectionWeight / unionWeight : 0;
}

function mean(arr: number[]): number {
  if (arr.length === 0) return 0;
  return arr.reduce((a, b) => a + b, 0) / arr.length;
}

function median(arr: number[]): number {
  if (arr.length === 0) return 0;
  const sorted = [...arr].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

function percentile(arr: number[], p: number): number {
  if (arr.length === 0) return 0;
  const sorted = [...arr].sort((a, b) => a - b);
  const idx = Math.floor(sorted.length * p);
  return sorted[Math.min(idx, sorted.length - 1)];
}

async function analyze(fixtureName: string): Promise<void> {
  const fixtureDir = join(FIXTURES_DIR, fixtureName);
  const bundlePath = join(fixtureDir, "bundle.js");
  const mapPath = join(fixtureDir, "bundle.js.map");
  const source = readFileSync(bundlePath, "utf-8");

  console.log(`\n=== Reference Analysis: ${fixtureName} ===\n`);

  // 1. Parse and build function graph
  console.log("Parsing bundle...");
  const { ast } = parseFile(bundlePath);
  const functions = buildFunctionGraph(ast, bundlePath);
  const topLevel = functions.filter((fn) => !fn.scopeParent);
  console.log(
    `  ${functions.length} functions (${topLevel.length} top-level)\n`
  );

  // 2. Compute reference sets
  console.log("Computing reference sets...");
  const refSets = new Map<string, Set<string>>();
  for (const fn of topLevel) {
    const stmt = fnToStatement(fn.path.node);
    const refs = collectReferencedNames(stmt);
    refSets.set(fn.sessionId, refs);
  }

  // 3. Compute name frequencies
  const nameFreq = computeNameFrequencies(refSets);
  const totalNames = nameFreq.size;
  let discriminativeNames = 0;
  let globalNames = 0;

  for (const [, count] of nameFreq) {
    if (count <= 5) discriminativeNames++;
    if (count >= 10) globalNames++;
  }

  console.log(`  ${totalNames} unique referenced names`);
  console.log(
    `  ${discriminativeNames} discriminative (1-5 refs), ${globalNames} global (10+ refs)\n`
  );

  // 4. Extract ground truth
  console.log("Extracting ground truth...");
  const groundTruth = await extractGroundTruth(functions, mapPath);
  console.log(`  ${groundTruth.sourceFiles.length} original source files\n`);

  // 5. Analyze pairwise reference overlap
  console.log("Analyzing pairwise reference overlap...");
  const { sameFilePairs, crossFilePairs } = analyzeReferenceOverlap(
    refSets,
    groundTruth.functionToFile
  );

  const sameFileMean = mean(sameFilePairs);
  const crossFileMean = mean(crossFilePairs);
  const separationRatio =
    crossFileMean > 0 ? sameFileMean / crossFileMean : Infinity;

  console.log(`\n--- Unweighted Jaccard Similarity ---`);
  console.log(
    `  Same-file pairs (${sameFilePairs.length}):  mean=${sameFileMean.toFixed(4)}, median=${median(sameFilePairs).toFixed(4)}, p75=${percentile(sameFilePairs, 0.75).toFixed(4)}`
  );
  console.log(
    `  Cross-file pairs (${crossFilePairs.length}): mean=${crossFileMean.toFixed(4)}, median=${median(crossFilePairs).toFixed(4)}, p75=${percentile(crossFilePairs, 0.75).toFixed(4)}`
  );
  console.log(`  Separation ratio: ${separationRatio.toFixed(2)}x`);

  // 6. IDF-weighted analysis
  console.log(`\n--- IDF-Weighted Jaccard Similarity ---`);
  const N = topLevel.length;
  const idf = new Map<string, number>();
  for (const [name, count] of nameFreq) {
    idf.set(name, Math.log(N / count));
  }

  const sameFileWeighted: number[] = [];
  const crossFileWeighted: number[] = [];
  const ids = Array.from(refSets.keys()).filter((id) =>
    groundTruth.functionToFile.has(id)
  );

  const maxPairs = 50000;
  let pairCount = 0;
  for (let i = 0; i < ids.length && pairCount < maxPairs; i++) {
    for (let j = i + 1; j < ids.length && pairCount < maxPairs; j++) {
      const refsA = refSets.get(ids[i])!;
      const refsB = refSets.get(ids[j])!;
      const sim = weightedJaccard(refsA, refsB, idf);
      const fileA = groundTruth.functionToFile.get(ids[i])!;
      const fileB = groundTruth.functionToFile.get(ids[j])!;

      if (fileA === fileB) {
        sameFileWeighted.push(sim);
      } else {
        crossFileWeighted.push(sim);
      }
      pairCount++;
    }
  }

  const sameWeightedMean = mean(sameFileWeighted);
  const crossWeightedMean = mean(crossFileWeighted);
  const weightedSeparation =
    crossWeightedMean > 0 ? sameWeightedMean / crossWeightedMean : Infinity;

  console.log(
    `  Same-file pairs:  mean=${sameWeightedMean.toFixed(4)}, median=${median(sameFileWeighted).toFixed(4)}`
  );
  console.log(
    `  Cross-file pairs: mean=${crossWeightedMean.toFixed(4)}, median=${median(crossFileWeighted).toFixed(4)}`
  );
  console.log(`  Separation ratio: ${weightedSeparation.toFixed(2)}x`);

  // 7. Sparsity analysis (call graph)
  console.log(`\n--- Call Graph Sparsity ---`);
  const topLevelIds = new Set(topLevel.map((fn) => fn.sessionId));
  let zeroEdge = 0;
  for (const fn of topLevel) {
    const topLevelCallees = Array.from(fn.internalCallees).filter((c) =>
      topLevelIds.has(c.sessionId)
    );
    if (topLevelCallees.length === 0) {
      zeroEdge++;
    }
  }
  const sparsity = zeroEdge / topLevel.length;
  console.log(
    `  ${zeroEdge}/${topLevel.length} functions have zero top-level callees (${(sparsity * 100).toFixed(1)}% sparse)`
  );

  // 8. Bundler-specific signals
  console.log(`\n--- Bundler-Specific Signals ---`);
  const signals = detectBundlerSignals(source);
  console.log(`  __name() calls: ${signals.esbuildNameCalls}`);
  console.log(`  __export() blocks: ${signals.esbuildExportBlocks}`);
  if (signals.originalNames.length > 0) {
    console.log(
      `  Sample original names: ${signals.originalNames.slice(0, 10).join(", ")}`
    );
  }
  if (signals.exportGroups.size > 0) {
    console.log(`  Export groups:`);
    for (const [ns, names] of signals.exportGroups) {
      console.log(
        `    ${ns}: ${names.slice(0, 8).join(", ")}${names.length > 8 ? "..." : ""}`
      );
    }
  }

  // 9. Top discriminative names
  console.log(`\n--- Top Discriminative Names (by IDF) ---`);
  const sortedByIdf = Array.from(nameFreq.entries())
    .filter(([, count]) => count >= 2 && count <= 10)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20);
  for (const [name, count] of sortedByIdf) {
    console.log(
      `  ${name}: referenced by ${count} functions (idf=${idf.get(name)?.toFixed(2)})`
    );
  }

  // Summary
  const stats: RefStats = {
    totalFunctions: topLevel.length,
    sameFileMeanJaccard: sameFileMean,
    crossFileMeanJaccard: crossFileMean,
    separationRatio,
    discriminativeNames,
    globalNames,
    totalNames
  };

  console.log(`\n=== Summary ===`);
  console.log(JSON.stringify(stats, null, 2));
  console.log(
    `\nConclusion: ${separationRatio > 2 ? "STRONG" : separationRatio > 1.5 ? "MODERATE" : "WEAK"} signal from reference overlap`
  );
  console.log(
    `IDF weighting: ${weightedSeparation > separationRatio ? "IMPROVES" : "does not improve"} separation (${weightedSeparation.toFixed(2)}x vs ${separationRatio.toFixed(2)}x)`
  );
  console.log(
    `Call graph sparsity: ${(sparsity * 100).toFixed(1)}% — ${sparsity > 0.7 ? "reference clustering NEEDED" : "call graph may suffice"}`
  );
}

const fixtureName = process.argv[2];
if (!fixtureName) {
  console.log(
    "Usage: tsx experiments/007-reference-analysis/analyze.ts <fixture-name>"
  );
  process.exit(1);
}

analyze(fixtureName).catch((err) => {
  console.error(err);
  process.exit(1);
});

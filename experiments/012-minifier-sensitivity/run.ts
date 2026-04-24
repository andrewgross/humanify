/**
 * Minifier sensitivity study: for each real library × minifier, measure the
 * blast radius of a single function perturbation and verify that the matching
 * cascade handles it correctly.
 *
 * Approach: inject a uniquely-marked console.log into one function, minify,
 * remove the marker from the minified output, then diff against the unperturbed
 * minified code. After marker removal, all structuralHashes should be identical —
 * any matching failure is a real cascade bug.
 *
 * Usage:
 *   npx tsx experiments/012-minifier-sensitivity/run.ts [--library <id>]
 */

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { randomUUID } from "node:crypto";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { generate, traverse } from "../../src/babel-utils.js";
import {
  MINIFIER_CONFIGS,
  BUN_BUNDLE_CONFIG
} from "../../test/e2e/harness/minify.js";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import {
  matchFunctions,
  getMatchStats
} from "../../src/analysis/fingerprint-index.js";
import { minifyString } from "../011-perturbation-lab/minify-string.js";
import { computeSourceGroundTruth } from "../011-perturbation-lab/ground-truth.js";
import { discoverPerturbableNames } from "./discover.js";
import { removeMarker } from "./marker-removal.js";
import { measureBlastRadius, type BlastRadius } from "./diff-analysis.js";

// ── Corpus ──────────────────────────────────────────────────────────────

interface CorpusEntry {
  id: string;
  sourcePath: string;
  description: string;
}

const FIXTURES_ROOT = join(
  import.meta.dirname,
  "..",
  "..",
  "test",
  "e2e",
  "fixtures"
);

const CORPUS: CorpusEntry[] = [
  {
    id: "mitt",
    sourcePath: join(FIXTURES_ROOT, "mitt/build/v3.0.0/build/index.js"),
    description: "tiny event emitter (~4 fns)"
  },
  {
    id: "nanoid",
    sourcePath: join(FIXTURES_ROOT, "nanoid/build/v5.0.8/build/index.js"),
    description: "ID generator (~5 fns)"
  },
  {
    id: "zustand",
    sourcePath: join(FIXTURES_ROOT, "zustand/build/v4.4.0/build/vanilla.js"),
    description: "state management (~7 fns)"
  },
  {
    id: "preact",
    sourcePath: join(FIXTURES_ROOT, "preact/build/v10.24.0/build/index.js"),
    description: "virtual DOM library (~35 fns)"
  },
  {
    id: "r1b-synthetic",
    sourcePath: join(
      FIXTURES_ROOT,
      "r1b-synthetic/build/v1.0.0/build/index.js"
    ),
    description: "synthetic store fixture (~13 fns)"
  }
];

// ── Marker-aware perturbation ───────────────────────────────────────────

/**
 * Inject `console.log("<marker>")` into a specific function.
 * Returns the modified source and the marker string for later removal.
 */
function injectMarker(
  source: string,
  targetFunctionName: string
): { source: string; marker: string } {
  const marker = `__PERTURB_${randomUUID().slice(0, 8)}__`;
  const ast = parseSync(source, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse source");

  let injected = false;

  const logStmt = t.expressionStatement(
    t.callExpression(
      t.memberExpression(t.identifier("console"), t.identifier("log")),
      [t.stringLiteral(marker)]
    )
  );

  traverse(ast, {
    FunctionDeclaration(path: NodePath<t.FunctionDeclaration>) {
      if (injected || path.node.id?.name !== targetFunctionName) return;
      path.node.body.body.unshift(t.cloneNode(logStmt, true));
      injected = true;
      path.stop();
    },
    FunctionExpression(path: NodePath<t.FunctionExpression>) {
      if (injected) return;
      const match =
        path.node.id?.name === targetFunctionName ||
        (t.isVariableDeclarator(path.parent) &&
          t.isIdentifier(path.parent.id) &&
          path.parent.id.name === targetFunctionName);
      if (!match) return;
      path.node.body.body.unshift(t.cloneNode(logStmt, true));
      injected = true;
      path.stop();
    },
    ArrowFunctionExpression(path: NodePath<t.ArrowFunctionExpression>) {
      if (injected) return;
      if (
        !t.isVariableDeclarator(path.parent) ||
        !t.isIdentifier(path.parent.id) ||
        path.parent.id.name !== targetFunctionName
      )
        return;
      if (t.isBlockStatement(path.node.body)) {
        path.node.body.body.unshift(t.cloneNode(logStmt, true));
      } else {
        path.node.body = t.blockStatement([
          t.cloneNode(logStmt, true),
          t.returnStatement(path.node.body)
        ]);
      }
      injected = true;
      path.stop();
    },
    ObjectMethod(path: NodePath<t.ObjectMethod>) {
      if (injected) return;
      if (
        !t.isIdentifier(path.node.key) ||
        path.node.computed ||
        path.node.key.name !== targetFunctionName
      )
        return;
      path.node.body.body.unshift(t.cloneNode(logStmt, true));
      injected = true;
      path.stop();
    }
  });

  if (!injected) {
    throw new Error(
      `injectMarker: could not find function "${targetFunctionName}"`
    );
  }

  return { source: generate(ast, { retainLines: false }).code, marker };
}

// ── Result types ────────────────────────────────────────────────────────

interface RunResult {
  library: string;
  functionName: string;
  minifier: string;
  blastRadius: BlastRadius;
  matchResult: "PASS" | "FAIL";
  matched: number;
  ambiguous: number;
  unmatched: number;
  total: number;
  /** Source-level ground truth: all hashes should match after marker removal */
  expectedMatches: number;
  resolutionStats: Record<string, number>;
}

interface MinifierSummary {
  minifier: string;
  avgCategory: string;
  avgFunctionsAffected: number;
  passRate: number;
  failures: string[];
}

// ── Main logic ──────────────────────────────────────────────────────────

async function runOne(
  library: CorpusEntry,
  functionName: string,
  minifierId: string,
  originalMinified: Map<string, { code: string; path: string }>,
  allConfigs: typeof MINIFIER_CONFIGS,
  enablePropagation = false
): Promise<RunResult> {
  const source = readFileSync(library.sourcePath, "utf-8");
  const minifierConfig = allConfigs.find((c) => c.id === minifierId)!;

  // Inject marker into the target function
  const { source: perturbedSource, marker } = injectMarker(
    source,
    functionName
  );

  // Source-level ground truth: after removing the marker, structures are identical
  const sourceGT = computeSourceGroundTruth(source, perturbedSource);

  // Minify the perturbed source
  const perturbedMin = await minifyString(
    perturbedSource,
    minifierConfig,
    "perturbed.js"
  );

  // Remove the marker from minified output
  const cleanedCode = removeMarker(perturbedMin.code, marker);

  // Get the cached original minified code
  const origMin = originalMinified.get(minifierId)!;

  // Measure blast radius: original.min vs cleaned.min
  const blastRadius = measureBlastRadius(origMin.code, cleanedCode);

  // Build fingerprint indices and run matcher
  const origData = buildFingerprintData(origMin.code, origMin.path);
  const cleanedData = buildFingerprintData(cleanedCode, "cleaned.min.js");
  const matchResult = matchFunctions(origData.index, cleanedData.index, {
    enablePropagation
  });
  const stats = getMatchStats(matchResult);

  const pass = stats.ambiguous === 0 && stats.unmatched === 0;

  return {
    library: library.id,
    functionName,
    minifier: minifierId,
    blastRadius,
    matchResult: pass ? "PASS" : "FAIL",
    matched: stats.matched,
    ambiguous: stats.ambiguous,
    unmatched: stats.unmatched,
    total: stats.total,
    expectedMatches: sourceGT.expectedMatches,
    resolutionStats: matchResult.resolutionStats as unknown as Record<
      string,
      number
    >
  };
}

// ── Output formatting ───────────────────────────────────────────────────

function pad(v: string | number, w: number): string {
  return String(v).padEnd(w);
}

function printTable(rows: RunResult[]): void {
  console.log(
    "\n" +
      pad("LIBRARY", 16) +
      pad("FUNCTION", 24) +
      pad("MINIFIER", 16) +
      pad("BLAST", 10) +
      pad("FNS_AFFECTED", 14) +
      pad("MATCH", 7) +
      pad("AMB", 5) +
      pad("UNM", 5) +
      "RESULT"
  );
  console.log("─".repeat(100));

  for (const r of rows) {
    const fnsAffected = `${r.blastRadius.functionsAffected}/${r.blastRadius.totalFunctions}`;
    console.log(
      pad(r.library, 16) +
        pad(r.functionName, 24) +
        pad(r.minifier, 16) +
        pad(r.blastRadius.category, 10) +
        pad(fnsAffected, 14) +
        pad(r.matched, 7) +
        pad(r.ambiguous, 5) +
        pad(r.unmatched, 5) +
        r.matchResult
    );
  }
}

function buildMinifierSummaries(rows: RunResult[]): MinifierSummary[] {
  const byMinifier = new Map<string, RunResult[]>();
  for (const r of rows) {
    const arr = byMinifier.get(r.minifier) ?? [];
    arr.push(r);
    byMinifier.set(r.minifier, arr);
  }

  const summaries: MinifierSummary[] = [];
  for (const [minifier, mRows] of byMinifier) {
    const avgFns =
      mRows.reduce((s, r) => s + r.blastRadius.functionsAffected, 0) /
      mRows.length;

    // Most common category
    const catCounts = new Map<string, number>();
    for (const r of mRows) {
      const c = r.blastRadius.category;
      catCounts.set(c, (catCounts.get(c) ?? 0) + 1);
    }
    const avgCategory = [...catCounts.entries()].sort(
      (a, b) => b[1] - a[1]
    )[0][0];

    const passes = mRows.filter((r) => r.matchResult === "PASS").length;
    const failures = mRows
      .filter((r) => r.matchResult === "FAIL")
      .map((r) => `${r.library}:${r.functionName}`);

    summaries.push({
      minifier,
      avgCategory,
      avgFunctionsAffected: Math.round(avgFns * 10) / 10,
      passRate: passes / mRows.length,
      failures
    });
  }

  return summaries;
}

function printSummary(summaries: MinifierSummary[]): void {
  console.log(
    "\n" +
      pad("MINIFIER", 16) +
      pad("AVG_BLAST", 12) +
      pad("AVG_FNS", 10) +
      pad("PASS_RATE", 12) +
      "FAILURES"
  );
  console.log("─".repeat(80));

  for (const s of summaries) {
    const passRate = `${(s.passRate * 100).toFixed(0)}%`;
    const failStr = s.failures.length === 0 ? "(none)" : s.failures.join(", ");
    console.log(
      pad(s.minifier, 16) +
        pad(s.avgCategory, 12) +
        pad(s.avgFunctionsAffected, 10) +
        pad(passRate, 12) +
        failStr
    );
  }
}

function printFailures(rows: RunResult[]): void {
  const failures = rows.filter((r) => r.matchResult === "FAIL");
  if (failures.length === 0) {
    console.log("\nNo matching failures — every perturbation matched 100%.");
    return;
  }
  console.log(
    `\n${failures.length} runs had matching failures (AMB or UNM > 0):`
  );
  for (const r of failures) {
    console.log(
      `  ${r.library} / ${r.functionName} / ${r.minifier}: ` +
        `matched=${r.matched}, amb=${r.ambiguous}, unm=${r.unmatched}, ` +
        `blast=${r.blastRadius.category} (${r.blastRadius.functionsAffected}/${r.blastRadius.totalFunctions} fns)`
    );
  }
}

// ── CLI ─────────────────────────────────────────────────────────────────

function parseArgs(argv: string[]): {
  library: string | null;
  bun: boolean;
  propagation: boolean;
} {
  let library: string | null = null;
  let bun = false;
  let propagation = false;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--library" && i + 1 < argv.length) {
      library = argv[i + 1];
      i++;
    }
    if (argv[i] === "--bun") {
      bun = true;
    }
    if (argv[i] === "--propagation") {
      propagation = true;
    }
  }
  return { library, bun, propagation };
}

async function main(): Promise<void> {
  const {
    library: filterLibrary,
    bun: includeBun,
    propagation: enablePropagation
  } = parseArgs(process.argv.slice(2));

  if (enablePropagation) {
    console.log("Call-graph propagation: ENABLED");
  }

  const minifierConfigs = includeBun
    ? [...MINIFIER_CONFIGS, BUN_BUNDLE_CONFIG]
    : MINIFIER_CONFIGS;

  const corpus = filterLibrary
    ? CORPUS.filter((c) => c.id === filterLibrary)
    : CORPUS;

  if (corpus.length === 0) {
    console.error(
      `Unknown library: ${filterLibrary}. Available: ${CORPUS.map((c) => c.id).join(", ")}`
    );
    process.exit(1);
  }

  const allRows: RunResult[] = [];

  for (const lib of corpus) {
    const source = readFileSync(lib.sourcePath, "utf-8");
    const functionNames = discoverPerturbableNames(source);

    console.log(
      `\n${lib.id}: ${functionNames.length} perturbable functions (${lib.description})`
    );
    console.log(`  Functions: ${functionNames.join(", ")}`);

    // Pre-minify the unperturbed original for each minifier (cached)
    const originalMinified = new Map<string, { code: string; path: string }>();
    for (const mc of minifierConfigs) {
      try {
        const result = await minifyString(source, mc, `${lib.id}.js`);
        originalMinified.set(mc.id, {
          code: result.code,
          path: result.minifiedPath
        });
      } catch {
        console.log(`  Skipping ${mc.id} for ${lib.id} (minification failed)`);
      }
    }

    // Run each (function × minifier) combination
    for (const fnName of functionNames) {
      for (const mc of minifierConfigs) {
        if (!originalMinified.has(mc.id)) continue;
        process.stdout.write(`  ${lib.id} · ${fnName} · ${mc.id} ... `);
        try {
          const row = await runOne(
            lib,
            fnName,
            mc.id,
            originalMinified,
            minifierConfigs,
            enablePropagation
          );
          allRows.push(row);
          const { blastRadius, matchResult, ambiguous } = row;
          console.log(
            `${matchResult} blast=${blastRadius.category}(${blastRadius.functionsAffected}/${blastRadius.totalFunctions}) amb=${ambiguous}`
          );
        } catch (err) {
          console.log(`ERROR: ${(err as Error).message}`);
        }
      }
    }
  }

  // Print results
  printTable(allRows);
  const summaries = buildMinifierSummaries(allRows);
  printSummary(summaries);
  printFailures(allRows);

  // Write JSON results
  const outputPath = join(import.meta.dirname, "results", "sensitivity.json");
  mkdirSync(dirname(outputPath), { recursive: true });

  const output = {
    timestamp: new Date().toISOString(),
    corpus: corpus.map((c) => ({
      id: c.id,
      description: c.description
    })),
    minifiers: minifierConfigs.map((m) => m.id),
    totalRuns: allRows.length,
    passCount: allRows.filter((r) => r.matchResult === "PASS").length,
    failCount: allRows.filter((r) => r.matchResult === "FAIL").length,
    summaries,
    rows: allRows
  };

  writeFileSync(outputPath, JSON.stringify(output, null, 2), "utf-8");
  console.log(`\nWrote ${outputPath}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

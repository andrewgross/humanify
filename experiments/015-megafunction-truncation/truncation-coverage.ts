/**
 * Truncation coverage: which functions exceed the LLM code cap
 * (MAX_CODE_LINES = 500 in src/rename/processor.ts), and how many of their
 * OWNED bindings are invisible in the truncated code the LLM is shown.
 *
 * A binding is "visible" iff a word-boundary match for its name appears in
 * the first CAP lines of the function's generated code — exactly the text
 * `truncateFunctionCode` would send. Invisible bindings are asked for by
 * name with no declaration/usage in sight: the LLM omits them (→
 * unrenamed.missing → minifier-reroll noise) or names them blind (→
 * asymmetric / transfer-gap noise). This is the before/after coverage
 * metric for experiment 015.
 *
 * The input is prepared exactly as the pipeline prepares it: unpack via the
 * detected adapter (bun: factories extracted, refs rewritten), then the
 * babel beautify plugin. Pass a raw bundle, or --prepared to skip both
 * steps when the input is already the pre-rename beautified runtime.js.
 *
 * Usage (large bundles need a big heap):
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/015-megafunction-truncation/truncation-coverage.ts \
 *     <bundle.js> [--prepared] [--save-prepared <path>] [--cap 500] \
 *     [--diag <cc-diag.json>] [--json <out.json>]
 *
 * e.g. the v120 input of the exp014 round-3 run:
 *   ... truncation-coverage.ts \
 *     /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js \
 *     --save-prepared /tmp/exp015/v120-prepared.js \
 *     --diag /tmp/exp014-round3/cc-120-diag.json
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type { FunctionNode } from "../../src/analysis/types.js";
import { generate } from "../../src/babel-utils.js";
import { detectBundle } from "../../src/detection/detect.js";
import { buildPipelineConfig } from "../../src/pipeline/config.js";
import { createBabelPlugin } from "../../src/plugins/babel/babel.js";
import { collectOwnedBindingInfos } from "../../src/rename/function-bindings.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";
import { selectUnpackAdapter } from "../../src/unpack/index.js";

interface BindingCoverage {
  name: string;
  /** 1-based line of the declaration relative to the function start (input locs). */
  declLineInFn: number | null;
  visible: boolean;
}

interface OversizedFunction {
  id: string;
  genLines: number;
  /** 1-based input line range (locs survive renames — output lines match). */
  locStartLine: number | null;
  locEndLine: number | null;
  bindings: number;
  eligible: number;
  visible: number;
  invisible: number;
  invisibleNames: string[];
  perBinding: BindingCoverage[];
  /** Segmentability probe: top-level body statements + the largest one. */
  bodyStatements: number;
  largestStmtLines: number;
}

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

import { identifierRegex } from "../../src/utils/identifier-regex.js";

const wordRegex = identifierRegex;

function takeFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  if (idx === -1) return undefined;
  const value = args[idx + 1];
  args.splice(idx, 2);
  return value;
}

function takeBool(args: string[], flag: string): boolean {
  const idx = args.indexOf(flag);
  if (idx === -1) return false;
  args.splice(idx, 1);
  return true;
}

interface Args {
  inputPath: string;
  prepared: boolean;
  savePrepared?: string;
  cap: number;
  diagPath?: string;
  jsonPath?: string;
}

function parseArgs(): Args {
  const args = process.argv.slice(2);
  const prepared = takeBool(args, "--prepared");
  const savePrepared = takeFlag(args, "--save-prepared");
  const cap = Number(takeFlag(args, "--cap") ?? "500");
  const diagPath = takeFlag(args, "--diag");
  const jsonPath = takeFlag(args, "--json");
  const [inputPath] = args;
  if (!inputPath) {
    console.error(
      "usage: truncation-coverage.ts <bundle.js> [--prepared] [--save-prepared <path>] [--cap 500] [--diag <diag.json>] [--json <out.json>]"
    );
    process.exit(1);
  }
  return { inputPath, prepared, savePrepared, cap, diagPath, jsonPath };
}

/** Unpack + beautify exactly as the pipeline does, returning runtime.js text. */
async function prepareRuntime(bundlePath: string): Promise<string> {
  const bundledCode = fs.readFileSync(bundlePath, "utf-8");
  const detection = detectBundle(bundledCode);
  // Same overrides the exp013 harness passes (--bundler bun --minifier bun);
  // detection alone returns passthrough for the decompiled entrypoints.
  const config = buildPipelineConfig(detection, {
    bundlerOverride: "bun",
    minifierOverride: "bun"
  });
  console.log(
    `bundler=${config.bundlerType} adapter=${config.unpackAdapterName}`
  );
  const adapter = selectUnpackAdapter(config);
  const outDir = fs.mkdtempSync(path.join(os.tmpdir(), "exp015-unpack-"));
  const { files } = await adapter.unpack(bundledCode, outDir);
  const runtime =
    files.find((f) => path.basename(f.path) === "runtime.js") ?? files[0];
  console.log(`unpacked ${files.length} file(s), using ${runtime.path}`);
  const code = fs.readFileSync(runtime.path, "utf-8");
  const beautify = createBabelPlugin();
  const beautified = await beautify(code);
  fs.rmSync(outDir, { recursive: true, force: true });
  return beautified;
}

/**
 * Functions whose ORIGINAL loc span suggests they might exceed the cap.
 * Beautified-input lines track generated lines closely (both are babel
 * output); the margin catches drift. Only candidates pay a generate().
 */
function locSpan(fn: FunctionNode): number {
  const loc = fn.path.node.loc;
  if (!loc) return Number.MAX_SAFE_INTEGER; // no loc — measure it precisely
  return loc.end.line - loc.start.line + 1;
}

function analyzeFunction(
  fn: FunctionNode,
  cap: number,
  isEligible: (name: string) => boolean
): OversizedFunction | null {
  const code = generate(fn.path.node).code;
  const lines = code.split("\n");
  if (lines.length <= cap) return null;

  const truncated = lines.slice(0, cap).join("\n");
  const all = collectOwnedBindingInfos(fn.path);
  const eligibleBindings = all.filter((b) => isEligible(b.name));
  const fnStartLine = fn.path.node.loc?.start.line;

  const perBinding: BindingCoverage[] = eligibleBindings.map((b) => {
    const declLine = b.identifier.loc?.start.line;
    return {
      name: b.name,
      declLineInFn:
        declLine !== undefined && fnStartLine !== undefined
          ? declLine - fnStartLine + 1
          : null,
      visible: wordRegex(b.name).test(truncated)
    };
  });

  const invisible = perBinding.filter((b) => !b.visible);
  const body = fn.path.node.body;
  let bodyStatements = 0;
  let largestStmtLines = 0;
  if (body.type === "BlockStatement") {
    bodyStatements = body.body.length;
    for (const stmt of body.body) {
      const span = stmt.loc ? stmt.loc.end.line - stmt.loc.start.line + 1 : 0;
      if (span > largestStmtLines) largestStmtLines = span;
    }
  }
  return {
    id: fn.sessionId,
    genLines: lines.length,
    locStartLine: fn.path.node.loc?.start.line ?? null,
    locEndLine: fn.path.node.loc?.end.line ?? null,
    bindings: all.length,
    eligible: eligibleBindings.length,
    visible: perBinding.length - invisible.length,
    invisible: invisible.length,
    invisibleNames: invisible.map((b) => b.name),
    perBinding,
    bodyStatements,
    largestStmtLines
  };
}

/** Join oversized-function ids against the run diag's unrenamed.missing. */
function crossCheckDiag(
  diagPath: string,
  oversized: OversizedFunction[]
): void {
  const diag = JSON.parse(fs.readFileSync(diagPath, "utf-8")) as {
    unrenamed?: { missing?: Array<{ name: string; functionId: string }> };
  };
  const missing = diag.unrenamed?.missing ?? [];
  const byId = new Map(oversized.map((f) => [f.id, f]));
  const invisibleSets = new Map(
    oversized.map((f) => [f.id, new Set(f.invisibleNames)])
  );

  let inOversized = 0;
  let inInvisibleSet = 0;
  for (const entry of missing) {
    const fn = byId.get(entry.functionId);
    if (!fn) continue;
    inOversized++;
    if (invisibleSets.get(entry.functionId)?.has(entry.name)) {
      inInvisibleSet++;
    }
  }
  console.log(`\n--- diag cross-check (${diagPath}) ---`);
  console.log(
    `unrenamed.missing total:                 ${fmt(missing.length)}`
  );
  console.log(
    `  in an oversized function:              ${fmt(inOversized)} (${((100 * inOversized) / Math.max(1, missing.length)).toFixed(1)}%)`
  );
  console.log(
    `  name is in that fn's INVISIBLE set:    ${fmt(inInvisibleSet)}`
  );
}

async function main(): Promise<void> {
  const { inputPath, prepared, savePrepared, cap, diagPath, jsonPath } =
    parseArgs();
  const t0 = Date.now();

  const code = prepared
    ? fs.readFileSync(inputPath, "utf-8")
    : await prepareRuntime(inputPath);
  if (savePrepared) {
    fs.mkdirSync(path.dirname(savePrepared), { recursive: true });
    fs.writeFileSync(savePrepared, code);
    console.log(`saved prepared input to ${savePrepared}`);
  }

  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);

  // The wrapper IIFE is pre-done (markWrapperPreDone) and its bindings go
  // through the module-binding path (code-less per-identifier profiles) —
  // truncation never applies to it.
  const wrapperNode = graph.wrapperPath?.node;
  const functions: FunctionNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    if (wrapperNode && node.node.path.node === wrapperNode) continue;
    functions.push(node.node);
  }
  console.log(
    `graph: ${fmt(functions.length)} functions (${((Date.now() - t0) / 1000).toFixed(1)}s)`
  );

  // Candidate filter on input loc span, then measure precisely.
  const candidates = functions.filter((fn) => locSpan(fn) > cap * 0.8);
  const oversized: OversizedFunction[] = [];
  for (const fn of candidates) {
    const result = analyzeFunction(fn, cap, isEligible);
    if (result) oversized.push(result);
  }
  oversized.sort((a, b) => b.genLines - a.genLines);

  const totalEligible = oversized.reduce((s, f) => s + f.eligible, 0);
  const totalInvisible = oversized.reduce((s, f) => s + f.invisible, 0);
  const withInvisible = oversized.filter((f) => f.invisible > 0);

  console.log(`\n=== truncation coverage (cap ${cap} lines) ===`);
  console.log(`functions total (excl. wrapper): ${fmt(functions.length)}`);
  console.log(`candidates (loc-span):      ${fmt(candidates.length)}`);
  console.log(`OVERSIZED (gen > cap):      ${fmt(oversized.length)}`);
  console.log(`  with >=1 invisible:       ${fmt(withInvisible.length)}`);
  console.log(`  eligible bindings:        ${fmt(totalEligible)}`);
  console.log(
    `  INVISIBLE past the cap:   ${fmt(totalInvisible)} (${((100 * totalInvisible) / Math.max(1, totalEligible)).toFixed(1)}% of eligible)`
  );

  console.log(
    `\n${"id".padEnd(28)} ${"genLines".padStart(8)} ${"bindings".padStart(8)} ${"eligible".padStart(8)} ${"visible".padStart(8)} ${"INVIS".padStart(6)}`
  );
  for (const f of oversized) {
    console.log(
      `${f.id.padEnd(28)} ${fmt(f.genLines).padStart(8)} ${fmt(f.bindings).padStart(8)} ${fmt(f.eligible).padStart(8)} ${fmt(f.visible).padStart(8)} ${fmt(f.invisible).padStart(6)}`
    );
  }

  // Sample of invisible names for the biggest offenders, for spot checks.
  for (const f of oversized.slice(0, 5)) {
    if (f.invisibleNames.length === 0) continue;
    console.log(
      `\n  ${f.id} invisible (first 20): ${f.invisibleNames.slice(0, 20).join(", ")}`
    );
  }

  if (diagPath) crossCheckDiag(diagPath, oversized);

  if (jsonPath) {
    fs.mkdirSync(path.dirname(jsonPath), { recursive: true });
    fs.writeFileSync(
      jsonPath,
      JSON.stringify({ cap, input: inputPath, oversized }, null, 2)
    );
    console.log(`\nwrote per-function coverage to ${jsonPath}`);
  }

  console.log(`\ntotal ${((Date.now() - t0) / 1000).toFixed(1)}s`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

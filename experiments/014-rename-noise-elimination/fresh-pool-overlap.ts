/**
 * Fresh-pool overlap: how much of the incremental run's UNMATCHED v-new
 * function pool actually exists in the prior version?
 *
 * The incremental (--prior-version) leg settles functions three ways:
 * exact match (names transferred), close match (LLM with prior context),
 * or fresh (no counterpart found — full-cost LLM naming, unstable names,
 * rename-noise). This diagnostic reruns the REAL matcher entry point
 * (matchPriorVersion) on the same inputs and classifies every new-side
 * function the matcher did NOT exact-match:
 *
 *   (a) exact structural-hash twin exists in the prior  → recoverable miss
 *         - 1:1 buckets (hash unique on BOTH sides): pure matcher bug —
 *           the only path here is the singleton corroboration gate
 *         - ambiguous buckets: identity cracking failed, or surplus
 *           copies (more new-side holders than prior twins)
 *   (b) no exact twin, but the pipeline close-matched it → changed code
 *   (c) neither                                          → genuinely new
 *
 * Plus the reverse: prior functions that did not exact-match (disappeared
 * / changed / missed), classified the same way.
 *
 * Graphs are built exactly as the pipeline builds them: parseSync +
 * buildUnifiedGraph on BEAUTIFIED code, target filename "input.js" (so
 * sessionIds align with diag functionIds), prior filename "prior.js" (so
 * sessionIds align with matchPriorVersion's internal prior graph).
 *
 * Usage (large bundles need a big heap):
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/014-rename-noise-elimination/fresh-pool-overlap.ts \
 *     <prior-humanified.js> <new-beautified.js> \
 *     [--diag <cc-NEW-diag.json>] [--dump <dispositions.json>]
 *
 * --diag cross-checks counts against the real run's coverage summary;
 * --dump writes per-function dispositions for id-level joins.
 *
 * e.g. the phase-6 v119→v120 run:
 *   ... fresh-pool-overlap.ts /tmp/exp013-phase6/cc-119/runtime.js \
 *       /tmp/exp013-anomaly/v120-beautified.js \
 *       --diag /tmp/exp013-phase6/cc-120-diag.json
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import type { FunctionNode } from "../../src/analysis/types.js";
import { generate } from "../../src/babel-utils.js";

interface PriorFnRecord {
  id: string;
  hash: string;
  /** Humanified display name (fn id / var declarator / object key), if any. */
  name: string | null;
}

function hms(ms: number): string {
  return `${(ms / 1000).toFixed(1)}s`;
}

function fmt(n: number): string {
  return n.toLocaleString("en-US");
}

function pct(part: number, whole: number): string {
  return whole > 0 ? `${((100 * part) / whole).toFixed(1)}%` : "n/a";
}

function buildFunctionMap(
  code: string,
  filename: string
): Map<string, FunctionNode> {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error(`parse failed: ${filename}`);
  const graph = buildUnifiedGraph(ast, filename, undefined, undefined, code);
  const functions = new Map<string, FunctionNode>();
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
  }
  return functions;
}

/** Best-effort humanified display name for a prior function node. */
function displayName(fn: FunctionNode): string | null {
  const node = fn.path.node;
  if (
    (t.isFunctionDeclaration(node) || t.isFunctionExpression(node)) &&
    node.id
  ) {
    return node.id.name;
  }
  const parent = fn.path.parentPath?.node;
  if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
    return parent.id.name;
  }
  if (
    (t.isObjectProperty(parent) || t.isClassMethod(parent)) &&
    t.isIdentifier(parent.key)
  ) {
    return parent.key.name;
  }
  return null;
}

/** Extract compact per-function records, letting the graph be released. */
function capturePriorRecords(priorCode: string, t0: number): PriorFnRecord[] {
  const priorMap = buildFunctionMap(priorCode, "prior.js");
  const records: PriorFnRecord[] = [...priorMap.values()].map((fn) => ({
    id: fn.sessionId,
    hash: fn.fingerprint.structuralHash,
    name: displayName(fn)
  }));
  console.log(
    `prior graph: ${fmt(records.length)} functions ${hms(Date.now() - t0)}`
  );
  return records;
}

function countBy<T>(items: Iterable<T>, key: (item: T) => string) {
  const counts = new Map<string, number>();
  for (const item of items) {
    const k = key(item);
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  return counts;
}

type Category = "exact-twin" | "close-only" | "no-counterpart";

interface Classified {
  id: string;
  hash: string;
  category: Category;
  nPrior: number;
  nNew: number;
  closeMatched: boolean;
}

function classifyUnmatched(
  ids: string[],
  hashOf: (id: string) => string,
  priorHashCounts: Map<string, number>,
  newHashCounts: Map<string, number>,
  closeMatchedIds: Set<string>
): Classified[] {
  return ids.map((id) => {
    const hash = hashOf(id);
    const nPrior = priorHashCounts.get(hash) ?? 0;
    const nNew = newHashCounts.get(hash) ?? 0;
    const closeMatched = closeMatchedIds.has(id);
    let category: Category;
    if (nPrior > 0) category = "exact-twin";
    else if (closeMatched) category = "close-only";
    else category = "no-counterpart";
    return { id, hash, category, nPrior, nNew, closeMatched };
  });
}

/** Bucket-size pattern for an exact-twin miss, e.g. "1:1", "m:n". */
function bucketPattern(c: Classified): string {
  const p = c.nPrior === 1 ? "1" : "m";
  const n = c.nNew === 1 ? "1" : "n";
  return `${p}:${n}`;
}

/**
 * Split exact-twin misses into avoidable (enough prior twins existed —
 * the matcher failed on ambiguity or a gate) vs unavoidable surplus
 * (more same-hash holders on this side than the other side has twins —
 * even a perfect 1:1 matcher must leave nThis−nOther unmatched; only a
 * hash→name cache recovers those).
 */
function splitAvoidable(twinMisses: Classified[]): {
  avoidable: number;
  unavoidable: number;
} {
  const byHash = new Map<string, Classified[]>();
  for (const c of twinMisses) {
    const list = byHash.get(c.hash) ?? [];
    list.push(c);
    byHash.set(c.hash, list);
  }
  let avoidable = 0;
  let unavoidable = 0;
  for (const [, misses] of byHash) {
    const { nPrior, nNew } = misses[0];
    // Counts are side-symmetric: for the new side surplus = nNew−nPrior,
    // for the prior side (records passed with swapped counts) the same
    // formula applies because callers swap nPrior/nNew when classifying.
    const surplus = Math.max(0, nNew - nPrior);
    const unavoidableHere = Math.min(misses.length, surplus);
    unavoidable += unavoidableHere;
    avoidable += misses.length - unavoidableHere;
  }
  return { avoidable, unavoidable };
}

function printCategoryBlock(
  label: string,
  classified: Classified[],
  totalUnmatched: number
): Classified[] {
  const twins = classified.filter((c) => c.category === "exact-twin");
  const closeOnly = classified.filter((c) => c.category === "close-only");
  const fresh = classified.filter((c) => c.category === "no-counterpart");

  const oneToOne = twins.filter((c) => c.nPrior === 1 && c.nNew === 1);
  const ambiguousTwins = twins.filter((c) => !(c.nPrior === 1 && c.nNew === 1));
  const patterns = countBy(ambiguousTwins, bucketPattern);
  const { avoidable, unavoidable } = splitAvoidable(twins);
  const twinsCloseMatched = twins.filter((c) => c.closeMatched).length;

  console.log(`\n--- ${label} (${fmt(totalUnmatched)} functions) ---`);
  console.log(
    `(a) EXACT hash twin on other side:  ${fmt(twins.length).padStart(7)}  (${pct(twins.length, totalUnmatched)})  <- recoverable`
  );
  console.log(
    `      1:1 buckets (unique both sides — PURE MATCHER MISS): ${fmt(oneToOne.length)}`
  );
  console.log(
    `      ambiguous buckets (multi-member either side):        ${fmt(ambiguousTwins.length)}`
  );
  const patternStr = [...patterns.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([k, v]) => `${k}=${fmt(v)}`)
    .join("  ");
  if (patternStr) console.log(`        by bucket pattern: ${patternStr}`);
  console.log(
    `      avoidable (enough twins existed): ${fmt(avoidable)}   unavoidable surplus copies: ${fmt(unavoidable)}`
  );
  console.log(
    `      of (a), pipeline close-matched anyway: ${fmt(twinsCloseMatched)}   fell fully fresh: ${fmt(twins.length - twinsCloseMatched)}`
  );
  console.log(
    `(b) no twin, CLOSE-matched (changed code):   ${fmt(closeOnly.length).padStart(7)}  (${pct(closeOnly.length, totalUnmatched)})`
  );
  console.log(
    `(c) no plausible counterpart (genuinely new/gone): ${fmt(fresh.length).padStart(7)}  (${pct(fresh.length, totalUnmatched)})`
  );
  return oneToOne;
}

function printSamples(
  title: string,
  items: Classified[],
  describe: (c: Classified) => string,
  limit = 5
): void {
  if (items.length === 0) return;
  console.log(
    `\n  [${title}] ${fmt(items.length)} total, first ${Math.min(limit, items.length)}:`
  );
  for (const c of items.slice(0, limit)) {
    console.log(`    ${describe(c)}`);
  }
}

/** One-line code preview for a live function node. */
function preview(fn: FunctionNode | undefined): string {
  if (!fn) return "";
  try {
    return generate(fn.path.node).code.replace(/\s+/g, " ").slice(0, 90);
  } catch {
    return "<codegen failed>";
  }
}

interface DiagCoverage {
  total: number;
  llm: number;
  cached: number;
  closeMatch: number;
  alreadyNamed: number;
  nothingToRename: number;
  failed: number;
}

function printDiagConsistency(
  diagPath: string,
  newTotal: number,
  exactMatched: number,
  closeMatched: number,
  freshPool: number
): void {
  const diag = JSON.parse(fs.readFileSync(diagPath, "utf-8")) as {
    coverage?: { functions?: DiagCoverage };
  };
  const cov = diag.coverage?.functions;
  if (!cov) {
    console.log(`\n[diag] ${diagPath}: no coverage.functions — skipping`);
    return;
  }
  const diagExact = cov.cached + cov.alreadyNamed;
  const diagFresh = cov.total - diagExact - cov.closeMatch;
  console.log(`\n--- diag consistency (${diagPath}) ---`);
  console.log(
    `  functions total:   diag ${fmt(cov.total)}   this run ${fmt(newTotal)}   Δ ${fmt(newTotal - cov.total)}`
  );
  console.log(
    `  exact-matched:     diag ${fmt(diagExact)} (cached ${fmt(cov.cached)} + alreadyNamed ${fmt(cov.alreadyNamed)})   this run ${fmt(exactMatched)}   Δ ${fmt(exactMatched - diagExact)}`
  );
  console.log(
    `  close-matched:     diag ${fmt(cov.closeMatch)}   this run ${fmt(closeMatched)}   Δ ${fmt(closeMatched - cov.closeMatch)}`
  );
  console.log(
    `  fresh pool:        diag ${fmt(diagFresh)} (total−exact−close; llm bucket ${fmt(cov.llm)} incl. close-match fns)   this run ${fmt(freshPool)}   Δ ${fmt(freshPool - diagFresh)}`
  );
}

interface Args {
  priorPath: string;
  newPath: string;
  diagPath?: string;
  dumpPath?: string;
}

function takeFlag(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  if (idx === -1) return undefined;
  const value = args[idx + 1];
  args.splice(idx, 2);
  return value;
}

function parseArgs(): Args {
  const args = process.argv.slice(2);
  const diagPath = takeFlag(args, "--diag");
  const dumpPath = takeFlag(args, "--dump");
  const [priorPath, newPath] = args;
  if (!priorPath || !newPath) {
    console.error(
      "usage: fresh-pool-overlap.ts <prior-humanified.js> <new-beautified.js> [--diag <diag.json>] [--dump <dispositions.json>]"
    );
    process.exit(1);
  }
  return { priorPath, newPath, diagPath, dumpPath };
}

async function main(): Promise<void> {
  const { priorPath, newPath, diagPath, dumpPath } = parseArgs();
  const t0 = Date.now();
  console.log(`=== fresh-pool-overlap ===`);
  console.log(`prior: ${priorPath}`);
  console.log(`new:   ${newPath}`);

  // 1. Prior graph, script-side, ONLY to capture [id, hash, name] — then
  // release it (matchPriorVersion builds its own prior graph internally;
  // filename "prior.js" makes the sessionIds join).
  const priorCode = fs.readFileSync(priorPath, "utf-8");
  const priorRecords = capturePriorRecords(priorCode, t0);
  (globalThis as { gc?: () => void }).gc?.();

  const priorHashCounts = countBy(priorRecords, (r) => r.hash);
  const priorIdsByHash = new Map<string, string[]>();
  const priorById = new Map<string, PriorFnRecord>();
  for (const r of priorRecords) {
    priorById.set(r.id, r);
    const list = priorIdsByHash.get(r.hash) ?? [];
    list.push(r.id);
    priorIdsByHash.set(r.hash, list);
  }

  // 2. Target graph, built exactly as the pipeline builds it (filename
  // "input.js" — sessionIds align with the run diag's functionIds).
  const t1 = Date.now();
  const newCode = fs.readFileSync(newPath, "utf-8");
  const functions = buildFunctionMap(newCode, "input.js");
  console.log(
    `new graph:   ${fmt(functions.size)} functions ${hms(Date.now() - t1)}`
  );
  const newHashCounts = countBy(
    functions.values(),
    (fn) => fn.fingerprint.structuralHash
  );

  // 3. The production matcher (same cascade + propagation the run used).
  const t2 = Date.now();
  const result = matchPriorVersion(priorCode, functions);
  console.log(`matchPriorVersion: ${hms(Date.now() - t2)}`);
  const { matchResult, closeMatchContext } = result;

  const matchedNewIds = new Set(matchResult.matches.values());
  const matchedPriorIds = new Set(matchResult.matches.keys());
  const closeMatchedPriorIds = new Set(
    [...closeMatchContext.values()].map((info) => info.priorId)
  );

  const unmatchedNewIds = [...functions.keys()].filter(
    (id) => !matchedNewIds.has(id)
  );
  const unmatchedPriorIds = priorRecords
    .map((r) => r.id)
    .filter((id) => !matchedPriorIds.has(id));
  const freshPoolCount = unmatchedNewIds.filter(
    (id) => !closeMatchContext.has(id)
  ).length;

  console.log(`\n--- pipeline match result (HEAD matcher) ---`);
  console.log(`new functions total:       ${fmt(functions.size)}`);
  console.log(
    `exact-matched:             ${fmt(matchResult.matches.size)}  (${pct(matchResult.matches.size, functions.size)})`
  );
  console.log(
    `close-matched:             ${fmt(closeMatchContext.size)}  (${pct(closeMatchContext.size, functions.size)})`
  );
  console.log(
    `fresh pool (neither):      ${fmt(freshPoolCount)}  (${pct(freshPoolCount, functions.size)})`
  );
  console.log(
    `resolution stats: ${JSON.stringify(matchResult.resolutionStats)}`
  );

  // 4. Forward classification: every new fn the matcher did NOT exact-match.
  const hashOfNew = (id: string): string => {
    const fn = functions.get(id);
    if (!fn) throw new Error(`unknown new fn ${id}`);
    return fn.fingerprint.structuralHash;
  };
  const newClassified = classifyUnmatched(
    unmatchedNewIds,
    hashOfNew,
    priorHashCounts,
    newHashCounts,
    new Set(closeMatchContext.keys())
  );
  const newOneToOne = printCategoryBlock(
    "v-new functions NOT exact-matched",
    newClassified,
    unmatchedNewIds.length
  );

  // Fresh-pool-only view (what actually got full-cost fresh LLM naming).
  const freshOnly = newClassified.filter((c) => !c.closeMatched);
  const freshTwins = freshOnly.filter((c) => c.category === "exact-twin");
  const freshOneToOne = freshTwins.filter(
    (c) => c.nPrior === 1 && c.nNew === 1
  );
  console.log(`\n--- FRESH POOL only (not exact, not close) ---`);
  console.log(`total:                      ${fmt(freshOnly.length)}`);
  console.log(
    `  exact twin in prior:      ${fmt(freshTwins.length)}  (1:1 ${fmt(freshOneToOne.length)}, ambiguous ${fmt(freshTwins.length - freshOneToOne.length)})`
  );
  console.log(
    `  no counterpart:           ${fmt(freshOnly.length - freshTwins.length)}`
  );

  // 5. Reverse classification: prior fns that did not exact-match.
  const hashOfPrior = (id: string): string => {
    const r = priorById.get(id);
    if (!r) throw new Error(`unknown prior fn ${id}`);
    return r.hash;
  };
  // Note swapped count maps: for the prior side, "twin" means the hash
  // exists in the NEW version, and surplus is nPrior−nNew.
  const priorClassified = classifyUnmatched(
    unmatchedPriorIds,
    hashOfPrior,
    newHashCounts,
    priorHashCounts,
    closeMatchedPriorIds
  );
  printCategoryBlock(
    "REVERSE: v-prior functions NOT exact-matched (disappeared side)",
    priorClassified,
    unmatchedPriorIds.length
  );
  console.log(
    `  (pipeline's own unmatched list — hash absent or singleton-rejected: ${fmt(matchResult.unmatched.length)}; still-ambiguous prior fns: ${fmt(matchResult.ambiguous.size)})`
  );

  // 6. Samples for spot-checking.
  console.log(`\n=== samples ===`);
  const describeNew = (c: Classified): string => {
    const twinIds = priorIdsByHash.get(c.hash) ?? [];
    const twin = twinIds.length > 0 ? priorById.get(twinIds[0]) : undefined;
    const twinStr = twin
      ? ` twin=${twin.id}${twin.name ? ` (${twin.name})` : ""}${twinIds.length > 1 ? ` +${twinIds.length - 1} more` : ""}`
      : "";
    const closeStr = c.closeMatched
      ? ` close→${closeMatchContext.get(c.id)?.priorId}`
      : "";
    return `${c.id} hash=${c.hash} bucket=${c.nPrior}:${c.nNew}${twinStr}${closeStr} | ${preview(functions.get(c.id))}`;
  };
  printSamples("NEW 1:1 pure matcher miss", newOneToOne, describeNew);
  printSamples(
    "NEW exact-twin, ambiguous bucket",
    newClassified.filter(
      (c) => c.category === "exact-twin" && !(c.nPrior === 1 && c.nNew === 1)
    ),
    describeNew
  );
  printSamples(
    "NEW close-only (changed code)",
    newClassified.filter((c) => c.category === "close-only"),
    describeNew
  );
  printSamples(
    "NEW no-counterpart (genuinely new)",
    newClassified.filter((c) => c.category === "no-counterpart"),
    describeNew
  );
  const describePrior = (c: Classified): string => {
    const r = priorById.get(c.id);
    return `${c.id}${r?.name ? ` (${r.name})` : ""} hash=${c.hash} bucket(new:prior)=${c.nPrior}:${c.nNew}`;
  };
  printSamples(
    "PRIOR disappeared (no counterpart in new)",
    priorClassified.filter((c) => c.category === "no-counterpart"),
    describePrior
  );

  // 7. Optional per-function disposition dump (id-level joins, e.g.
  // against a diag's renamed functionIds).
  if (dumpPath) {
    const enrich = (c: Classified) => {
      const twinIds = priorIdsByHash.get(c.hash) ?? [];
      return {
        ...c,
        closePriorId: closeMatchContext.get(c.id)?.priorId,
        twinIds: twinIds.slice(0, 3),
        twinName: priorById.get(twinIds[0] ?? "")?.name ?? undefined
      };
    };
    const dump = {
      prior: priorPath,
      new: newPath,
      newUnmatched: newClassified.map(enrich),
      priorUnmatched: priorClassified,
      exactMatches: [...matchResult.matches.entries()]
    };
    fs.writeFileSync(dumpPath, JSON.stringify(dump));
    console.log(`\nwrote dispositions to ${dumpPath}`);
  }

  // 8. Diag consistency.
  if (diagPath) {
    printDiagConsistency(
      diagPath,
      functions.size,
      matchResult.matches.size,
      closeMatchContext.size,
      freshPoolCount
    );
  }

  console.log(`\ntotal ${hms(Date.now() - t0)}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

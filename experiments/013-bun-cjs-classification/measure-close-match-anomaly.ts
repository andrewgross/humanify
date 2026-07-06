/**
 * A/B the close-match anomaly, in the pipeline's TRUE representation.
 *
 * The real pipeline beautifies code (createBabelPlugin: void 0→undefined,
 * comparison flips, bautifier statement-splitting) BEFORE the rename
 * plugin matches it — and the humanified prior is beautified output too.
 * Raw↔raw match rates (measure-binding-match.ts, 37,086 exact + 5,884
 * close) therefore are NOT comparable to the full run's numbers; matching
 * a raw target against a beautified prior mismatches everywhere (found by
 * this harness's first, wrong, raw-target version: exact ≈ 0 and the
 * close-match cross-product overflowed V8's max array length).
 *
 * Corrected design — target is ALWAYS beautified v120:
 *   prior=beautified  → beautified v119, NOT renamed (control)
 *   prior=humanified  → cc-119/runtime.js (beautified + renamed, Run A')
 * The control isolates beautification; the delta between legs isolates
 * renaming (C10 name-keyed placeholder instability + cascade effects).
 * Expected humanified leg ≈ the real Run B' diag: 24,463 exact + 17,445
 * close on 43,198 functions.
 *
 * Usage (one leg per process — matchPriorVersion mutates the target graph):
 *   node --max-old-space-size=16384 --expose-gc --import tsx/esm \
 *     experiments/013-bun-cjs-classification/measure-close-match-anomaly.ts \
 *     prep
 *   ... run --prior beautified --out /tmp/exp013-anomaly/leg-beautified.json
 *   ... run --prior humanified --out /tmp/exp013-anomaly/leg-humanified.json
 *   ... compare /tmp/exp013-anomaly/leg-beautified.json /tmp/exp013-anomaly/leg-humanified.json
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import { createBabelPlugin } from "../../src/plugins/babel/babel.js";
import type { FunctionNode } from "../../src/analysis/types.js";

const V119_RAW = "/tmp/exp013-remeasure/v119/runtime.js";
const V120_RAW = "/tmp/exp013-remeasure/v120/runtime.js";
const V119_BEAUTIFIED = "/tmp/exp013-anomaly/v119-beautified.js";
const V120_BEAUTIFIED = "/tmp/exp013-anomaly/v120-beautified.js";
const TARGET = V120_BEAUTIFIED;
const PRIORS: Record<string, string> = {
  beautified: V119_BEAUTIFIED,
  humanified: "/tmp/exp013-phase2/cc-119/runtime.js"
};

/** Beautify raw runtime.js files exactly as the pipeline does, cached. */
async function prep() {
  const babelPlugin = createBabelPlugin();
  for (const [src, dst] of [
    [V119_RAW, V119_BEAUTIFIED],
    [V120_RAW, V120_BEAUTIFIED]
  ]) {
    if (fs.existsSync(dst)) {
      console.log(`[cached] ${dst}`);
      continue;
    }
    const t0 = Date.now();
    const out = await babelPlugin(fs.readFileSync(src, "utf-8"));
    fs.mkdirSync(path.dirname(dst), { recursive: true });
    fs.writeFileSync(dst, out);
    console.log(`beautified ${src} → ${dst} ${hms(Date.now() - t0)}`);
  }
}

interface Disposition {
  id: string;
  hash: string;
  disp: "exact" | "close" | "none";
  priorId?: string;
}

interface LegDump {
  priorKind: string;
  priorPath: string;
  summary: {
    targetFunctions: number;
    exact: number;
    close: number;
    none: number;
    priorFunctions: number;
  };
  resolutionStats: Record<string, number>;
  dispositions: Disposition[];
  /** Prior-side functions in graph iteration order: [sessionId, structuralHash] */
  priorFns: [string, string][];
}

function hms(ms: number) {
  return `${(ms / 1000).toFixed(1)}s`;
}

function buildFunctionMap(code: string, filename: string) {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error(`parse failed: ${filename}`);
  const graph = buildUnifiedGraph(ast, filename, undefined, undefined, code);
  const functions = new Map<string, FunctionNode>();
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
  }
  return functions;
}

async function runLeg(priorKind: string, outPath: string) {
  const priorPath = PRIORS[priorKind];
  if (!priorPath) throw new Error(`unknown prior kind: ${priorKind}`);
  const t0 = Date.now();

  // 1. Prior graph built script-side ONLY to dump [id, hash] in iteration
  // order (matchPriorVersion builds its own internally and doesn't expose
  // it). Extract plain strings, then release before the real match run.
  const priorCode = fs.readFileSync(priorPath, "utf-8");
  // Filename must match matchPriorVersion's internal graph build ("prior.js")
  // so sessionIds from matchResult join against this dump.
  let priorFns: [string, string][] = [];
  {
    const priorMap = buildFunctionMap(priorCode, "prior.js");
    priorFns = [...priorMap.values()].map((fn) => [
      fn.sessionId,
      fn.fingerprint.structuralHash
    ]);
    console.log(
      `prior graph (${priorKind}): ${priorFns.length} functions ${hms(Date.now() - t0)}`
    );
  }
  (globalThis as { gc?: () => void }).gc?.();

  // 2. Target graph + the production matching entry point.
  const t1 = Date.now();
  const newCode = fs.readFileSync(TARGET, "utf-8");
  const functions = buildFunctionMap(newCode, "runtime.js");
  console.log(
    `target graph: ${functions.size} functions ${hms(Date.now() - t1)}`
  );

  const t2 = Date.now();
  const result = matchPriorVersion(priorCode, functions);
  console.log(`matchPriorVersion ${hms(Date.now() - t2)}`);

  // matches: priorId → newId. Invert for target-keyed lookup.
  const exactByNewId = new Map<string, string>();
  for (const [priorId, newId] of result.matchResult.matches) {
    exactByNewId.set(newId, priorId);
  }

  const dispositions: Disposition[] = [];
  let exact = 0;
  let close = 0;
  let none = 0;
  for (const [id, fn] of functions) {
    const hash = fn.fingerprint.structuralHash;
    const exactPrior = exactByNewId.get(id);
    if (exactPrior) {
      exact++;
      dispositions.push({ id, hash, disp: "exact", priorId: exactPrior });
      continue;
    }
    const closeInfo = result.closeMatchContext.get(id);
    if (closeInfo) {
      close++;
      dispositions.push({
        id,
        hash,
        disp: "close",
        priorId: closeInfo.priorId
      });
      continue;
    }
    none++;
    dispositions.push({ id, hash, disp: "none" });
  }

  const dump: LegDump = {
    priorKind,
    priorPath,
    summary: {
      targetFunctions: functions.size,
      exact,
      close,
      none,
      priorFunctions: priorFns.length
    },
    resolutionStats: result.matchResult.resolutionStats as unknown as Record<
      string,
      number
    >,
    dispositions,
    priorFns
  };

  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, JSON.stringify(dump));
  console.log(
    `\n=== LEG ${priorKind} ===\n` +
      `exact ${exact}  close ${close}  none ${none}  ` +
      `(target ${functions.size}, prior ${priorFns.length})\n` +
      `stats: ${JSON.stringify(dump.resolutionStats)}\n` +
      `wrote ${outPath}  total ${hms(Date.now() - t0)}`
  );
}

function loadLeg(p: string): LegDump {
  return JSON.parse(fs.readFileSync(p, "utf-8")) as LegDump;
}

function compare(pathA: string, pathB: string) {
  const a = loadLeg(pathA);
  const b = loadLeg(pathB);
  console.log(
    `A: prior=${a.priorKind} exact=${a.summary.exact} close=${a.summary.close} none=${a.summary.none}`
  );
  console.log(
    `B: prior=${b.priorKind} exact=${b.summary.exact} close=${b.summary.close} none=${b.summary.none}`
  );

  const byIdB = new Map(b.dispositions.map((d) => [d.id, d]));

  // 3x3 transition matrix A → B
  const matrix = new Map<string, number>();
  for (const dA of a.dispositions) {
    const dB = byIdB.get(dA.id);
    const key = `${dA.disp}→${dB ? dB.disp : "MISSING"}`;
    matrix.set(key, (matrix.get(key) ?? 0) + 1);
  }
  console.log("\n=== TRANSITIONS (A → B) ===");
  for (const [key, count] of [...matrix.entries()].sort(
    (x, y) => y[1] - x[1]
  )) {
    console.log(`  ${key}: ${count}`);
  }

  // The moved population: exact in A, not exact in B. Discriminate by
  // whether the target's hash exists in B's prior at all.
  const bPriorHashCounts = new Map<string, number>();
  for (const [, hash] of b.priorFns) {
    bPriorHashCounts.set(hash, (bPriorHashCounts.get(hash) ?? 0) + 1);
  }

  const moved = a.dispositions.filter((d) => {
    if (d.disp !== "exact") return false;
    const dB = byIdB.get(d.id);
    return !dB || dB.disp !== "exact";
  });

  let hashAbsent = 0;
  let hashPresent = 0;
  const absentSamples: Disposition[] = [];
  const presentSamples: Disposition[] = [];
  for (const d of moved) {
    if (bPriorHashCounts.has(d.hash)) {
      hashPresent++;
      if (presentSamples.length < 20) presentSamples.push(d);
    } else {
      hashAbsent++;
      if (absentSamples.length < 20) absentSamples.push(d);
    }
  }
  console.log(`\n=== MOVED (exact in A, not exact in B): ${moved.length} ===`);
  console.log(
    `  hash ABSENT from B prior (hash instability → C10/normalization): ${hashAbsent}`
  );
  console.log(
    `  hash PRESENT in B prior (cascade failed to match → C4/C5/C7): ${hashPresent}`
  );

  // Traversal-order alignment raw-prior ↔ humanified-prior: if counts
  // match, index i of A.priorFns is the same source function as index i
  // of B.priorFns (humanify renames identifiers only, order preserved).
  console.log(
    `\n=== PRIOR ALIGNMENT (A ${a.summary.priorFunctions} vs B ${b.summary.priorFunctions} functions) ===`
  );
  if (a.summary.priorFunctions === b.summary.priorFunctions) {
    let hashSame = 0;
    let hashDiff = 0;
    for (let i = 0; i < a.priorFns.length; i++) {
      if (a.priorFns[i][1] === b.priorFns[i][1]) hashSame++;
      else hashDiff++;
    }
    console.log(
      `  index-aligned prior hash equality: same=${hashSame} diff=${hashDiff} ` +
        `(${((100 * hashDiff) / a.priorFns.length).toFixed(1)}% of prior fns changed hash after humanify+regenerate)`
    );
  } else {
    console.log("  counts differ — index alignment unreliable, skipping");
  }

  // Samples for manual drill-down (task 2)
  const aPriorIndex = new Map(a.priorFns.map(([id], i) => [id, i]));
  console.log("\n=== SAMPLES: moved, hash absent from B prior ===");
  for (const d of absentSamples.slice(0, 10)) {
    const rawPriorIdx = d.priorId ? aPriorIndex.get(d.priorId) : undefined;
    const humanifiedCounterpart =
      rawPriorIdx !== undefined &&
      a.summary.priorFunctions === b.summary.priorFunctions
        ? b.priorFns[rawPriorIdx]
        : undefined;
    console.log(
      `  target ${d.id} hash ${d.hash.slice(0, 16)}… ` +
        `rawPrior ${d.priorId} → humanifiedCounterpart ${
          humanifiedCounterpart
            ? `${humanifiedCounterpart[0]} hash ${humanifiedCounterpart[1].slice(0, 16)}…`
            : "?"
        }`
    );
  }
  console.log("\n=== SAMPLES: moved, hash present in B prior ===");
  for (const d of presentSamples.slice(0, 10)) {
    console.log(
      `  target ${d.id} hash ${d.hash.slice(0, 16)}… bucketSizeInBPrior=${bPriorHashCounts.get(d.hash)}`
    );
  }
}

async function main() {
  const [mode, ...rest] = process.argv.slice(2);
  if (mode === "prep") {
    await prep();
  } else if (mode === "run") {
    const priorKind = rest[rest.indexOf("--prior") + 1];
    const out = rest[rest.indexOf("--out") + 1];
    if (!priorKind || !out)
      throw new Error("run --prior <beautified|humanified> --out <json>");
    await runLeg(priorKind, out);
  } else if (mode === "compare") {
    const [pa, pb] = rest;
    if (!pa || !pb) throw new Error("compare <legA.json> <legB.json>");
    compare(pa, pb);
  } else {
    throw new Error("mode must be 'prep', 'run' or 'compare'");
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

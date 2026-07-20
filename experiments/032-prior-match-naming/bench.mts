/**
 * 032 — the prior-match → naming window, re-measured under PER-AST caches.
 *
 * On main (module-level WeakMap caches) this bench demonstrated the ephemeron
 * pathology on real archived ~32MB bundles (see RESULTS.md):
 *   - buildUnifiedGraph NEW into a FRESH table:            49 s
 *   - buildUnifiedGraph PRIOR into the NEW-filled table:  214 s  (4.4×)
 *   - and after dropping the prior, the naming-era workload thrashe(d) the
 *     tombstone-dense tables unless resetAnalysisNodeCaches() ran first.
 *
 * With per-AST caches (src/analysis/analysis-cache.ts) there is no shared
 * table: the NEW graph fills the NEW AST's cache, the PRIOR graph fills the
 * PRIOR AST's cache (fresh by construction), and dropping the prior AST drops
 * its cache wholesale. The claims under test:
 *   1. PRIOR build ≈ NEW build (the 4.4× dense-table penalty is gone);
 *   2. the post-drop workload runs at fresh-table speed with NO reset call
 *      (there is no reset API anymore).
 *
 * Two phases (--phase=):
 *   insert  (default) — after the prior is dropped, bulk-insert a fresh batch
 *           of NEW-AST keys (a second NEW parse, then buildUnifiedGraph).
 *   sig     — repeated whole-Program computeStructuralSignature over the live
 *           NEW AST (the naming-analog read/insert mix).
 *
 * Run under shell `timeout` with a big heap and --expose-gc so the prior AST
 * is deterministically collected before the timed section:
 *
 *   NODE_OPTIONS="--max-old-space-size=14336 --expose-gc" \
 *     timeout 1800 npx tsx bench.mts --phase=insert
 *
 * Bundle paths default to the SAFE COPIES in ~/Development/humanify-bench-data
 * (never point this at the active walk tree); override with NEW_BUNDLE /
 * PRIOR_BUNDLE.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import {
  parseFileAst,
  parseSourceAst,
  traverse
} from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/index.js";

const DATA = path.join(os.homedir(), "Development", "humanify-bench-data");
const NEW = fs.readFileSync(
  process.env.NEW_BUNDLE ?? path.join(DATA, "2.1.208-humanified.js"),
  "utf8"
);
const PRIOR = fs.readFileSync(
  process.env.PRIOR_BUNDLE ?? path.join(DATA, "2.1.207-humanified.js"),
  "utf8"
);

const phaseArg = process.argv.find((a) => a.startsWith("--phase="));
const phase = phaseArg?.split("=")[1] ?? "insert";
const maybeGc = (globalThis as { gc?: () => void }).gc;

function rssMB(): number {
  return Math.round(process.memoryUsage().rss / 1048576);
}

function time<T>(label: string, fn: () => T): T {
  const t0 = performance.now();
  const r = fn();
  console.log(
    `  ${label}: ${Math.round(performance.now() - t0)}ms  rss ${rssMB()}MB`
  );
  return r;
}

function programSignature(ast: t.File): string {
  let sig = "";
  traverse(ast, {
    Program(path) {
      sig = computeStructuralSignature(path).slice(0, 8);
      path.stop();
    }
  });
  return sig;
}

console.log(
  `arm: PER-AST CACHES (no reset API)  phase: ${phase}  ` +
    `new ${(NEW.length / 1e6).toFixed(1)}MB  prior ${(PRIOR.length / 1e6).toFixed(1)}MB` +
    `${maybeGc ? "" : "  [WARN: no --expose-gc, prior-AST collection not forced]"}`
);

// Step 1 — naming-era state: parse NEW + build its graph; HELD LIVE below.
let newAst: t.File | null = time("parse+graph NEW (held live)", () => {
  const ast = parseFileAst(NEW) as t.File;
  const g = buildUnifiedGraph(ast, "input.js", NULL_PROFILER, () => true, NEW);
  console.log(`    new graph nodes: ${g.nodes.size}`);
  return ast;
});

// Step 2 — prior-version matching analog: parse PRIOR (preserveAstCaches, as
// the matcher does) + build its graph, then DROP both. Under per-AST caches
// the prior build fills the PRIOR AST's OWN cache — claim 1 is this build's
// time vs step 1's.
time("parse+graph PRIOR (own per-AST cache → dropped)", () => {
  let priorAst: t.File | null = parseSourceAst(PRIOR, {
    preserveAstCaches: true
  }) as t.File;
  let priorGraph: { nodes: Map<unknown, unknown> } | null = buildUnifiedGraph(
    priorAst,
    "prior.js",
    NULL_PROFILER,
    () => true,
    PRIOR
  );
  console.log(`    prior graph nodes: ${priorGraph.nodes.size}`);
  priorAst = null;
  priorGraph = null;
});

// The prior AST (and with it, its whole AnalysisCache) is now unreachable;
// force GC so the timed section runs after its collection.
time("force GC (prior AST + its cache collected)", () => {
  maybeGc?.();
  maybeGc?.();
});

// Step 3 — the timed naming-analog workload over the NEW AST, with NO reset.
if (phase === "sig") {
  const ITERS = 4;
  for (let i = 0; i < ITERS; i++) {
    const sig = time(`Program signature iter ${i + 1}`, () =>
      programSignature(newAst as t.File)
    );
    if (i === 0) console.log(`    (sig ${sig})`);
  }
} else {
  let fresh: t.File | null = time("parse NEW #2 (fresh AST, fresh cache)", () =>
    parseSourceAst(NEW, { preserveAstCaches: true })
  ) as t.File | null;
  time("buildUnifiedGraph over fresh NEW (bulk cache insert)", () => {
    const g = buildUnifiedGraph(
      fresh as t.File,
      "input2.js",
      NULL_PROFILER,
      () => true,
      NEW
    );
    console.log(`    fresh graph nodes: ${g.nodes.size}`);
  });
  fresh = null;
}

console.log(`done (new still live: ${(newAst as t.File).type})`);
newAst = null;

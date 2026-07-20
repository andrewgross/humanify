/**
 * 032 — the prior-match → naming ephemeron window.
 *
 * Reproduces, on REAL archived ~32MB bundles, the exact heap state the naming
 * pass inherits after prior-version matching returns, and measures whether
 * resetting the node-keyed caches at that boundary removes the V8 ephemeron
 * thrash.
 *
 * The window (src/rename/plugin.ts createRenamePlugin):
 *   1. parse NEW bundle (funnel resets → fresh cache era) + buildUnifiedGraph
 *      → the naming-era AST + caches, HELD LIVE for the whole pass;
 *   2. prior-version matching parses the PRIOR bundle with
 *      preserveAstCaches:true (the funnel deliberately does NOT reset — the
 *      matcher reads hash/binding entries keyed by BOTH ASTs at once), builds
 *      a PRIOR UnifiedGraph (fills the 3 node caches + Babel's path cache with
 *      millions of PRIOR-AST keys), matches, then RETURNS — dropping the prior
 *      AST + graph. Those millions of keys are now tombstones.
 *   3. naming runs its node-cache ops over the NEW AST. Bulk-inserting /
 *      reading through the tombstone-dense tables makes V8 re-hash the backing
 *      store on nearly every op → O(n^2) 100%-CPU/flat-RSS hang.
 *
 * THE FIX under test: resetAnalysisNodeCaches() + clearBabelTraverseCache()
 * after step 2, before step 3 (gated on a prior). The caches are pure
 * deterministic memoization, so a reset only forces recompute-on-demand.
 *
 * Two arms (run each in its OWN process for a fresh heap):
 *   (default)  THRASH — no reset after dropping the prior.
 *   --reset    FIX    — reset the caches after dropping the prior.
 *
 * Two phases (--phase=):
 *   insert  (default) — the cleanest pathology: after the prior is dropped,
 *           bulk-insert a fresh batch of NEW-AST keys (a second NEW parse with
 *           preserveAstCaches, then buildUnifiedGraph) — "the next bulk-insert
 *           into the tombstone-dense WeakMap". Parse cost is identical across
 *           arms; the DELTA is the tombstone rehash overhead.
 *   sig     — the task's literal naming-analog: repeated whole-Program
 *           computeStructuralSignature over the live NEW AST (every identifier
 *           does a bindingByIdentifierNode.get, the traverse fills Babel's
 *           path cache). Serialization-heavy, so the cache delta is a smaller
 *           fraction of each iteration.
 *
 * Run under shell `timeout` (a quadratic cannot self-interrupt) with a big
 * heap and --expose-gc so the prior AST is deterministically collected into
 * tombstones before the timed section:
 *
 *   NODE_OPTIONS="--max-old-space-size=14336 --expose-gc" \
 *     timeout 900 npx tsx bench.mts --phase=insert [--reset]
 */
import fs from "node:fs";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { resetAnalysisNodeCaches } from "../../src/analysis/node-caches.js";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import {
  clearBabelTraverseCache,
  parseFileAst,
  parseSourceAst,
  traverse
} from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/index.js";

const V =
  process.env.VERSIONS_ROOT ??
  "/Users/andrewgross/Development/unpacked-claude-code-run-2026-07-17/versions";
const NEW = fs.readFileSync(
  `${V}/claude-code-2.1.208/.humanify/humanified.js`,
  "utf8"
);
const PRIOR = fs.readFileSync(
  `${V}/claude-code-2.1.207/.humanify/humanified.js`,
  "utf8"
);

const reset = process.argv.includes("--reset");
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
  `arm: ${reset ? "RESET (fix)" : "THRASH (no reset)"}  phase: ${phase}  ` +
    `new ${(NEW.length / 1e6).toFixed(1)}MB  prior ${(PRIOR.length / 1e6).toFixed(1)}MB` +
    `${maybeGc ? "" : "  [WARN: no --expose-gc, tombstone collection not forced]"}`
);

// Step 1 — naming-era state: parse NEW (funnel resets to a fresh era) + build
// its graph; HELD LIVE below so its cache keys stay live (NOT tombstones).
let newAst: t.File | null = time("parse+graph NEW (held live)", () => {
  const ast = parseFileAst(NEW) as t.File;
  const g = buildUnifiedGraph(ast, "input.js", NULL_PROFILER, () => true, NEW);
  console.log(`    new graph nodes: ${g.nodes.size}`);
  return ast;
});

// Step 2 — prior-version matching analog: parse PRIOR (preserveAstCaches, as
// the matcher does) + build its graph (fills caches with PRIOR-AST keys), then
// DROP both, mirroring matchPriorVersion returning.
time("parse+graph PRIOR (preserveAstCaches → dropped)", () => {
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

// The prior AST is now unreachable; force GC so its millions of node keys
// become tombstones — the exact table state at the start of naming.
time("force GC (prior AST → tombstones)", () => {
  maybeGc?.();
  maybeGc?.();
});

// THE FIX under test — only the RESET arm swaps the tombstone-dense husks for
// fresh tables here, at the prior-match → naming boundary.
if (reset) {
  time("resetAnalysisNodeCaches + clearBabelTraverseCache", () => {
    resetAnalysisNodeCaches();
    clearBabelTraverseCache();
  });
}

// Step 3 — the timed naming-analog workload over the NEW AST.
if (phase === "sig") {
  const ITERS = 4;
  for (let i = 0; i < ITERS; i++) {
    const sig = time(`Program signature iter ${i + 1}`, () =>
      programSignature(newAst as t.File)
    );
    if (i === 0) console.log(`    (sig ${sig})`);
  }
} else {
  // Insert stress: a fresh batch of NEW-AST keys bulk-inserted into whatever
  // table state exists. preserveAstCaches so this second parse does NOT itself
  // reset the caches (that is the funnel's job at step 1, not here).
  let fresh: t.File | null = time("parse NEW #2 (preserve; cache untouched)", () =>
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

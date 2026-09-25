// WP3.2 probe: the prior-version transfer stage's strategy trail on small
// synthetic pairs, run on the REAL TS (`applyPriorVersionIfPresent` in
// src/rename/prior-transfer.ts over a `buildUnifiedGraph` of the fresh
// text — the plugin's own call sequence, minus the eval/wrapper freezes no
// case here triggers). Prints, per case, the trail rows in the
// transfers.json shape (span-sorted; the texts are ASCII, so the UTF-16
// spans are the UTF-8 offsets the Rust dump keys by). The Rust port
// (`rename::transfer`, test `synthetic_pairs_match_the_ts_probe`) runs the
// same cases through `prior::match_prior_version` + `apply_prior_version`
// and must reproduce every row.
//
// Run: npx tsx test/parity/wp32-transfer-probe.ts > test/parity/wp32-transfers.json
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type { FunctionNode } from "../../src/analysis/types.js";
import { parseSourceAst } from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/profiler.js";
import { applyPriorVersionIfPresent } from "../../src/rename/prior-transfer.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";
import { strategyTrail } from "../../src/rename/strategy-trail.js";

import { CASES } from "./wp32-transfer-cases.js";

function runCase(prior: string, fresh: string) {
  const ast = parseSourceAst(fresh);
  if (!ast) throw new Error("fresh text failed to parse");
  const graph = buildUnifiedGraph(
    ast,
    "input.js",
    NULL_PROFILER,
    createIsEligible(),
    fresh
  );
  const allFunctions: FunctionNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") allFunctions.push(node.node);
  }
  strategyTrail.reset(true);
  applyPriorVersionIfPresent(prior, allFunctions, graph, NULL_PROFILER);
  return strategyTrail
    .report()
    .trails.map((e) => ({
      target: {
        text: e.declText ?? "fresh",
        start: e.declSpan?.start ?? -1,
        end: e.declSpan?.end ?? -1
      },
      oldName: e.oldName,
      finalName: e.finalName ?? null,
      settledBy: e.settledBy ?? null,
      attempts: e.trail.map((a) => ({
        tier: a.strategy,
        outcome: a.outcome,
        ...(a.reason !== undefined ? { reason: a.reason } : {}),
        ...(a.newName !== undefined ? { proposedName: a.newName } : {})
      }))
    }))
    .sort(
      (a, b) => a.target.start - b.target.start || a.target.end - b.target.end
    );
}

const out = CASES.map((c) => ({
  name: c.name,
  prior: c.prior,
  fresh: c.fresh,
  transfers: runCase(c.prior, c.fresh)
}));
process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);

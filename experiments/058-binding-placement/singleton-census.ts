/**
 * Task 0a — what the "0 singleton rejections out of 37,966" actually measures,
 * on a REAL version pair.
 *
 *   npx tsx experiments/058-binding-placement/singleton-census.ts <priorBundle> <freshBundle>
 *
 * `singleton-guard-probe.ts` establishes the mechanism: of the guard's three
 * tests, `propertyAccesses` and `externalCalls` are functions of the bucket key
 * (0 disagreements in 3,037 multi-member buckets), so only `memberKey` can
 * reject anything — and module BINDINGS carry neither, so the guard is absent
 * from that cascade entirely.
 *
 * This counts the consequence where it matters: over every singleton-bucket
 * accept on a real hop, how often did the guard have a testable signal at all?
 * A guard that is asked nothing cannot report a precision.
 *
 * The singleton branch of `runMatchingPass` is replicated here rather than
 * instrumented in place: it is four lines, and the alternative is a debug flag
 * on a hot path in production code.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import {
  buildBindingFingerprintIndex,
  buildFingerprintIndex
} from "../../src/analysis/fingerprint-index.js";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type {
  FingerprintIndex,
  FunctionNode,
  ModuleBindingNode,
  UnifiedGraph
} from "../../src/analysis/types.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: singleton-census.ts <priorBundle> <freshBundle>");
  process.exit(1);
}

function graphOf(path: string): UnifiedGraph {
  const ast = parseSync(fs.readFileSync(path, "utf8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`could not parse ${path}`);
  return buildUnifiedGraph(ast, path);
}
function functionsOf(g: UnifiedGraph): Map<string, FunctionNode> {
  const out = new Map<string, FunctionNode>();
  for (const [id, n] of g.nodes) if (n.type === "function") out.set(id, n.node);
  return out;
}
function bindingsOf(g: UnifiedGraph): ModuleBindingNode[] {
  const out: ModuleBindingNode[] = [];
  for (const n of g.nodes.values())
    if (n.type === "module-binding") out.push(n.node);
  return out;
}

/** The singleton branch's census over one pair of indices. */
function census(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  label: string
): void {
  let singleton = 0;
  let bothMemberKey = 0;
  let memberKeyDisagrees = 0;
  let bothFeatures = 0;
  let featuresDisagree = 0;
  for (const [, oldFp] of oldIndex.fingerprints) {
    const candidates =
      newIndex.byStructuralHash.get(oldFp.structuralHash) ?? [];
    if (candidates.length !== 1) continue;
    singleton++;
    const newFp = newIndex.fingerprints.get(candidates[0]);
    if (!newFp) continue;
    if (oldFp.memberKey !== undefined && newFp.memberKey !== undefined) {
      bothMemberKey++;
      if (oldFp.memberKey !== newFp.memberKey) memberKeyDisagrees++;
    }
    const of = oldFp.features;
    const nf = newFp.features;
    if (of && nf) {
      bothFeatures++;
      const eq = (a: readonly string[], b: readonly string[]) =>
        a.length === b.length && a.every((x, i) => x === b[i]);
      if (
        !eq(of.propertyAccesses, nf.propertyAccesses) ||
        !eq(of.externalCalls, nf.externalCalls)
      ) {
        featuresDisagree++;
      }
    }
  }
  const pct = (n: number) => `${((100 * n) / (singleton || 1)).toFixed(1)}%`;
  console.log(`\n  --- ${label} ---`);
  console.log(`  singleton-bucket accepts        : ${singleton}`);
  console.log(
    `  guard had a testable memberKey  : ${bothMemberKey}  (${pct(bothMemberKey)})`
  );
  console.log(`    ... and it DISAGREED (rejects) : ${memberKeyDisagrees}`);
  console.log(
    `  guard had testable features     : ${bothFeatures}  (${pct(bothFeatures)})`
  );
  console.log(`    ... and they DISAGREED         : ${featuresDisagree}`);
  console.log(
    `  guard asked NOTHING at all      : ${singleton - Math.max(bothMemberKey, bothFeatures)}` +
      `  (${pct(singleton - Math.max(bothMemberKey, bothFeatures))})`
  );
}

console.log(`prior: ${PRIOR}\nfresh: ${FRESH}`);
const priorGraph = graphOf(PRIOR);
const freshGraph = graphOf(FRESH);

census(
  buildFingerprintIndex(functionsOf(priorGraph)),
  buildFingerprintIndex(functionsOf(freshGraph)),
  "FUNCTION cascade"
);
census(
  buildBindingFingerprintIndex(bindingsOf(priorGraph)),
  buildBindingFingerprintIndex(bindingsOf(freshGraph)),
  "MODULE-BINDING cascade — the one exp058 would promote"
);

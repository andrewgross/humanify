/**
 * Would corroborating a binding singleton with calleeHashes / twoHopShapes
 * actually REJECT anything? (task #13, sized before building.)
 *
 *   npx tsx experiments/lib/size-binding-singleton-guard.ts <prior.js> <new.js>
 *
 * ## Why this measurement, specifically
 *
 * `singletonVerdict` guards a zero-corroboration match: a hash bucket with
 * exactly one candidate is accepted unless a version-stable signal contradicts.
 * It reads `memberKey` and `features`, and `buildBindingFullFingerprint` sets
 * NEITHER — so on the module-binding cascade the guard examines nothing. That
 * was 11,094 accepts with 0 examined on 2.1.215->216, reported as
 * `singletonRejected: 0`, which reads exactly like perfect precision.
 *
 * The obvious fix is to corroborate with the fields bindings DO carry:
 * `calleeHashes` and `twoHopShapes`. But those may be DETERMINED BY THE BUCKET
 * KEY. Bindings are bucketed by `structuralHash`, which hashes the binding's
 * initializer — and the initializer is what produces its callees. If the callee
 * set is a function of the hash, then within a bucket the two fingerprints can
 * never disagree, the guard can never fire, and we would have rebuilt the exact
 * failure we are trying to fix: a predicate that cannot test what its name
 * implies.
 *
 * So the number that decides this is not "how many singletons are there" but
 * "among same-hash pairs, how often do calleeHashes or twoHopShapes DIFFER".
 * If that is ~0, the corroboration is circular and must not be built.
 *
 * `--dump N` prints N disagreeing pairs with their names and initializer text,
 * which is the only way to answer the question the counts cannot: is a
 * disagreement a WRONG ACCEPT (two unrelated bindings sharing a hash) or a
 * RIGHT ONE (the same binding whose callees legitimately changed)? Only the
 * first kind is worth rejecting.
 *
 * Runs no LLM and writes nothing.
 *
 * ## MEASURED 2026-08-03
 *
 *                                  2.1.85->2.1.86    2.1.197->2.1.198
 *   binding singletons                     4,678             6,762
 *   with NO callee evidence at all         4,170 (89.14%)    6,006 (88.82%)
 *   calleeHashes differ                      365 (7.80%)       569 (8.41%)
 *   twoHopShapes differ                        1 (0.02%)        10 (0.15%)
 *   CEILING on rejections                    365               569
 *
 * THREE conclusions, and they do not all point the same way:
 *
 * 1. NOT CIRCULAR. `calleeHashes` disagree within a bucket ~8% of the time, so
 *    they are not determined by the bucket key. A guard built on them CAN
 *    fire. That was the question that had to be answered first, and the answer
 *    clears the corroboration to be built at all.
 *
 * 2. IT WOULD STILL BE MOSTLY BLIND. ~89% of binding singletons carry NO
 *    callee evidence whatsoever, so `singletonUnguarded` would remain the
 *    dominant outcome. Building this does NOT turn the binding cascade's
 *    guard into a real precision measure; it turns 11% of it into one. Anyone
 *    reading `singletonRejected` afterwards would still be reading a number
 *    that covers a minority of accepts — which is the exact misreading that
 *    created this task.
 *
 * 3. DO NOT ADD twoHopShapes. It disagrees on 0.02%-0.15% of pairs and
 *    contributes nothing calleeHashes does not already catch (`eitherDiffers`
 *    equals `calleeHashesDiffer` on both pairs). Including it would be
 *    instrumentation theatre.
 *
 * ## WHY THIS IS NOT YET A SHIP DECISION
 *
 * 365 and 569 are ceilings on REJECTIONS, not counts of BAD accepts. A
 * rejection is only an improvement if the accept was wrong; rejecting a
 * correct match LOSES a name and forces the LLM to re-invent it. exp044 is the
 * precedent and it is not encouraging — alias reservation refused names on
 * principle and cost +3,742 lines, because refusing a name moves a collision
 * rather than removing one (rules 5 and 6).
 *
 * ## READ 2026-08-03 — and the lever is REFUTED, not merely too small
 *
 * Dumped 20 disagreeing pairs on 2.1.85->2.1.86 (`--dump 20`) and read them.
 * EVERY ONE is the same binding. Not one is two unrelated bindings sharing a
 * hash. A representative sample, prior (humanified) vs new (minified):
 *
 *   getMacOSPaths -> w5$     callees 5 vs 3   identical path-building code
 *   getLinuxPaths -> D5$     callees 6 vs 4   identical
 *   parseJsonInput -> b$_    callees 9 vs 8   identical 4-call composition
 *   createComponentVar -> Ze_ callees 14 vs 13 identical class definition
 *   isBlobLike -> Lf8        callees 2 vs 0   identical duck-type predicate
 *   wrapError -> zbH         callees 3 vs 0   identical Error normaliser
 *
 * So the callee sets are NOT comparable across a humanified prior and a
 * minified new file — they are computed asymmetrically, and the `N vs 0` cases
 * show the new side sometimes resolves no internal callees at all where the
 * prior resolved several.
 *
 * CONSEQUENCE: a guard that rejected on calleeHashes disagreement would reject
 * CORRECT matches, losing names the LLM must then re-invent. That is worse than
 * the status quo, not merely too small to measure — exp044's outcome (+3,742
 * lines from refusing names on principle) is the shape of the downside.
 *
 * DO NOT BUILD IT. The 8% "the guard could fire" figure above is real and
 * completely misleading on its own; it is a rate of DISAGREEMENT, and every
 * disagreement read so far was the evidence being wrong, not the match.
 *
 * OPEN QUESTION this raises, deliberately not answered here: the FUNCTION
 * cascade already uses `calleeHashes` as a disambiguation tier
 * (`calleeHashesResolved`). If callee sets are asymmetric prior-vs-new, is that
 * tier reliable? It may well be — disambiguation picks AMONG candidates inside
 * one bucket, and an asymmetry that hits every candidate equally is harmless
 * there, unlike a reject/accept gate. Worth measuring before assuming either
 * way: compare the callee-set sizes of MATCHED prior/new function pairs and see
 * whether the asymmetry is uniform.
 */
import * as fs from "node:fs";
import type * as t from "@babel/types";
import { parseSourceAst } from "../../src/babel-utils.js";
import { buildBindingFingerprintIndex } from "../../src/analysis/fingerprint-index.js";
import {
  buildUnifiedGraph,
  resolveBindingContentPath
} from "../../src/analysis/function-graph.js";
import type {
  FingerprintIndex,
  FunctionFingerprint,
  ModuleBindingNode
} from "../../src/analysis/types.js";

interface Side {
  index: FingerprintIndex;
  /** By sessionId, so a dumped pair can be named and printed. */
  nodes: Map<string, ModuleBindingNode>;
  code: string;
}

/** ONE parse per side; the index and the nodes come from the same graph. */
function loadSide(code: string, label: string): Side {
  const ast = parseSourceAst(code);
  if (!ast) throw new Error(`could not parse ${label}`);
  const graph = buildUnifiedGraph(ast as t.File, label);
  const bindings: ModuleBindingNode[] = [];
  const nodes = new Map<string, ModuleBindingNode>();
  for (const node of graph.nodes.values()) {
    if (node.type !== "module-binding") continue;
    bindings.push(node.node);
    nodes.set(node.node.sessionId, node.node);
  }
  return { index: buildBindingFingerprintIndex(bindings), nodes, code };
}

const same = (a?: string[], b?: string[]): boolean =>
  (a ?? []).length === (b ?? []).length &&
  (a ?? []).every((v, i) => v === (b ?? [])[i]);

function pct(n: number, d: number): string {
  return d === 0 ? "n/a" : `${((100 * n) / d).toFixed(2)}%`;
}

interface Tally {
  singletons: number;
  calleeHashesDiffer: number;
  twoHopDiffer: number;
  eitherDiffers: number;
  bothEmpty: number;
}

/** Walk the singleton buckets and count where the callee evidence disagrees. */
function tally(prior: FingerprintIndex, fresh: FingerprintIndex): Tally {
  const out: Tally = {
    singletons: 0,
    calleeHashesDiffer: 0,
    twoHopDiffer: 0,
    eitherDiffers: 0,
    bothEmpty: 0
  };
  for (const [, oldIds] of prior.byStructuralHash) {
    const pair = singletonPair(prior, fresh, oldIds);
    if (pair) countPair(out, pair.oldFp, pair.newFp);
  }
  return out;
}

/** The (old, new) fingerprints when this bucket is a guarded singleton. */
function singletonPair(
  prior: FingerprintIndex,
  fresh: FingerprintIndex,
  oldIds: string[]
): { oldFp: FunctionFingerprint; newFp: FunctionFingerprint } | null {
  const oldFp = oldIds[0] ? prior.fingerprints.get(oldIds[0]) : undefined;
  if (!oldFp) return null;
  const newIds = fresh.byStructuralHash.get(oldFp.structuralHash);
  // The guarded case: exactly one candidate on the new side.
  if (newIds?.length !== 1) return null;
  const newFp = fresh.fingerprints.get(newIds[0]);
  return newFp ? { oldFp, newFp } : null;
}

function countPair(
  out: Tally,
  oldFp: FunctionFingerprint,
  newFp: FunctionFingerprint
): void {
  out.singletons++;
  const ch = !same(oldFp.calleeHashes, newFp.calleeHashes);
  const th = !same(oldFp.twoHopShapes, newFp.twoHopShapes);
  if (ch) out.calleeHashesDiffer++;
  if (th) out.twoHopDiffer++;
  if (ch || th) out.eitherDiffers++;
  if (
    (oldFp.calleeHashes ?? []).length === 0 &&
    (oldFp.twoHopShapes ?? []).length === 0
  ) {
    out.bothEmpty++;
  }
}

/** The binding's initializer text, truncated — the evidence a reader judges. */
function initText(side: Side, id: string, cap = 220): string {
  const node = side.nodes.get(id);
  if (!node) return "(node not found)";
  const babelBinding = node.scope.bindings[node.name];
  const path = babelBinding ? resolveBindingContentPath(babelBinding) : null;
  const start = path?.node?.start;
  const end = path?.node?.end;
  if (start == null || end == null) return "(no content path)";
  const text = side.code.slice(start, end).replace(/\s+/g, " ");
  return text.length > cap ? `${text.slice(0, cap)}…` : text;
}

/**
 * Print disagreeing pairs so they can be READ.
 *
 * The counts say how OFTEN the callee evidence disagrees. They cannot say
 * whether a disagreement means the accept was wrong. Two unrelated bindings
 * that happen to share a structural hash is a bad accept worth rejecting; the
 * same binding whose callees changed across the release is a GOOD accept, and
 * rejecting it loses a name for nothing. Only reading them tells you which.
 */
function dumpDisagreements(
  priorSide: Side,
  freshSide: Side,
  limit: number
): void {
  let shown = 0;
  for (const [, oldIds] of priorSide.index.byStructuralHash) {
    if (shown >= limit) break;
    const pair = singletonPair(priorSide.index, freshSide.index, oldIds);
    if (!pair) continue;
    if (same(pair.oldFp.calleeHashes, pair.newFp.calleeHashes)) continue;

    const newIds =
      freshSide.index.byStructuralHash.get(pair.oldFp.structuralHash) ?? [];
    const oldId = oldIds[0];
    const newId = newIds[0];
    shown++;
    console.log(`\n--- ${shown} --- hash ${pair.oldFp.structuralHash}`);
    console.log(`  prior name : ${priorSide.nodes.get(oldId)?.name}`);
    console.log(`  new name   : ${freshSide.nodes.get(newId)?.name}`);
    console.log(
      `  calleeHashes prior=${(pair.oldFp.calleeHashes ?? []).length} new=${(pair.newFp.calleeHashes ?? []).length}`
    );
    console.log(`  prior init : ${initText(priorSide, oldId)}`);
    console.log(`  new init   : ${initText(freshSide, newId)}`);
  }
  if (shown === 0) console.log("\n(no disagreeing pairs to show)");
}

function main(): void {
  const [priorPath, newPath] = process.argv.slice(2);
  const dumpIdx = process.argv.indexOf("--dump");
  const dumpN = dumpIdx >= 0 ? Number(process.argv[dumpIdx + 1] ?? 20) : 0;
  if (!priorPath || !newPath) {
    console.error("usage: size-binding-singleton-guard.ts <prior.js> <new.js>");
    process.exit(2);
  }

  const priorSide = loadSide(fs.readFileSync(priorPath, "utf8"), "prior");
  const freshSide = loadSide(fs.readFileSync(newPath, "utf8"), "new");
  const prior = priorSide.index;
  const fresh = freshSide.index;

  const {
    singletons,
    calleeHashesDiffer,
    twoHopDiffer,
    eitherDiffers,
    bothEmpty
  } = tally(prior, fresh);

  console.log(`binding singletons examined:   ${singletons}`);
  console.log(
    `  with NO callee evidence at all: ${bothEmpty} (${pct(bothEmpty, singletons)}) — unguardable either way`
  );
  console.log("");
  console.log("WOULD THE GUARD EVER FIRE? (same hash, evidence disagrees)");
  console.log(
    `  calleeHashes differ: ${calleeHashesDiffer} (${pct(calleeHashesDiffer, singletons)})`
  );
  console.log(
    `  twoHopShapes differ: ${twoHopDiffer} (${pct(twoHopDiffer, singletons)})`
  );
  console.log(
    `  either differs:      ${eitherDiffers} (${pct(eitherDiffers, singletons)})  <-- the ceiling`
  );
  console.log("");
  if (dumpN > 0) {
    console.log(`\n=== ${dumpN} disagreeing pair(s), for reading ===`);
    dumpDisagreements(priorSide, freshSide, dumpN);
    console.log("");
  }
  if (eitherDiffers === 0) {
    console.log(
      "VERDICT: the evidence NEVER disagrees within a bucket — it is determined\n" +
        "by the bucket key. Building this corroboration would produce another\n" +
        "guard that cannot reject anything, which is the defect being fixed.\n" +
        "DO NOT BUILD IT."
    );
  } else {
    console.log(
      `VERDICT: the guard could reject at most ${eitherDiffers} accept(s).\n` +
        "That is a CEILING on rejections, not a count of BAD accepts — each one\n" +
        "still has to be shown wrong before the rejection is an improvement."
    );
  }
}

main();

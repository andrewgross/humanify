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
 * NEXT STEP, before any code: sample ~20 of the differing pairs and read them.
 * Are they genuinely different bindings that share a hash, or the same binding
 * whose callees legitimately changed across the release? Only the first kind
 * is a bad accept. That is the exp058 method — read the disagreements by hand
 * before building the thing that acts on them.
 */
import * as fs from "node:fs";
import type * as t from "@babel/types";
import { parseSourceAst } from "../../src/babel-utils.js";
import { buildBindingFingerprintIndex } from "../../src/analysis/fingerprint-index.js";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type {
  FingerprintIndex,
  FunctionFingerprint,
  ModuleBindingNode
} from "../../src/analysis/types.js";

function bindingIndexOf(code: string, label: string): FingerprintIndex {
  const ast = parseSourceAst(code);
  if (!ast) throw new Error(`could not parse ${label}`);
  const graph = buildUnifiedGraph(ast as t.File, label);
  const bindings: ModuleBindingNode[] = [];
  for (const node of graph.nodes.values()) {
    if (node.type === "module-binding") bindings.push(node.node);
  }
  return buildBindingFingerprintIndex(bindings);
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

function main(): void {
  const [priorPath, newPath] = process.argv.slice(2);
  if (!priorPath || !newPath) {
    console.error("usage: size-binding-singleton-guard.ts <prior.js> <new.js>");
    process.exit(2);
  }

  const prior = bindingIndexOf(fs.readFileSync(priorPath, "utf8"), "prior");
  const fresh = bindingIndexOf(fs.readFileSync(newPath, "utf8"), "new");

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

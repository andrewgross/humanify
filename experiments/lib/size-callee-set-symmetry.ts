/**
 * Are callee sets COMPARABLE between a humanified prior and a minified new
 * file, for FUNCTIONS? (task #20)
 *
 *   npx tsx experiments/lib/size-callee-set-symmetry.ts <prior.js> <new.js> [--dump N]
 *
 * ## Why ask
 *
 * For MODULE BINDINGS the answer is no, established by reading twenty pairs
 * (see `size-binding-singleton-guard.ts`): twenty out of twenty disagreeing
 * singleton pairs were the SAME binding with different callee counts, including
 * `N vs 0` cases where the new side resolved no internal callees at all.
 *
 * The function matching cascade uses `calleeHashes` as a disambiguation tier
 * (`calleeHashesResolved`), so the same asymmetry would matter there — but NOT
 * necessarily in the same way, which is why this is measured separately rather
 * than inferred. Disambiguation picks AMONG candidates inside one bucket, and
 * an asymmetry that hits every candidate equally does not mislead a comparison
 * between them. An accept/reject gate has no such protection. The binding
 * result does not transfer on its own.
 *
 * ## The control, and why it is the whole design
 *
 * Pairs matched BY the callee tier would trivially have equal callee sets —
 * that is how they matched. Measuring those would prove nothing.
 *
 * So this measures only buckets that are a SINGLETON ON BOTH SIDES. Those are
 * resolved by `structuralHashUnique` with no callee evidence consulted at all,
 * so callee agreement is an INDEPENDENT observation about pairs already known
 * to correspond. That is the unbiased population.
 *
 * Runs no LLM and writes nothing.
 *
 * ## MEASURED 2026-08-03 — the function tier is FINE, and the binding result
 * does NOT transfer
 *
 *   2.1.85->2.1.86, pairs matched on hash alone (singleton both sides): 9,023
 *     both sides have zero callees        6,440 (71.37%)  no evidence either way
 *     callee-set sizes EQUAL              9,021 (99.98%)
 *     prior has more                          0 (0.00%)
 *     new has more                            2 (0.02%)
 *     new is ZERO while prior is not          0 (0.00%)
 *
 *   Among the 2,583 pairs where either side has callees, 2 disagree (0.08%).
 *
 *   2.1.197->2.1.198 reproduces it: 14,572 pairs, 99.99% equal, 1 disagreement
 *   out of the 4,863 with callees (0.02%), and again ZERO `new is zero` cases.
 *
 * Compare the BINDING result on the same pair: ~8% of singleton pairs
 * disagreed, and reading twenty of them found twenty same-binding pairs,
 * several with the `N vs 0` shape. Functions show that shape ZERO times.
 *
 * So `calleeHashes` IS a comparable signal across versions for functions, and
 * the cascade's disambiguation tier is not resting on incomparable sets. No
 * change warranted.
 *
 * A coherent explanation for the difference, offered as such and not as a
 * measured claim: a binding's "callees" come from its INITIALIZER, which is
 * exactly what humanification restructures (lazy-initializer wrappers, module
 * bindings resolving to graph nodes). A function's callees come from call
 * expressions in its body, which survive minification structurally intact.
 */
import * as fs from "node:fs";
import type * as t from "@babel/types";
import { parseSourceAst } from "../../src/babel-utils.js";
import { buildFingerprintIndex } from "../../src/analysis/fingerprint-index.js";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import type {
  FingerprintIndex,
  FunctionNode
} from "../../src/analysis/types.js";

interface Side {
  index: FingerprintIndex;
  nodes: Map<string, FunctionNode>;
  code: string;
}

function loadSide(code: string, label: string): Side {
  const ast = parseSourceAst(code);
  if (!ast) throw new Error(`could not parse ${label}`);
  const fns = buildFunctionGraph(ast as t.File, label);
  const nodes = new Map(fns.map((f) => [f.sessionId, f]));
  return { index: buildFingerprintIndex(nodes), nodes, code };
}

function pct(n: number, d: number): string {
  return d === 0 ? "n/a" : `${((100 * n) / d).toFixed(2)}%`;
}

interface Pair {
  oldId: string;
  newId: string;
  oldN: number;
  newN: number;
}

/** Buckets that are a singleton on BOTH sides — matched on hash alone. */
function unbiasedPairs(prior: Side, fresh: Side): Pair[] {
  const out: Pair[] = [];
  for (const [hash, oldIds] of prior.index.byStructuralHash) {
    if (oldIds.length !== 1) continue;
    const newIds = fresh.index.byStructuralHash.get(hash);
    if (newIds?.length !== 1) continue;
    const oldFp = prior.index.fingerprints.get(oldIds[0]);
    const newFp = fresh.index.fingerprints.get(newIds[0]);
    if (!oldFp || !newFp) continue;
    out.push({
      oldId: oldIds[0],
      newId: newIds[0],
      oldN: (oldFp.calleeHashes ?? []).length,
      newN: (newFp.calleeHashes ?? []).length
    });
  }
  return out;
}

interface Counts {
  sameCount: number;
  priorMore: number;
  newMore: number;
  /** The shape that made the binding evidence useless: one side resolved
   *  callees and the other resolved none. */
  newZeroPriorNot: number;
  bothZero: number;
}

function tally(pairs: readonly Pair[]): Counts {
  const c: Counts = {
    sameCount: 0,
    priorMore: 0,
    newMore: 0,
    newZeroPriorNot: 0,
    bothZero: 0
  };
  for (const p of pairs) {
    if (p.oldN === p.newN) c.sameCount++;
    else if (p.oldN > p.newN) c.priorMore++;
    else c.newMore++;
    if (p.newN === 0 && p.oldN > 0) c.newZeroPriorNot++;
    if (p.oldN === 0 && p.newN === 0) c.bothZero++;
  }
  return c;
}

function main(): void {
  const [priorPath, newPath] = process.argv.slice(2);
  if (!priorPath || !newPath) {
    console.error(
      "usage: size-callee-set-symmetry.ts <prior.js> <new.js> [--dump N]"
    );
    process.exit(2);
  }
  const dumpIdx = process.argv.indexOf("--dump");
  const dumpN = dumpIdx >= 0 ? Number(process.argv[dumpIdx + 1] ?? 10) : 0;

  const prior = loadSide(fs.readFileSync(priorPath, "utf8"), "prior");
  const fresh = loadSide(fs.readFileSync(newPath, "utf8"), "new");
  const pairs = unbiasedPairs(prior, fresh);

  const { sameCount, priorMore, newMore, newZeroPriorNot, bothZero } =
    tally(pairs);

  console.log(
    `function pairs matched on HASH ALONE (singleton both sides): ${pairs.length}`
  );
  console.log(
    `  both have zero callees:            ${bothZero} (${pct(bothZero, pairs.length)}) — no evidence either way`
  );
  console.log("");
  console.log(
    "CALLEE-SET SIZE AGREEMENT on pairs already known to correspond:"
  );
  console.log(
    `  equal:            ${sameCount} (${pct(sameCount, pairs.length)})`
  );
  console.log(
    `  prior has more:   ${priorMore} (${pct(priorMore, pairs.length)})`
  );
  console.log(`  new has more:     ${newMore} (${pct(newMore, pairs.length)})`);
  console.log(
    `  new is ZERO, prior is not: ${newZeroPriorNot} (${pct(newZeroPriorNot, pairs.length)})  <-- the pathological shape seen on bindings`
  );

  const withEvidence = pairs.length - bothZero;
  const disagreeing = priorMore + newMore;
  console.log("");
  console.log(
    `Among the ${withEvidence} pair(s) where either side HAS callees, ` +
      `${disagreeing} disagree (${pct(disagreeing, withEvidence)}).`
  );

  if (dumpN > 0) {
    console.log(`\n=== up to ${dumpN} disagreeing pair(s) ===`);
    let shown = 0;
    for (const p of pairs) {
      if (shown >= dumpN || p.oldN === p.newN) continue;
      shown++;
      console.log(
        `  ${p.oldId} -> ${p.newId}   callees ${p.oldN} vs ${p.newN}`
      );
    }
  }
}

main();

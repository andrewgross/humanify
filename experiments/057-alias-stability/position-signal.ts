/**
 * Is RELATIVE POSITION usable to reject a bogus statement-hash match?
 *
 *   npx tsx experiments/057-alias-stability/position-signal.ts <freshBundle> <priorLedger> <label>
 *
 * The masked-hash collision that moved 32 declarations into the wrong file
 * paired fresh statement 33461 with prior statement 21051 — about a third of the
 * bundle apart. The obvious guard is "reject a match that moved implausibly
 * far". Whether that works depends entirely on how far HONEST matches move, and
 * the project has a standing finding that declaration position does not
 * correspond across versions (positional tie-break assignment cost +50,606
 * noise lines). So measure before proposing.
 *
 * Method: take every statement the hash tier would accept — hash occurring
 * exactly once on each side, the same `unambiguous` gate `alignFileStatements`
 * uses — and compute how far it moved, in RANK terms normalised for the two
 * releases having different statement counts:
 *
 *     drift = i_fresh - j_prior * (N_fresh / N_prior)
 *
 * Reported as a distribution, plus what a threshold would cost: how many honest
 * matches a given cutoff would reject. Run it on the CALM hop and on 85->86,
 * where upstream reordered ~35% of the bundle — a guard tuned on the calm hop
 * and untested on the shuffle hop is exactly how this axis failed before.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { statementHash } from "../../src/split/statement-hash.js";

const [BUNDLE, LEDGER, LABEL = ""] = process.argv.slice(2);
if (!BUNDLE || !LEDGER) {
  console.error(
    "usage: position-signal.ts <freshBundle> <priorLedger> [label]"
  );
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
if (!ast) throw new Error(`cannot parse ${BUNDLE}`);
const freshStmts = topLevelStatements(buildUnifiedGraph(ast, BUNDLE));
const freshHashes = freshStmts.map((p) => statementHash(p.node as t.Statement));

const ledger = JSON.parse(fs.readFileSync(LEDGER, "utf8"));
const priorHashes: string[] = ledger.hashes;
const priorOrder: string[] = ledger.order;

const freshCount = new Map<string, number>();
for (const h of freshHashes) freshCount.set(h, (freshCount.get(h) ?? 0) + 1);
const priorIdx = new Map<string, number[]>();
priorHashes.forEach((h, i) => {
  const l = priorIdx.get(h);
  if (l) l.push(i);
  else priorIdx.set(h, [i]);
});

const scale = freshHashes.length / priorHashes.length;
type Pair = { i: number; j: number; drift: number; names: number };

/** Declarator count — the proxy for "how much content survives name masking". */
function declaratorCount(node: t.Node): number {
  return node.type === "VariableDeclaration"
    ? (node as t.VariableDeclaration).declarations.length
    : 0;
}

const pairs: Pair[] = [];
freshHashes.forEach((h, i) => {
  const js = priorIdx.get(h);
  if (!js || js.length !== 1 || freshCount.get(h) !== 1) return; // the tier's own gate
  const j = js[0];
  pairs.push({
    i,
    j,
    drift: Math.abs(i - j * scale),
    names: declaratorCount(freshStmts[i].node)
  });
});

const drifts = pairs.map((p) => p.drift).sort((a, b) => a - b);
const q = (f: number) =>
  Math.round(drifts[Math.floor(f * (drifts.length - 1))]);
const N = freshHashes.length;

console.log(`\n=== POSITION SIGNAL — ${LABEL} ===`);
console.log(`  statements: fresh ${N}, prior ${priorHashes.length}`);
console.log(
  `  unambiguous hash matches (what the tier accepts): ${pairs.length}`
);
console.log(`\n  |rank drift| percentiles:`);
for (const f of [0.5, 0.75, 0.9, 0.95, 0.99, 0.999, 1]) {
  console.log(
    `    p${String(f * 100).padEnd(5)} ${String(q(f)).padStart(7)}  (${((100 * q(f)) / N).toFixed(1)}% of the bundle)`
  );
}

console.log(
  `\n  what a rejection threshold would COST (honest matches refused):`
);
for (const pct of [1, 2, 5, 10, 25]) {
  const thr = (pct / 100) * N;
  const lost = pairs.filter((p) => p.drift > thr).length;
  console.log(
    `    reject drift > ${String(pct).padStart(2)}% of bundle (${Math.round(thr)}): ${String(lost).padStart(6)} matches lost  (${((100 * lost) / pairs.length).toFixed(2)}%)`
  );
}

const big = pairs.filter((p) => p.names >= 8).sort((a, b) => b.drift - a.drift);
console.log(
  `\n  matches on bare multi-declarator statements (>= 8 declarators): ${big.length}`
);
for (const p of big.slice(0, 8)) {
  console.log(
    `    fresh ${String(p.i).padStart(6)} <- prior ${String(p.j).padStart(6)}  drift ${String(Math.round(p.drift)).padStart(6)} (${((100 * p.drift) / N).toFixed(1)}%)  ${p.names} declarators  -> ${priorOrder[p.j]}`
  );
}

/**
 * Task 1 — read every disagreement, rule 1. Not a sample: the whole population
 * across eight hops is 13 statements, so all of them are printed in full.
 *
 *   npx tsx experiments/058-binding-placement/read-disagreements.ts \
 *      <disagreeDump> <freshBundle> <priorBundle> <priorLedger>
 *
 * For each statement the hash tier placed against a dissenting tier, this puts
 * the two competing claims next to each other so the verdict is READ:
 *
 *  - the FRESH statement's text;
 *  - the PRIOR statement(s) carrying the same `statementHash`, with the file the
 *    hash tier inherited from them — if that text is manifestly other code, the
 *    hash matched a collision and the dissenter is right;
 *  - the dissenting tier's file, and `nameToFiles` for the declared names, which
 *    is where a name vote's claim comes from.
 *
 * Generalises `057/hash-collision-probe.ts` from one name to a population.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { statementHash } from "../../src/split/statement-hash.js";

const [DUMP, FRESH, PRIOR, LEDGER] = process.argv.slice(2);
if (!DUMP || !FRESH || !PRIOR || !LEDGER) {
  console.error(
    "usage: read-disagreements.ts <disagreeDump> <freshBundle> <priorBundle> <priorLedger>"
  );
  process.exit(1);
}

const WIDTH = Number(process.env.READ_WIDTH ?? 400);

function statements(path: string): { node: t.Node; text: string }[] {
  const code = fs.readFileSync(path, "utf8");
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`cannot parse ${path}`);
  return topLevelStatements(buildUnifiedGraph(ast, path)).map((p) => ({
    node: p.node,
    text:
      p.node.start != null && p.node.end != null
        ? code.slice(p.node.start, p.node.end)
        : ""
  }));
}

const dump = JSON.parse(fs.readFileSync(DUMP, "utf8"));
const ledger = JSON.parse(fs.readFileSync(LEDGER, "utf8"));
const fresh = statements(FRESH);
const prior = statements(PRIOR);

// `statementHash` over the whole prior side once — the ledger's `hashes` array
// is the same thing, but recomputing it proves the two agree rather than
// assuming the ledger on disk describes this bundle.
const priorHashes = prior.map((s) => statementHash(s.node as t.Statement));
const ledgerMatches = priorHashes.filter(
  (h, i) => h === (ledger.hashes as string[])[i]
).length;
console.log(
  `ledger/bundle hash agreement: ${ledgerMatches}/${priorHashes.length}` +
    (ledgerMatches === priorHashes.length ? "  OK" : "  *** MISMATCH ***")
);

const byHash = new Map<string, number[]>();
priorHashes.forEach((h, i) => {
  const l = byHash.get(h) ?? [];
  l.push(i);
  byHash.set(h, l);
});
const freshCounts = new Map<string, number>();
for (const s of fresh) {
  const h = statementHash(s.node as t.Statement);
  freshCounts.set(h, (freshCounts.get(h) ?? 0) + 1);
}

for (const d of dump.dissent) {
  const f = fresh[d.index];
  const h = statementHash(f.node as t.Statement);
  const priorIdx = byHash.get(h) ?? [];
  console.log(`\n${"=".repeat(78)}`);
  console.log(
    `[${d.index}] ${d.emptyDecl ? "ZERO-INIT DECLARATION" : "other"}  lines=${d.lines}  names=${d.nameCount ?? d.names.length}`
  );
  console.log(`  hash            : ${h}`);
  console.log(
    `  occurrences     : fresh ${freshCounts.get(h)}  prior ${priorIdx.length}`
  );
  console.log(`  HASH tier placed: ${d.file}`);
  console.log(`  DISSENT         : ${JSON.stringify(d.alternatives)}`);
  console.log(`\n  --- FRESH text ---\n${f.text.slice(0, WIDTH)}`);
  for (const i of priorIdx.slice(0, 3)) {
    console.log(
      `\n  --- PRIOR [${i}] (ledger.order: ${ledger.order[i]}) ---\n${prior[i].text.slice(0, WIDTH)}`
    );
  }
  console.log("\n  --- nameToFiles for the declared names ---");
  for (const n of d.names.slice(0, 8)) {
    console.log(
      `    ${n.padEnd(34)} ${JSON.stringify(ledger.nameToFiles[n] ?? null)}`
    );
  }
}

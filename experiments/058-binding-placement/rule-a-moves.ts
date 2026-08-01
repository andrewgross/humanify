/**
 * Rule 6 and rule 11, for candidate (A): what does the rule actually DO?
 *
 *   npx tsx experiments/058-binding-placement/rule-a-moves.ts \
 *      <freshBundle> <priorLedger> <priorBundle>
 *
 * Rule 6 — "refusing the fingerprint on one statement class does not remove a
 * mis-placement, it moves it" — cannot be answered by a churn total. It needs
 * the per-statement trail: which statements the rule takes off the hash tier,
 * which tier catches each one, and whether the file it lands in is the one the
 * prior release had it in.
 *
 * Rule 11 — a pass that logs what it did turns "did the metric move?" into "did
 * the code do anything HERE?". Every statement (A) re-places is printed.
 *
 * The refusal is applied to a COPY of the prior ledger's `hashes`, exactly as in
 * `ceiling-ab.ts`, so the splitter runs unmodified.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { emptyPriorCarry } from "../../src/split/prior-carry.js";
import { placementTrail } from "../../src/split/placement-trail.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { statementHash } from "../../src/split/statement-hash.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

const [BUNDLE, LEDGER, PRIOR_BUNDLE] = process.argv.slice(2);
if (!BUNDLE || !LEDGER || !PRIOR_BUNDLE) {
  console.error(
    "usage: rule-a-moves.ts <freshBundle> <priorLedger> <priorBundle>"
  );
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const prior = JSON.parse(fs.readFileSync(LEDGER, "utf8"));

const ast = parseFileAst(code);
if (!ast) throw new Error("parse");
const wrapper = findWrapperFunction(ast);
if (!wrapper) throw new Error("no wrapper");
const bodyNode = wrapper.functionPath.node.body;
if (!t.isBlockStatement(bodyNode)) throw new Error("not a block");
const body = bodyNode.body;
const hashes = body.map((s) => statementHash(s));

const isEmptyDeclaration = (stmt: t.Statement) =>
  t.isVariableDeclaration(stmt) &&
  stmt.declarations.length > 0 &&
  stmt.declarations.every((d) => d.init === null || d.init === undefined);

const refused = new Set<string>();
body.forEach((s, i) => {
  if (isEmptyDeclaration(s)) refused.add(hashes[i]);
});

function priorTexts(p: string): readonly string[] {
  const c = fs.readFileSync(p, "utf8");
  const a = parseSync(c, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!a) throw new Error("parse prior");
  return topLevelStatements(buildUnifiedGraph(a, p)).map((x) =>
    x.node.start != null && x.node.end != null
      ? c.slice(x.node.start, x.node.end)
      : ""
  );
}
/** The real binding-identity map, off disk — without it the `preempt` and
 * `fill` tiers are inert and the reconstruction is not the shipped splitter. */
function priorMatchMap(): ReadonlyMap<string, string> {
  const p = `${BUNDLE.slice(0, BUNDLE.lastIndexOf("/"))}/prior-match-map.json`;
  if (!fs.existsSync(p)) return new Map();
  return new Map(
    Object.entries(
      JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>
    )
  );
}

const carry = {
  ...emptyPriorCarry(),
  matchMap: priorMatchMap(),
  statementTexts: priorTexts(PRIOR_BUNDLE)
};

async function run(ledger: unknown) {
  placementTrail.reset(true);
  const r = await stableSplitFromCode(code, {
    prior: ledger as typeof prior,
    priorCarry: carry
  });
  if (!r) throw new Error("split produced nothing");
  return { report: placementTrail.report(), ledger: r.ledger };
}

const off = await run(prior);
const on = await run({
  ...prior,
  hashes: (prior.hashes as string[]).map((h, i) =>
    refused.has(h) ? `refused-${i}` : h
  )
});

const offT = new Map(off.report.trails.map((x) => [x.index, x]));
const onT = new Map(on.report.trails.map((x) => [x.index, x]));

// `nameToFiles`, the same map the name vote reads — used here only to say where
// the prior release kept each declared name, so a landing can be judged.
const nameToFiles: Record<string, string[]> = prior.nameToFiles;

let refusedStatements = 0;
let unchanged = 0;
const moves: string[] = [];
for (const [i, a] of offT) {
  if (!isEmptyDeclaration(body[i])) continue;
  if (a.placedBy !== "hash") continue;
  refusedStatements++;
  const b = onT.get(i);
  if (!b) continue;
  if (a.file === b.file) {
    unchanged++;
    continue;
  }
  const names = a.names;
  const homes = new Set(
    names.flatMap((n) => nameToFiles[n] ?? []).map((f) => f)
  );
  const unanimousHome = homes.size === 1 ? [...homes][0] : undefined;
  moves.push(
    `  [${i}] decls=${(body[i] as t.VariableDeclaration).declarations.length}\n` +
      `      hash tier said : ${a.file}\n` +
      `      rule (A) lands : ${b.file}   (via ${b.placedBy})\n` +
      `      prior homes of its names: ${homes.size === 1 ? unanimousHome : `${homes.size} distinct — ${[...homes].slice(0, 3).join(", ")}`}\n` +
      `      VERDICT        : ${
        unanimousHome === undefined
          ? "names disagree — no ground truth from the name map"
          : b.file === unanimousHome
            ? "lands where every declared name lived — the mis-placement is REMOVED"
            : "lands somewhere ELSE — the mis-placement MOVED (rule 6)"
      }`
  );
}

console.log(`=== rule (A): statements taken off the hash tier ===`);
console.log(`  hash-placed zero-init declarations : ${refusedStatements}`);
console.log(`  landed in the SAME file anyway     : ${unchanged}`);
console.log(`  re-placed                          : ${moves.length}`);
console.log(`\n  tier counts  OFF: ${JSON.stringify(off.report.tiers)}`);
console.log(`  tier counts  ON : ${JSON.stringify(on.report.tiers)}`);
console.log("");
for (const m of moves) console.log(`${m}\n`);

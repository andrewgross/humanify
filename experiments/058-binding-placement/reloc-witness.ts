/**
 * What `relocatedStatements` is pointing at — read, not inferred.
 *
 *   npx tsx experiments/058-binding-placement/reloc-witness.ts <freshDir> <priorDir>
 *
 * The gate's `reloc(st)` KPI is documented as the ORDER-INDEPENDENT counterpart
 * to the name-keyed `reloc` that measurement-pitfalls rule 7 is about. It is —
 * but it is still keyed on a bare `statementHash`, with the SAME inference the
 * placement hash tier makes:
 *
 *     a hash occurring exactly once on each side is the same statement
 *
 * exp058's whole finding is that this inference is false for one statement
 * class, because `statementHash` masks identifiers and a declaration with no
 * initializers masks to a declarator count. So on that class the KPI cannot
 * disagree with the tier — it re-computes the tier's premise. A change that
 * declines the premise necessarily registers as a relocation.
 *
 * "Necessarily" is a claim, so this prints the witness: every statement the KPI
 * counts, its two candidate files, its declared names, and whether the prior
 * statement it was paired with shares ANY of them. Zero shared names means the
 * pairing is a collision and the "relocation" is the fix.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

const [FRESH_DIR, PRIOR_DIR] = process.argv.slice(2);
if (!FRESH_DIR || !PRIOR_DIR) {
  console.error("usage: reloc-witness.ts <freshDir> <priorDir>");
  process.exit(1);
}

interface Ledger {
  order: string[];
  hashes: string[];
}

function ledger(dir: string): Ledger {
  return JSON.parse(
    fs.readFileSync(path.join(dir, ".humanify/split-ledger.json"), "utf8")
  ) as Ledger;
}

/** Top-level wrapper statements, the array both ledgers index into. */
function statements(dir: string): t.Statement[] {
  const code = fs.readFileSync(
    path.join(dir, ".humanify/humanified.js"),
    "utf8"
  );
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`could not parse ${dir}`);
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error(`no wrapper in ${dir}`);
  const body = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(body)) throw new Error("wrapper body is not a block");
  return body.body;
}

const fresh = ledger(FRESH_DIR);
const prior = ledger(PRIOR_DIR);
const freshStmts = statements(FRESH_DIR);
const priorStmts = statements(PRIOR_DIR);

const count = (hs: string[]) => {
  const m = new Map<string, number>();
  for (const h of hs) m.set(h, (m.get(h) ?? 0) + 1);
  return m;
};
const fc = count(fresh.hashes);
const pc = count(prior.hashes);
const priorIndexByHash = new Map<string, number>();
prior.hashes.forEach((h, i) => {
  if (pc.get(h) === 1) priorIndexByHash.set(h, i);
});

const names = (s: t.Statement | undefined) =>
  s ? Object.keys(t.getBindingIdentifiers(s, false)) : [];

let compared = 0;
let relocated = 0;
fresh.hashes.forEach((h, i) => {
  if (fc.get(h) !== 1) return;
  const pi = priorIndexByHash.get(h);
  if (pi === undefined) return;
  compared++;
  if (prior.order[pi] === fresh.order[i]) return;
  relocated++;
  const fn = names(freshStmts[i]);
  const pn = new Set(names(priorStmts[pi]));
  const shared = fn.filter((n) => pn.has(n));
  const stmt = freshStmts[i];
  const emptyDecl =
    t.isVariableDeclaration(stmt) &&
    stmt.declarations.length > 0 &&
    stmt.declarations.every((d) => d.init == null);
  console.log(`\n--- relocation ${relocated} ---`);
  console.log(`  hash            : ${h}   (1 occurrence on each side)`);
  console.log(`  prior file      : ${prior.order[pi]}`);
  console.log(`  fresh file      : ${fresh.order[i]}`);
  console.log(
    `  statement type  : ${stmt?.type}${emptyDecl ? "  ZERO-INITIALIZER DECLARATION" : ""}`
  );
  console.log(`  declared names  : ${fn.length} fresh / ${pn.size} prior`);
  console.log(
    `  names in COMMON : ${shared.length}${shared.length ? ` (${shared.slice(0, 6).join(", ")})` : "  <-- disjoint: the hash paired unrelated statements"}`
  );
  console.log(`  fresh names     : ${fn.slice(0, 8).join(", ")}`);
  console.log(`  prior names     : ${[...pn].slice(0, 8).join(", ")}`);
});

console.log(
  `\nstatementsCompared ${compared}, relocatedStatements ${relocated}`
);

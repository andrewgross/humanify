/**
 * Task 0a/1 — the DISAGREEMENT population: every statement the hash tier placed
 * where some other placement tier, had it been asked, would have said a
 * different file.
 *
 *   npx tsx experiments/058-binding-placement/disagree.ts <trailDump> <freshBundle>
 *
 * That population is exactly what exp058's two candidates fight over:
 *
 *  - **(B)** — promote the binding correspondence above the fingerprint — can
 *    only ever change a statement in it, because everywhere else the tiers
 *    agree and re-ranking them is a no-op.
 *  - **(A)** — refuse the hash tier on a declaration with no initializers —
 *    changes a SUBSET of it, plus (this is the part a target-scoped ceiling
 *    misses, rule 5) every zero-initializer statement the hash tier is placing
 *    CORRECTLY today, which the rule would also push down the cascade.
 *
 * So both counts are printed: the disagreements, and the blast radius.
 *
 * Statement indices in the trail are into the split's wrapper body, so the
 * bundle is re-parsed the same way `stableSplitFromCode` does — no re-derived
 * proxy for the index.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import type { PlacementTrailReport } from "../../src/split/placement-trail.js";

const [TRAIL, BUNDLE] = process.argv.slice(2);
if (!TRAIL || !BUNDLE) {
  console.error("usage: disagree.ts <trailDump> <freshBundle>");
  process.exit(1);
}

const report: PlacementTrailReport = JSON.parse(fs.readFileSync(TRAIL, "utf8"));
const code = fs.readFileSync(BUNDLE, "utf8");
const ast = parseFileAst(code);
if (!ast) throw new Error(`could not parse ${BUNDLE}`);
const wrapper = findWrapperFunction(ast);
if (!wrapper) throw new Error(`no wrapper in ${BUNDLE}`);
const bodyNode = wrapper.functionPath.node.body;
if (!t.isBlockStatement(bodyNode))
  throw new Error("wrapper body is not a block");
const body = bodyNode.body;

if (body.length !== report.trails.length) {
  // A mismatch means the dump and the bundle describe different runs; every
  // index below would be off by an unknown amount. Loud, not silent.
  throw new Error(
    `trail describes ${report.trails.length} statements, bundle has ${body.length}`
  );
}

/**
 * (A)'s predicate, stated so it can be checked against the claim it is making:
 * a variable declaration in which NO declarator has an initializer. Its masked
 * form is `var $0, $1, …, $n;` — a declarator count and nothing else, which is
 * why two unrelated ones collide (057's 32-declarator pair).
 */
function isEmptyDeclaration(stmt: t.Statement): boolean {
  return (
    t.isVariableDeclaration(stmt) &&
    stmt.declarations.length > 0 &&
    stmt.declarations.every((d) => d.init === null || d.init === undefined)
  );
}

/** Tiers whose dissent (B) would act on — the identity/name evidence. Ordinal
 * is positional and is counted apart, since promoting it is not on the table. */
const STRONG = new Set([
  "preempt",
  "fill",
  "name",
  "allsame",
  "anchor",
  "anchorPreempt"
]);

const text = (i: number) => {
  const n = body[i];
  return n.start != null && n.end != null ? code.slice(n.start, n.end) : "";
};

const hashPlaced = report.trails.filter((x) => x.placedBy === "hash");
const emptyDecl = report.trails.filter((x) =>
  isEmptyDeclaration(body[x.index])
);
const emptyHashPlaced = emptyDecl.filter((x) => x.placedBy === "hash");
const dissent = hashPlaced.filter((x) => x.alternatives);
const strongDissent = dissent.filter((x) =>
  Object.keys(x.alternatives ?? {}).some((k) => STRONG.has(k))
);

const lines = (i: number) => text(i).split("\n").length;
const sum = (rows: typeof hashPlaced) =>
  rows.reduce((a, x) => a + lines(x.index), 0);

console.log(`=== ${TRAIL} ===`);
console.log(`  statements                       : ${report.trails.length}`);
console.log(`  hash-placed                      : ${hashPlaced.length}`);
console.log("");
console.log("  (B) population — hash-placed, some tier dissents:");
console.log(
  `      any dissent                  : ${dissent.length}  (${sum(dissent)} stmt lines)`
);
console.log(
  `      dissent from a STRONG tier   : ${strongDissent.length}  (${sum(strongDissent)} stmt lines)`
);
console.log("");
console.log("  (A) blast radius — zero-initializer declarations:");
console.log(
  `      all                          : ${emptyDecl.length}  (${sum(emptyDecl)} stmt lines)`
);
console.log(
  `      placed by hash (rule refuses): ${emptyHashPlaced.length}  (${sum(emptyHashPlaced)} stmt lines)`
);
console.log(
  `      of those, a tier dissents    : ${emptyHashPlaced.filter((x) => x.alternatives).length}`
);
console.log(
  `      of those, NO tier dissents   : ${emptyHashPlaced.filter((x) => !x.alternatives).length}  <- the rule moves these off a CORRECT placement's evidence`
);
console.log("");
console.log("  declarator counts of hash-placed empty declarations:");
{
  const hist = new Map<number, number>();
  for (const x of emptyHashPlaced) {
    const d = (body[x.index] as t.VariableDeclaration).declarations.length;
    const k = d >= 8 ? 8 : d;
    hist.set(k, (hist.get(k) ?? 0) + 1);
  }
  for (const [k, n] of [...hist].sort((a, b) => a[0] - b[0])) {
    console.log(
      `      ${k === 8 ? "8+" : String(k).padStart(2)} declarators : ${n}`
    );
  }
}

console.log("\n  === every disagreement, in full (rule 1: read them) ===");
for (const x of dissent) {
  const stmt = body[x.index];
  const kind = t.isVariableDeclaration(stmt)
    ? `${stmt.kind}[${stmt.declarations.length}] init=${stmt.declarations.filter((d) => d.init).length}`
    : stmt.type;
  console.log(
    `\n  [${x.index}] ${kind}  lines=${lines(x.index)}  names=${x.nameCount ?? x.names.length}` +
      `\n      placed  : ${x.file}` +
      `\n      dissent : ${JSON.stringify(x.alternatives)}` +
      `\n      names   : ${x.names.slice(0, 10).join(", ")}` +
      `\n      text    : ${text(x.index).slice(0, 220).replace(/\n/g, " ⏎ ")}`
  );
}

const DUMP = process.env.DISAGREE_DUMP;
if (DUMP) {
  fs.writeFileSync(
    DUMP,
    JSON.stringify(
      {
        trail: TRAIL,
        bundle: BUNDLE,
        statements: report.trails.length,
        hashPlaced: hashPlaced.length,
        dissent: dissent.map((x) => ({
          index: x.index,
          file: x.file,
          alternatives: x.alternatives,
          names: x.names,
          nameCount: x.nameCount,
          emptyDecl: isEmptyDeclaration(body[x.index]),
          lines: lines(x.index),
          text: text(x.index).slice(0, 4000)
        })),
        emptyHashPlaced: emptyHashPlaced.map((x) => ({
          index: x.index,
          file: x.file,
          declarators: (body[x.index] as t.VariableDeclaration).declarations
            .length,
          dissent: x.alternatives ?? null
        }))
      },
      null,
      1
    )
  );
  console.log(`\n  wrote ${DUMP}`);
}

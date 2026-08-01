/**
 * Task 0b — the GIT-CAPPED ceiling for (A) and (B), separately.
 *
 *   npx tsx experiments/058-binding-placement/ceiling-ab.ts \
 *      <freshBundle> <priorLedger> <priorBundle> <priorSrcDir> <freshSrcDir> <mode> [outDir]
 *
 *   mode = off | A | B
 *
 * A decomposition attributes lines; it does not bound them (051 over-charged its
 * own population by 29%). So nothing is attributed here: the counterfactual TREE
 * is built with the real splitter and the real runnable emitter, and the same
 * `diff` a reviewer runs is re-run against the prior release. Both sides are
 * real texts.
 *
 * ## How the counterfactual is applied without touching pipeline code
 *
 * Both candidates are "the hash tier does not get to claim this statement". The
 * hash tier's ONLY input is `prior.hashes` — it groups the prior ledger's
 * hashes by file and looks each fresh statement's hash up in that map
 * (`hashTier`, stable-split.ts). Replacing a hash with an unmatchable token in
 * a COPY of the ledger therefore makes the tier report `absent` for exactly the
 * statements that carry it, and changes nothing else: `alignEmissionOrder` reads
 * `emitHashes`, which is a separate array and is left alone.
 *
 * So the production splitter and emitter run unmodified, and the refusal rule
 * lives in this file. That is what Task 0b asks for — a ceiling BEFORE any
 * pipeline code exists.
 *
 *   (A) refuse the hash tier on every variable declaration with NO initializers
 *       — the masked form is `var $0, …, $n;`, a declarator count and nothing
 *       else, which is what collided in 057.
 *   (B) refuse it wherever an identity/name tier would have said a different
 *       file — i.e. promote that evidence above the fingerprint, on exactly the
 *       population where the promotion changes anything. The dissent set is read
 *       from the placement trail (see `disagree.ts`).
 *
 * The OFF leg is a control that must reproduce the SHIPPED tree; it is diffed
 * against the real fresh `src/` and that number is printed first. A
 * reconstruction that does not reproduce the shipped tree cannot bound anything.
 */
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import * as t from "@babel/types";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { emptyPriorCarry } from "../../src/split/prior-carry.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { statementHash } from "../../src/split/statement-hash.js";
import { emitRunnableCjs } from "../../src/split/cjs-emit.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

const [BUNDLE, LEDGER, PRIOR_BUNDLE, PRIOR_SRC, FRESH_SRC, MODE, OUT] =
  process.argv.slice(2);
if (!BUNDLE || !LEDGER || !PRIOR_BUNDLE || !PRIOR_SRC || !FRESH_SRC || !MODE) {
  console.error(
    "usage: ceiling-ab.ts <freshBundle> <priorLedger> <priorBundle> <priorSrc> <freshSrc> <off|A|B> [outDir]"
  );
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const prior = JSON.parse(fs.readFileSync(LEDGER, "utf8"));

/** (A)'s predicate. Stated once, used once — a declaration none of whose
 * declarators has an initializer. */
function isEmptyDeclaration(stmt: t.Statement): boolean {
  return (
    t.isVariableDeclaration(stmt) &&
    stmt.declarations.length > 0 &&
    stmt.declarations.every((d) => d.init === null || d.init === undefined)
  );
}

// The fresh wrapper body, parsed the way `stableSplitFromCode` parses it, so a
// statement index here is the index the splitter and the trail both use.
const freshAst = parseFileAst(code);
if (!freshAst) throw new Error(`could not parse ${BUNDLE}`);
const freshWrapper = findWrapperFunction(freshAst);
if (!freshWrapper) throw new Error("no wrapper");
const freshBodyNode = freshWrapper.functionPath.node.body;
if (!t.isBlockStatement(freshBodyNode)) throw new Error("wrapper not a block");
const freshBody = freshBodyNode.body;
const freshHashes = freshBody.map((s) => statementHash(s));

/** Hashes the counterfactual denies the hash tier, and why. */
function refusedHashes(): { hashes: Set<string>; statements: number } {
  const out = new Set<string>();
  let n = 0;
  if (MODE === "A") {
    freshBody.forEach((stmt, i) => {
      if (!isEmptyDeclaration(stmt)) return;
      n++;
      out.add(freshHashes[i]);
    });
  } else if (MODE === "B") {
    const dumpPath = process.env.DISSENT_DUMP;
    if (!dumpPath) throw new Error("mode B needs DISSENT_DUMP=<disagree dump>");
    const dump = JSON.parse(fs.readFileSync(dumpPath, "utf8"));
    const STRONG = new Set([
      "preempt",
      "fill",
      "name",
      "allsame",
      "anchor",
      "anchorPreempt"
    ]);
    for (const d of dump.dissent) {
      if (!Object.keys(d.alternatives ?? {}).some((k) => STRONG.has(k)))
        continue;
      n++;
      out.add(freshHashes[d.index]);
    }
  }
  return { hashes: out, statements: n };
}

const { hashes: refused, statements: refusedCount } = refusedHashes();

// A refused hash must not drag an unrelated statement down with it: if some
// OTHER fresh statement carries the same hash, denying it is collateral the
// ceiling would silently include. Counted and printed, never hidden.
let collateral = 0;
if (refused.size > 0) {
  freshBody.forEach((stmt, i) => {
    if (!refused.has(freshHashes[i])) return;
    const intended = MODE === "A" ? isEmptyDeclaration(stmt) : true; // B's set is per-index
    if (!intended) collateral++;
  });
}

const priorForRun =
  refused.size === 0
    ? prior
    : {
        ...prior,
        hashes: (prior.hashes as string[]).map((h, i) =>
          refused.has(h) ? `refused-${i}` : h
        )
      };

/** Prior statement texts — the content-anchor tier's evidence. Without them
 * that tier is off and the counterfactual would measure a different splitter
 * (057's trail-check learned this the same way). */
function priorStatementTexts(p: string): readonly string[] {
  const c = fs.readFileSync(p, "utf8");
  const ast = parseSync(c, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`could not parse ${p}`);
  return topLevelStatements(buildUnifiedGraph(ast, p)).map((x) =>
    x.node.start != null && x.node.end != null
      ? c.slice(x.node.start, x.node.end)
      : ""
  );
}

/**
 * The REAL binding-identity map the shipped run used, off disk.
 *
 * Leaving this empty is not a smaller version of the experiment — it turns the
 * `preempt` and `fill` tiers off, and a reconstruction with those tiers off
 * reports "identity never dissents" for the reason rule 10's corollary names:
 * the thing was not free to vary. The shipped runs place 6 / 0 / 1 / 2
 * statements by `preempt` and 2 / 0 / 0 / 0 by `fill`, and the reconstruction
 * has to reproduce that before it can bound anything.
 */
function priorMatchMap(): ReadonlyMap<string, string> {
  const p = path.join(path.dirname(BUNDLE), "prior-match-map.json");
  if (!fs.existsSync(p)) {
    console.log(`  matchMap: ABSENT at ${p} — identity tiers will be inert`);
    return new Map();
  }
  const raw = JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>;
  return new Map(Object.entries(raw));
}

const carry = {
  ...emptyPriorCarry(),
  matchMap: priorMatchMap(),
  statementTexts: priorStatementTexts(PRIOR_BUNDLE)
};

const result = await stableSplitFromCode(code, {
  prior: priorForRun,
  priorCarry: carry
});
if (!result) throw new Error("split produced nothing");
const files = emitRunnableCjs(code, result.ledger, result.wrapper, priorForRun);

const outDir = OUT ?? fs.mkdtempSync("/tmp/ceil-");
for (const [rel, text] of files) {
  const p = path.join(outDir, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, text);
}

/** Per-file changed-line counts of `diff -rN a b`, keyed by path under `a`. */
function treeDiff(a: string, b: string): Map<string, number> {
  const r = spawnSync("diff", ["-rN", "-u", a, b], {
    encoding: "utf8",
    maxBuffer: 1 << 30
  });
  const per = new Map<string, number>();
  let file = "";
  for (const line of (r.stdout ?? "").split("\n")) {
    if (line.startsWith("--- ")) {
      const p = line.slice(4).split("\t")[0];
      file = p.startsWith(a) ? path.relative(a, p) : p;
      if (!per.has(file)) per.set(file, 0);
      continue;
    }
    if (line.startsWith("+++") || line.startsWith("@@")) continue;
    if (line[0] === "+" || line[0] === "-")
      per.set(file, (per.get(file) ?? 0) + 1);
  }
  return per;
}

const total = (m: Map<string, number>) =>
  [...m.values()].reduce((a, b) => a + b, 0);

const vsPrior = treeDiff(path.join(PRIOR_SRC), path.join(outDir, "src"));
const vsShipped = treeDiff(path.join(FRESH_SRC), path.join(outDir, "src"));

console.log(
  `MODE=${MODE}  refusedStatements=${refusedCount}  refusedHashes=${refused.size}  collateral=${collateral}`
);
console.log(`  emitted files            : ${files.size}`);
console.log(`  churn vs PRIOR src       : ${total(vsPrior)}`);
console.log(
  `  churn vs SHIPPED fresh   : ${total(vsShipped)}   <- control: the reconstruction's fidelity`
);
console.log(
  `  placement tiers          : ${JSON.stringify(result.stats.byTier)}`
);
console.log(
  `RESULT|${MODE}|${refusedCount}|${total(vsPrior)}|${total(vsShipped)}|${outDir}`
);

if (!OUT) fs.rmSync(outDir, { recursive: true, force: true });

if (process.env.PERFILE_DUMP) {
  fs.writeFileSync(
    process.env.PERFILE_DUMP,
    JSON.stringify([...vsPrior].sort((a, b) => b[1] - a[1]))
  );
}

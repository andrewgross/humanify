/**
 * 054 follow-up — CAN the post-split renames be carried back into the bundle?
 *
 *   npx tsx carry-ceiling.ts <treeRoot> <renameTrail.log> [label]
 *
 * The tree and the bundle must agree, or the lineage degrades a little every
 * hop: `.humanify/humanified.js` is what the NEXT release points
 * `--prior-version` at, and it does not carry this pass's renames.
 *
 * The obvious identification — hash the emitted statement, find the bundle
 * statement with that hash — does NOT work: the runnable emit rewrites
 * cross-file references (`lazyInitializer(x)` becomes
 * `resourceLifecycle.lazyInitializer(x)`), so an emitted statement is not
 * structurally the bundle statement it came from.
 *
 * What is left is the ledger's file assignment, which IS bundle-indexed
 * (`ledger.order[i]` is the file of bundle statement `i`). So a renamed binding
 * can be looked for among the bindings declared inside that file's bundle
 * statements. This measures whether that is enough:
 *
 *   UNIQUE     exactly one binding of that name in the file's bundle
 *              statements — carryable with no further evidence
 *   AMBIGUOUS  several; carrying would need a tie-break, or must abstain
 *   MISSING    none; the name never reached the bundle under that spelling
 *
 * Reported separately for top-level bindings (which live in the wrapper scope
 * and should be unique by construction) and inner locals (which need not be).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [TREE, TRAIL, LABEL = ""] = process.argv.slice(2);
if (!TREE || !TRAIL) {
  console.error("usage: carry-ceiling.ts <treeRoot> <renameTrail.log> [label]");
  process.exit(1);
}

interface Rename {
  file: string;
  from: string;
  to: string;
  kind: string;
}
// The debug channel prints its header and message on separate lines, so match
// the message shape rather than the channel tag.
const TRAIL_LINE = /^\s+(\S+\.js):\s+(\S+)\s+->\s+(\S+)\s+\[(\w+),/;
const renames: Rename[] = [];
for (const line of fs.readFileSync(TRAIL, "utf8").split("\n")) {
  const m = TRAIL_LINE.exec(line);
  if (m) renames.push({ file: m[1], from: m[2], to: m[3], kind: m[4] });
}
if (renames.length === 0) {
  console.error(`no post-split-reconcile lines in ${TRAIL}`);
  process.exit(1);
}

const ledger = JSON.parse(
  fs.readFileSync(path.join(TREE, ".humanify", "split-ledger.json"), "utf8")
) as StableSplitLedger;
const bundle = fs.readFileSync(
  path.join(TREE, ".humanify", "humanified.js"),
  "utf8"
);

const ast = parseSourceAst(bundle, { filename: "bundle.js" });
if (!ast) {
  console.error("bundle did not parse");
  process.exit(1);
}

/** The wrapper body — the statement list `ledger.order` indexes. */
function wrapperBody(file: t.File): t.Statement[] {
  // The bundle is a single wrapper call; the split indexes its BODY.
  let found: t.Statement[] | null = null;
  traverse(file, {
    Function(p: NodePath<t.Function>) {
      if (found) return;
      const body = p.node.body;
      if (body.type !== "BlockStatement") return;
      if (body.body.length < ledger.order.length) return;
      found = body.body;
      p.stop();
    }
  });
  return found ?? file.program.body;
}

const body = wrapperBody(ast);
console.log(
  `  wrapper body: ${body.length} statements, ledger.order: ${ledger.order.length}`
);

/** Bundle statement indexes assigned to each file. */
const stmtsByFile = new Map<string, number[]>();
for (let i = 0; i < ledger.order.length && i < body.length; i++) {
  const list = stmtsByFile.get(ledger.order[i]) ?? [];
  list.push(i);
  stmtsByFile.set(ledger.order[i], list);
}

// One traversal, collecting every binding by (name -> list of declaring
// statement index). Traversing per rename would be O(renames * bundle).
const homes = new Map<string, Map<number, number>>(); // name -> stmtIdx -> count
const ranges = body.map((s) => [s.start ?? -1, s.end ?? -1] as const);
function statementOf(pos: number): number {
  let lo = 0;
  let hi = ranges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const [a, b] = ranges[mid];
    if (pos < a) hi = mid - 1;
    else if (pos > b) lo = mid + 1;
    else return mid;
  }
  return -1;
}

traverse(ast, {
  Scopable(p: NodePath) {
    for (const [name, binding] of Object.entries(p.scope.bindings)) {
      if (binding.scope.block !== p.node) continue;
      const pos = binding.identifier.start;
      if (pos == null) continue;
      const idx = statementOf(pos);
      if (idx < 0) continue;
      const per = homes.get(name) ?? new Map<number, number>();
      per.set(idx, (per.get(idx) ?? 0) + 1);
      homes.set(name, per);
    }
  }
});

let unique = 0;
let ambiguous = 0;
let missing = 0;
const perKind = new Map<string, [number, number, number]>();
const examples: string[] = [];

for (const r of renames) {
  const idxs = new Set(stmtsByFile.get(r.file) ?? []);
  const per = homes.get(r.from);
  let count = 0;
  if (per) for (const [idx, n] of per) if (idxs.has(idx)) count += n;
  const bucket = count === 1 ? 0 : count === 0 ? 2 : 1;
  if (bucket === 0) unique++;
  else if (bucket === 1) {
    ambiguous++;
    if (examples.length < 15) {
      examples.push(`AMBIGUOUS x${count}  ${r.from} -> ${r.to}  in ${r.file}`);
    }
  } else {
    missing++;
    if (examples.length < 15) {
      examples.push(`MISSING        ${r.from} -> ${r.to}  in ${r.file}`);
    }
  }
  const k = perKind.get(r.kind) ?? [0, 0, 0];
  k[bucket]++;
  perKind.set(r.kind, k);
}

const pad = (n: number, w = 6) => String(n).padStart(w);
const pct = (n: number) =>
  `${((100 * n) / renames.length).toFixed(1)}%`.padStart(7);
console.log(`\n=== 054 BUNDLE-CARRY CEILING — ${LABEL} ===`);
console.log(`  renames in the trail: ${renames.length}`);
console.log(`  UNIQUE (carryable):   ${pad(unique)} ${pct(unique)}`);
console.log(`  AMBIGUOUS:            ${pad(ambiguous)} ${pct(ambiguous)}`);
console.log(`  MISSING from bundle:  ${pad(missing)} ${pct(missing)}`);
console.log(`ROW|${LABEL}|${renames.length}|${unique}|${ambiguous}|${missing}`);
console.log(`\n  by tier (unique / ambiguous / missing):`);
for (const [k, v] of perKind) {
  console.log(`    ${k.padEnd(12)} ${v[0]} / ${v[1]} / ${v[2]}`);
}
console.log(`\n  examples:`);
for (const e of examples) console.log(`    ${e}`);

/**
 * 054 — WHICH carried rename breaks the bundle's pure-rename invariant?
 *
 *   npx tsx carry-bisect.ts <treeRoot> <renameTrail.log>
 *
 * The carry applies every rename and then checks the whole-program structural
 * signature, so one bad rename discards all of them. The zero-rename control
 * holds, so at least one is genuinely changing the program. Bisection over the
 * rename set finds a minimal failing one; theorising about which class it is
 * has been wrong twice in this experiment already.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../../src/output-validation.js";
import { attemptValidatedRename } from "../../src/rename/validated-rename.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [TREE, TRAIL] = process.argv.slice(2);
const ledger = JSON.parse(
  fs.readFileSync(path.join(TREE, ".humanify", "split-ledger.json"), "utf8")
) as StableSplitLedger;
const CODE = fs.readFileSync(
  path.join(TREE, ".humanify", "humanified.js"),
  "utf8"
);

interface Rec {
  from: string;
  to: string;
}
const LINE = /^\s+(\S+\.js):\s+(\S+)\s+->\s+(\S+)\s+\[(\w+),/;
const all: Rec[] = [];
for (const l of fs.readFileSync(TRAIL, "utf8").split("\n")) {
  const m = LINE.exec(l);
  if (m) all.push({ from: m[2], to: m[3] });
}

function wrapperBody(file: t.File): t.Statement[] {
  let found: t.Statement[] | null = null;
  traverse(file, {
    Function(p: NodePath<t.Function>) {
      if (found) return;
      const b = p.node.body;
      if (b.type !== "BlockStatement") return;
      if (b.body.length !== ledger.order.length) return;
      found = b.body;
      p.stop();
    }
  });
  if (!found) throw new Error("wrapper body not found");
  return found;
}

/** Apply `subset` to a fresh parse; true when the invariant still holds. */
function holds(subset: Rec[]): { ok: boolean; applied: number } {
  const ast = parseSourceAst(CODE, { filename: "humanified.js" });
  if (!ast) throw new Error("parse failed");
  const baseline = captureSemanticBaseline(ast);
  const body = wrapperBody(ast);
  const spans = body.map((s) => [s.start ?? -1, s.end ?? -1] as const);
  const inBody = (pos: number) => spans.some(([a, b]) => pos >= a && pos <= b);
  const byName = new Map<string, NodePath[]>();
  traverse(ast, {
    Scopable(p: NodePath) {
      for (const [name, binding] of Object.entries(p.scope.bindings)) {
        if (binding.scope.block !== p.node) continue;
        const s = binding.identifier.start;
        if (s == null || !inBody(s)) continue;
        const list = byName.get(name) ?? [];
        list.push(p);
        byName.set(name, list);
      }
    }
  });
  let applied = 0;
  for (const r of subset) {
    const cands = byName.get(r.from);
    if (!cands || cands.length !== 1) continue;
    if (attemptValidatedRename(cands[0].scope, r.from, r.to).applied) applied++;
  }
  const ok = !checkStructuralInvariant(ast, baseline);
  return { ok, applied };
}

const full = holds(all);
console.log(
  `all ${all.length} renames: applied ${full.applied}, invariant ${full.ok ? "HOLDS" : "FAILS"}`
);
if (full.ok) {
  console.log(
    "nothing to bisect — the AST-level invariant holds for the whole set"
  );
  process.exit(0);
}

// Shrink to a minimal failing subset.
let failing = all;
let changed = true;
while (changed && failing.length > 1) {
  changed = false;
  const mid = Math.floor(failing.length / 2);
  for (const half of [failing.slice(0, mid), failing.slice(mid)]) {
    if (half.length === 0) continue;
    const r = holds(half);
    console.log(
      `  probe ${half.length} renames: applied ${r.applied}, ${r.ok ? "holds" : "FAILS"}`
    );
    if (!r.ok) {
      failing = half;
      changed = true;
      break;
    }
  }
}
console.log(`\nMINIMAL FAILING SET (${failing.length}):`);
for (const r of failing) console.log(`  ${r.from} -> ${r.to}`);

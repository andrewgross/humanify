/**
 * 054 — why did the bundle carry's textual rewrite fail its soundness check?
 *
 *   npx tsx carry-diagnose.ts <treeRoot> <renameTrail.log>
 *
 * Replays the carry against the tree's own bundle and, when the rewritten text
 * is not a pure rename of the original, finds the FIRST wrapper statement whose
 * structure changed and prints both sides of it. A structural signature over the
 * whole program says only "something moved"; this says what.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import { attemptValidatedRename } from "../../src/rename/validated-rename.js";
import { statementHash } from "../../src/split/statement-hash.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [TREE, TRAIL] = process.argv.slice(2);
const ledger = JSON.parse(
  fs.readFileSync(path.join(TREE, ".humanify", "split-ledger.json"), "utf8")
) as StableSplitLedger;
const bundlePath = path.join(TREE, ".humanify", "humanified.js");
const original = fs.readFileSync(bundlePath, "utf8");

interface Rec {
  file: string;
  from: string;
  to: string;
}
const LINE = /^\s+(\S+\.js):\s+(\S+)\s+->\s+(\S+)\s+\[(\w+),/;
const renames: Rec[] = [];
for (const l of fs.readFileSync(TRAIL, "utf8").split("\n")) {
  const m = LINE.exec(l);
  if (m) renames.push({ file: m[1], from: m[2], to: m[3] });
}
console.log(`renames in trail: ${renames.length}`);
console.log(`ledger.emitIndexes present: ${Boolean(ledger.emitIndexes)}`);

const ast = parseSourceAst(original, { filename: "humanified.js" });
if (!ast) throw new Error("bundle did not parse");

function wrapperBody(file: t.File, expected: number): t.Statement[] {
  let found: t.Statement[] | null = null;
  traverse(file, {
    Function(p: NodePath<t.Function>) {
      if (found) return;
      const b = p.node.body;
      if (b.type !== "BlockStatement" || b.body.length !== expected) return;
      found = b.body;
      p.stop();
    }
  });
  if (!found) throw new Error("wrapper body not found");
  return found;
}
const body = wrapperBody(ast, ledger.order.length);
const before = body.map((s) => statementHash(s));
const beforeText = body.map((s) => original.slice(s.start ?? 0, s.end ?? 0));

// Rename in the AST exactly as the carry does, then rewrite the text.
const slots = new Map<string, number[]>();
for (let slot = 0; slot < ledger.order.length; slot++) {
  const l = slots.get(ledger.order[slot]) ?? [];
  l.push(slot);
  slots.set(ledger.order[slot], l);
}
let applied = 0;
traverseRename();
function traverseRename(): void {
  // Reuse the shipped path so the diagnosis is about the real thing.
  const decls: Array<{
    stmt: number;
    name: string;
    start: number;
    path: NodePath;
  }> = [];
  const spans = body.map((s) => [s.start ?? -1, s.end ?? -1] as const);
  const stmtOf = (pos: number) => {
    let lo = 0;
    let hi = spans.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (pos < spans[mid][0]) hi = mid - 1;
      else if (pos > spans[mid][1]) lo = mid + 1;
      else return mid;
    }
    return -1;
  };
  traverse(ast as t.File, {
    Scopable(p: NodePath) {
      for (const [name, b] of Object.entries(p.scope.bindings)) {
        if (b.scope.block !== p.node) continue;
        const s = b.identifier.start;
        if (s == null) continue;
        const i = stmtOf(s);
        if (i >= 0) decls.push({ stmt: i, name, start: s, path: p });
      }
    }
  });
  for (const r of renames) {
    const cands = decls
      .filter((d) => d.name === r.from)
      .sort((a, b2) => a.start - b2.start);
    if (cands.length !== 1) continue; // diagnosis only needs unambiguous ones
    if (attemptValidatedRename(cands[0].path.scope, r.from, r.to).applied) {
      applied++;
    }
  }
}
console.log(`applied in AST: ${applied}`);

const IDENT_AT = /^[A-Za-z_$][\w$]*/;
const lines = original.split("\n");
const subs: Array<{ line: number; col: number; from: string; to: string }> = [];
traverse(ast, {
  Identifier(p: NodePath<t.Identifier>) {
    const loc = p.node.loc;
    if (!loc) return;
    const text = lines[loc.start.line - 1];
    if (text === undefined) return;
    const m = IDENT_AT.exec(text.slice(loc.start.column));
    if (!m || m[0] === p.node.name) return;
    subs.push({
      line: loc.start.line,
      col: loc.start.column,
      from: m[0],
      to: p.node.name
    });
  }
});
console.log(`textual substitutions: ${subs.length}`);

const byLine = new Map<number, typeof subs>();
for (const s of subs) {
  const l = byLine.get(s.line) ?? [];
  l.push(s);
  byLine.set(s.line, l);
}
const out = lines.slice();
for (const [ln, list] of byLine) {
  list.sort((a, b2) => b2.col - a.col);
  let text = out[ln - 1];
  for (const s of list) {
    text = text.slice(0, s.col) + s.to + text.slice(s.col + s.from.length);
  }
  out[ln - 1] = text;
}
const rewritten = out.join("\n");

const reparsed = parseSourceAst(rewritten, { filename: "humanified.js" });
if (!reparsed) {
  console.log("REWRITTEN BUNDLE DOES NOT PARSE");
  process.exit(1);
}
const after = wrapperBody(reparsed, ledger.order.length).map((s) =>
  statementHash(s)
);
let mismatches = 0;
for (let i = 0; i < before.length; i++) {
  if (before[i] === after[i]) continue;
  mismatches++;
  if (mismatches <= 3) {
    const afterBody = wrapperBody(reparsed, ledger.order.length)[i];
    console.log(
      `\n### statement ${i} structure changed (file ${ledger.order[i]})`
    );
    console.log("--- BEFORE ---");
    console.log(beforeText[i].slice(0, 700));
    console.log("--- AFTER ---");
    console.log(
      rewritten.slice(afterBody.start ?? 0, (afterBody.start ?? 0) + 700)
    );
  }
}
console.log(`\nstatements whose structure changed: ${mismatches}`);

/**
 * 062 Task 0 — duplicate-instance census, rename-blind the RIGHT way.
 *
 *   npx tsx experiments/062-duplicate-instances/census.ts <treeRoot> [...more]
 *
 * The v1 regex census masked EVERY identifier and over-merged: `is-null`
 * and `is-undefined` differ only in the free identifier `undefined`, so
 * blanket masking called them one family. Here canonicalization is
 * scope-aware (Babel): only identifiers that resolve to bindings
 * DECLARED IN THE FILE are masked (require aliases, __commonJS params,
 * locals). Free identifiers, property names, and literals stay visible.
 * Require paths keep their target BASENAME (instances differ only in
 * directory prefix).
 *
 * A family = >1 vendor file with identical canonical text. "stub" =
 * single-statement re-export shape (`exports.f = __commonJS((a,b) =>
 * { b.exports = <expr> })` possibly preceded by requires/const only) —
 * the collapse guard is this SHAPE, never the hash alone.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";
import { parse } from "@babel/parser";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";

const roots = process.argv.slice(2);
if (roots.length === 0) {
  console.error("usage: census.ts <treeRoot> [...more]");
  process.exit(1);
}

function walk(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".js")) out.push(p);
  }
  return out;
}

interface CanonResult {
  canon: string;
  isStub: boolean;
}

/** Positions (start offsets) of identifiers bound within this file. */
function localIdentifierOffsets(ast: t.File): Set<number> {
  const offsets = new Set<number>();
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const binding = p.scope.getBinding(p.node.name);
      if (!binding) return;
      // Bound somewhere in THIS file (all scopes are in-file for a module).
      if (p.node.start != null) offsets.add(p.node.start);
    }
  });
  return offsets;
}

/**
 * Stub shape: every top-level statement is a require-alias const or the
 * single `exports.f = __commonJS(fn)` whose body is one ExpressionStatement
 * assigning `<param>.exports = <expr>`.
 */
function isStubShape(ast: t.File): boolean {
  let sawExport = false;
  for (const stmt of ast.program.body) {
    if (t.isVariableDeclaration(stmt)) continue;
    if (!t.isExpressionStatement(stmt)) return false;
    const e = stmt.expression;
    if (
      !t.isAssignmentExpression(e) ||
      !t.isMemberExpression(e.left) ||
      !t.isCallExpression(e.right) ||
      e.right.arguments.length !== 1
    )
      return false;
    const fn = e.right.arguments[0];
    if (!t.isArrowFunctionExpression(fn) && !t.isFunctionExpression(fn))
      return false;
    const body = t.isBlockStatement(fn.body) ? fn.body.body : null;
    if (body) {
      if (body.length !== 1) return false;
      const inner = body[0];
      if (
        !t.isExpressionStatement(inner) ||
        !t.isAssignmentExpression(inner.expression) ||
        !t.isMemberExpression(inner.expression.left)
      )
        return false;
    }
    if (sawExport) return false;
    sawExport = true;
  }
  return sawExport;
}

function canonicalize(code: string): CanonResult | null {
  let ast: t.File;
  try {
    ast = parse(code, { sourceType: "unambiguous" });
  } catch {
    return null;
  }
  const locals = localIdentifierOffsets(ast);
  // Rewrite from the end so offsets stay valid.
  const edits: { start: number; end: number; text: string }[] = [];
  traverse(ast, {
    Identifier(p: NodePath<t.Identifier>) {
      const { start, end } = p.node;
      if (start == null || end == null) return;
      if (locals.has(start)) edits.push({ start, end, text: "L" });
    },
    StringLiteral(p: NodePath<t.StringLiteral>) {
      const { start, end } = p.node;
      if (start == null || end == null) return;
      const v = p.node.value;
      if (v.includes("/") && v.endsWith(".js")) {
        edits.push({ start, end, text: JSON.stringify(path.basename(v)) });
      }
    }
  });
  edits.sort((a, b) => b.start - a.start);
  let canon = code;
  for (const e of edits) {
    canon = canon.slice(0, e.start) + e.text + canon.slice(e.end);
  }
  return { canon: canon.replace(/\s+/g, " "), isStub: isStubShape(ast) };
}

for (const root of roots) {
  const vendorDir = path.join(root, "vendor");
  if (!fs.existsSync(vendorDir)) {
    console.error(`no vendor/ under ${root}`);
    continue;
  }
  const byCanon = new Map<
    string,
    { files: string[]; isStub: boolean; bytes: number }
  >();
  let parsed = 0;
  for (const f of walk(vendorDir)) {
    const code = fs.readFileSync(f, "utf8");
    const res = canonicalize(code);
    if (!res) continue;
    parsed++;
    const h = crypto.createHash("sha1").update(res.canon).digest("hex");
    const entry = byCanon.get(h) ?? {
      files: [],
      isStub: res.isStub,
      bytes: code.length
    };
    entry.files.push(path.relative(vendorDir, f));
    byCanon.set(h, entry);
  }
  const fams = [...byCanon.values()].filter((v) => v.files.length > 1);
  const stubFams = fams.filter((v) => v.isStub);
  const codeFams = fams.filter((v) => !v.isStub);
  const count = (l: typeof fams) => l.reduce((s, v) => s + v.files.length, 0);
  console.log(`\n=== ${root} ===`);
  console.log(
    `vendor files parsed: ${parsed}; duplicate families: ${fams.length} (${count(fams)} files)`
  );
  console.log(
    `  stub families: ${stubFams.length} (${count(stubFams)} files)  |  code families: ${codeFams.length} (${count(codeFams)} files)`
  );
  for (const v of fams
    .sort((a, b) => b.files.length - a.files.length)
    .slice(0, 12)) {
    console.log(
      `  n=${String(v.files.length).padStart(3)} ${v.isStub ? "stub" : "code"} ${String(v.bytes).padStart(5)}B  ${v.files[0]}  |  ${v.files[1]}`
    );
  }
}

/**
 * Exp023: the two numbers that shape runnable emission.
 * 1. Cross-file WRITES: assignments to a top-level binding declared in a
 *    different file — illegal as ESM imports (import bindings are
 *    read-only); these statements must colocate with their binding or the
 *    emitter needs another strategy.
 * 2. Forward references: statements referencing bindings declared LATER
 *    (legal in one hoisted scope; under module imports they force early
 *    evaluation, reordering side effects).
 * Usage: npx tsx measure-emit-feasibility.ts <humanified.js> <ledger.json>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

const [input, ledgerPath] = process.argv.slice(2);
const ast = parseSync(fs.readFileSync(input, "utf-8"), {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
if (!ast || ast.type !== "File") throw new Error("parse");
const first = ast.program.body[0];
if (!t.isExpressionStatement(first)) throw new Error("no wrapper");
let expr = first.expression;
if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body))
  throw new Error("no wrapper fn");
const body = expr.body.body;

const nameToFiles = new Map<string, string[]>(
  Object.entries(JSON.parse(fs.readFileSync(ledgerPath, "utf-8")).nameToFiles)
);
const fileOf = (n: string) => nameToFiles.get(n)?.[0];

const declIndex = new Map<string, number>();
body.forEach((s, i) => {
  for (const n of Object.keys(t.getBindingIdentifiers(s, false))) {
    if (!declIndex.has(n)) declIndex.set(n, i);
  }
});

// Statement -> file via its own first declared name, else skip.
let crossWriteStmts = 0;
const crossWrittenBindings = new Set<string>();
let fwdRefStmts = 0;
let fwdRefsFromSideEffects = 0;

body.forEach((stmt, i) => {
  const ownNames = new Set(Object.keys(t.getBindingIdentifiers(stmt, false)));
  const ownFile = [...ownNames].map(fileOf).find(Boolean);
  let crossWrite = false;
  let fwd = false;
  t.traverseFast(stmt, (node) => {
    if (t.isAssignmentExpression(node) && t.isIdentifier(node.left)) {
      const name = node.left.name;
      if (!ownNames.has(name) && declIndex.has(name)) {
        const bindingFile = fileOf(name);
        if (bindingFile && ownFile && bindingFile !== ownFile) {
          crossWrite = true;
          crossWrittenBindings.add(name);
        }
      }
    }
    if (t.isIdentifier(node)) {
      const di = declIndex.get(node.name);
      if (di !== undefined && di > i && !ownNames.has(node.name)) fwd = true;
    }
  });
  if (crossWrite) crossWriteStmts++;
  if (fwd) {
    fwdRefStmts++;
    if (t.isExpressionStatement(stmt)) fwdRefsFromSideEffects++;
  }
});

console.log(
  JSON.stringify(
    {
      statements: body.length,
      crossFileWriteStatements: crossWriteStmts,
      crossWrittenBindings: crossWrittenBindings.size,
      forwardRefStatements: fwdRefStmts,
      forwardRefSideEffectStatements: fwdRefsFromSideEffects
    },
    null,
    2
  )
);

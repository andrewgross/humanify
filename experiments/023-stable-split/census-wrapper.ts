/**
 * Exp023: census the REAL splitting population — the statements inside the
 * Bun CJS wrapper IIFE that the current splitter never looks into.
 * Usage: npx tsx census-wrapper.ts <humanified.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

const ast = parseSync(fs.readFileSync(process.argv[2], "utf-8"), {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
if (!ast || ast.type !== "File") throw new Error("parse failed");

const first = ast.program.body[0];
let body: t.Statement[] | null = null;
if (t.isExpressionStatement(first)) {
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (
    (t.isFunctionExpression(expr) || t.isArrowFunctionExpression(expr)) &&
    t.isBlockStatement(expr.body)
  ) {
    body = expr.body.body;
  }
}
if (!body) {
  console.log("no wrapper found; program.body =", ast.program.body.length);
  process.exit(0);
}

console.log("wrapper body statements:", body.length);
const byType = new Map<string, number>();
let fnDecls = 0;
let classDecls = 0;
let varDeclarators = 0;
for (const s of body) {
  byType.set(s.type, (byType.get(s.type) ?? 0) + 1);
  if (t.isFunctionDeclaration(s)) fnDecls++;
  if (t.isClassDeclaration(s)) classDecls++;
  if (t.isVariableDeclaration(s)) varDeclarators += s.declarations.length;
}
console.log(
  "by type:",
  [...byType].sort((a, b) => b[1] - a[1])
);
console.log(
  `function decls: ${fnDecls}, class decls: ${classDecls}, var declarators: ${varDeclarators}`
);

// Named-binding share: statements that declare at least one named binding
// (the anchor for prior-assignment transfer by stable name).
let withBindings = 0;
for (const s of body) {
  const ids = t.getBindingIdentifiers(s, false);
  if (Object.keys(ids).length > 0) withBindings++;
}
console.log(
  `statements declaring >=1 binding: ${withBindings} (${((100 * withBindings) / body.length).toFixed(1)}%)`
);

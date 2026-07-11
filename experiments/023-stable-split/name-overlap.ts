/**
 * Exp023 Step-0: cross-leg overlap of wrapper-body declared binding names —
 * the ceiling on name-carried file-assignment transfer (the split-stability
 * analog of exp022's addressable-population sizing).
 * Usage: npx tsx name-overlap.ts <legA.js> <legB.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

function wrapperBodyNames(file: string): { stmts: number; names: Set<string> } {
  const ast = parseSync(fs.readFileSync(file, "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast || ast.type !== "File") throw new Error(`parse failed: ${file}`);
  const first = ast.program.body[0];
  if (!t.isExpressionStatement(first)) throw new Error("no wrapper");
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body))
    throw new Error("no wrapper fn");
  const names = new Set<string>();
  for (const s of expr.body.body) {
    for (const name of Object.keys(t.getBindingIdentifiers(s, false))) {
      names.add(name);
    }
  }
  return { stmts: expr.body.body.length, names };
}

const a = wrapperBodyNames(process.argv[2]);
const b = wrapperBodyNames(process.argv[3]);
const shared = [...a.names].filter((n) => b.names.has(n));
console.log(`legA: ${a.stmts} stmts, ${a.names.size} declared names`);
console.log(`legB: ${b.stmts} stmts, ${b.names.size} declared names`);
console.log(
  `shared names: ${shared.length} (${((100 * shared.length) / a.names.size).toFixed(1)}% of A, ${((100 * shared.length) / b.names.size).toFixed(1)}% of B)`
);
console.log(
  `A-only: ${a.names.size - shared.length}, B-only: ${b.names.size - shared.length}`
);

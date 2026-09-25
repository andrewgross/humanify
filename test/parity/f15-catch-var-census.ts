/**
 * 16-findings-queue #15 census: does the catch-var capture fire in real
 * runs? Its signature is a catch clause whose parameter NAME equals a `var`
 * declared in that catch body (a `var` in a catch body is function-scoped,
 * so sharing the name makes the initializer write the catch param instead).
 * Count per text; the bug's firings = count(output) - count(input).
 *   npx tsx test/parity/f15-catch-var-census.ts <file.js>...
 */
import * as fs from "node:fs";
import { parse } from "@babel/parser";
import type * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";

/** A function boundary: a `var` inside it belongs to that function. */
function isFunctionNode(node: t.Node): boolean {
  return (
    node.type === "FunctionDeclaration" ||
    node.type === "FunctionExpression" ||
    node.type === "ArrowFunctionExpression" ||
    node.type === "ClassMethod" ||
    node.type === "ObjectMethod"
  );
}

/** The AST children of a node (every nested object with a `type`). */
function childrenOf(node: t.Node): t.Node[] {
  const out: t.Node[] = [];
  for (const value of Object.values(node)) {
    const items = Array.isArray(value) ? value : [value];
    for (const v of items) {
      if (v && typeof v === "object" && "type" in v) out.push(v as t.Node);
    }
  }
  return out;
}

/** Names of the `var` declarations in a catch body, not crossing functions. */
function varNamesIn(body: t.BlockStatement): Set<string> {
  const names = new Set<string>();
  const stack: t.Node[] = [body];
  while (stack.length > 0) {
    const node = stack.pop() as t.Node;
    if (isFunctionNode(node)) continue;
    if (node.type === "VariableDeclaration" && node.kind === "var") {
      for (const d of node.declarations) {
        if (d.id.type === "Identifier") names.add(d.id.name);
      }
    }
    stack.push(...childrenOf(node));
  }
  return names;
}

for (const file of process.argv.slice(2)) {
  const ast = parse(fs.readFileSync(file, "utf8"), {
    sourceType: "unambiguous",
    errorRecovery: true
  });
  let hits = 0;
  const examples: string[] = [];
  traverse(ast, {
    CatchClause(p: { node: t.CatchClause }) {
      const param = p.node.param;
      if (param?.type !== "Identifier") return;
      if (varNamesIn(p.node.body).has(param.name)) {
        hits++;
        if (examples.length < 3)
          examples.push(`${param.name}@${p.node.loc?.start.line}`);
      }
    }
  });
  console.log(`${hits}\t${file}\t${examples.join(" ")}`);
}

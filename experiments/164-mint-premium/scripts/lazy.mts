import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const body: any[] = (ast.program.body[0] as any).expression.body.body;
// count raw container statements that are var decls whose init is a CallExpression of an identifier with one ArrowFunctionExpression arg
let lazy = 0;
const samples: string[] = [];
for (const s of body) {
  if (s.type === "VariableDeclaration" && s.declarations.length === 1) {
    const init = s.declarations[0].init;
    if (init && init.type === "CallExpression" && init.callee.type === "Identifier" && init.arguments.length === 1 && init.arguments[0].type === "ArrowFunctionExpression") {
      lazy++;
      if (samples.length < 3) samples.push(raw.slice(s.start, s.start + 80));
    }
  }
}
console.log("raw lazy-init-looking statements:", lazy, samples);
// how many raw container statements total are var single-declarator with ANY call init
let anyCall = 0;
for (const s of body) {
  if (s.type === "VariableDeclaration" && s.declarations.length === 1) {
    const init = s.declarations[0].init;
    if (init && init.type === "CallExpression") anyCall++;
  }
}
console.log("raw single-declarator var with call init:", anyCall);

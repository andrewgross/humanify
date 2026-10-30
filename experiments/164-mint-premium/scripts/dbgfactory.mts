import * as fs from "node:fs";
import * as parser from "@babel/parser";
import _traverse from "@babel/traverse";
const traverseMod: any = _traverse;
const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const raw = fs.readFileSync(RAW, "utf8");
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rBody: any[] = (() => {
  const p0: any = rAst.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return rAst.program.body;
})();
console.log("raw stmt 1016:", rBody[1016].type, rBody[1016].start, "..", rBody[1016].end);
console.log("head:", raw.slice(rBody[1016].start, rBody[1016].start + 300).replace(/\n/g, "\\n"));
console.log("...");
console.log("tail:", raw.slice(rBody[1016].end - 200, rBody[1016].end).replace(/\n/g, "\\n"));
// find factory declarators in it
const st: any = rBody[1016];
if (st.type === "VariableDeclaration") {
  for (const d of st.declarations) {
    const init: any = d.init;
    const isFactory = init && (init.type === "CallExpression" || init.type === "NewExpression") && init.arguments?.some((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
    console.log("declarator:", d.id?.name, d.start, "..", d.end, "factory:", isFactory);
    if (isFactory) {
      const fn = init.arguments.find((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
      console.log("  body:", fn.body.start, "..", fn.body.end, "body stmts:", fn.body.body.length);
      for (const bs of fn.body.body.slice(0, 6)) {
        console.log("   -", bs.type, bs.start, "..", bs.end, raw.slice(bs.start, bs.start + 60).replace(/\n/g, "\\n"));
      }
    }
  }
}

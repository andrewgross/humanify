import * as fs from "node:fs";
import * as parser from "@babel/parser";
const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const junk: any[] = JSON.parse(fs.readFileSync(`/work/exp164-mint-census/shipped-located-exp161-names-hop.json`, "utf8"));
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const raw = fs.readFileSync(RAW, "utf8");
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = rAst.program.body[0];
console.log("raw p0 type:", p0.type, "program body len:", rAst.program.body.length);
function bodyStatements(ast: any): any[] {
  const p = ast.program.body[0];
  if (p.type === "ExpressionStatement" && p.expression.type === "FunctionExpression") return p.expression.body.body;
  if (p.type === "FunctionDeclaration") return p.body.body;
  return ast.program.body;
}
const rBody = bodyStatements(rAst);
console.log("rBody len:", rBody.length, "p0.type:", rBody[0]?.type);
const junkNames = new Set(junk.map((r) => r.name));
function junkNamesOf(node: any): string[] {
  const out: string[] = [];
  const visit = (n: any) => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) { n.forEach(visit); return; }
    if (n.type === "Identifier" && junkNames.has(n.name)) out.push(n.name);
    for (const k of Object.keys(n)) {
      if (k === "leadingComments" || k === "trailingComments" || k === "loc") continue;
      const v = (n as any)[k];
      if (v && typeof v === "object") visit(v);
    }
  };
  visit(node);
  return out;
}
let noNames = 0, withNames = 0;
for (const st of rBody.slice(0, 200)) {
  const n = junkNamesOf(st);
  if (n.length === 0) noNames++; else withNames++;
}
console.log("first 200 raw stmts: no junk names:", noNames, "with:", withNames);
// what does the raw file start with?
console.log("raw head:", raw.slice(0, 200));
const sAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sBody = bodyStatements(sAst);
console.log("shipped stmt 39 text:", shipped.slice(sBody[39].start, sBody[39].start + 300));

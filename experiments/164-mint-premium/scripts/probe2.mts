import * as fs from "node:fs";
import * as parser from "@babel/parser";
const shipped = fs.readFileSync(`/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js`, "utf8");
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const sAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
let sBody: any[] = sAst.program.body as any[];
const p0: any = sAst.program.body[0];
if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") sBody = p0.expression.body.body;
let rBody: any[] = rAst.program.body as any[];
const q0: any = rAst.program.body[0];
if (q0.type === "ExpressionStatement" && q0.expression.type === "FunctionExpression") rBody = q0.expression.body.body;

function literals(node: any): string[] {
  const out: string[] = [];
  const visit = (n: any) => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) { n.forEach(visit); return; }
    if (n.type === "StringLiteral") out.push(n.value);
    else if (n.type === "TemplateElement") out.push(n.value.cooked ?? "");
    for (const k of Object.keys(n)) {
      if (k === "leadingComments" || k === "trailingComments" || k === "loc" || k === "regex") continue;
      const v = (n as any)[k];
      if (v && typeof v === "object") visit(v);
    }
  };
  visit(node);
  return out.sort();
}
const stmt = sBody[1129];
const ls = literals(stmt);
console.log("shipped stmt 1129:", stmt.type, "span", stmt.start, stmt.end, "literals:", ls.length, ls.slice(0, 6));
// raw declarators that are call-with-function-arg
const cands: any[] = [];
for (const st of rBody) {
  if (st.type !== "VariableDeclaration") continue;
  for (const d of st.declarations) {
    if (d.init && (d.init.type === "CallExpression" || d.init.type === "NewExpression")) {
      cands.push(d);
    }
  }
}
console.log("raw factory declarator candidates:", cands.length);
const target = ls.join("\u0000");
let hits = 0;
for (const d of cands) {
  const dl = literals(d);
  if (dl.join("\u0000") === target) {
    hits++;
    if (hits <= 3) console.log("DECLARATOR MATCH: span", d.start, d.end, "literals", dl.length);
  }
}
console.log("exact literal-multiset declarator matches:", hits);

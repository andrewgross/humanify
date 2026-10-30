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
const st = sBody[1129];
const ls = literals(st);
console.log("shipped stmt 1129 lits:", ls.length);
const q = rBody.find((s: any) => s.type === "VariableDeclaration" && s.declarations.some((d: any) => d.start === 100905 - 0));
// find raw statement containing 100905
const host = rBody.find((s: any) => s.start <= 100905 && s.end >= 109173);
console.log("host statement span:", host?.start, host?.end, "type", host?.type);
const d = host.declarations.find((x: any) => x.start === 100905);
const dl = literals(d);
console.log("declarator lits:", dl.length, "equal:", dl.join("\u0000") === ls.join("\u0000"));
const diff1 = ls.filter((x: string) => !dl.includes(x));
const diff2 = dl.filter((x: string) => !ls.includes(x));
console.log("shipped-only lits:", diff1.length, diff1.slice(0, 10));
console.log("raw-only lits:", diff2.length, diff2.slice(0, 10));

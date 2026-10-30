import * as fs from "node:fs";
import * as parser from "@babel/parser";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/exp161-names-hop/2.1.216.diag.json`, "utf8"));
const trails = diag.placementTrails.trails;
const sAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = sAst.program.body[0];
const sBody: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : p0.type === "FunctionDeclaration" ? p0.body.body : sAst.program.body;
let ok = 0, bad = 0;
for (const i of [0, 1, 39, 42, 1000, 5000, 20000, 35000, 35902]) {
  const t = trails[i];
  const s = sBody[i];
  const same = t.span.start === s.start && t.span.end === s.end;
  if (same) ok++; else { bad++; console.log(`stmt ${i}: trail ${t.span.start}..${t.span.end} vs shipped ${s.start}..${s.end} MISMATCH`); }
}
console.log(`trail-span vs shipped-span: ${ok} match, ${bad} mismatch (of 9 probed)`);
// also: how many trails total match shipped spans exactly? sample 2000
let m = 0, n = 0;
for (let i = 0; i < 35903; i += 17) {
  const t = trails[i];
  const s = sBody[i];
  if (t.span.start === s.start && t.span.end === s.end) m++;
  n++;
}
console.log(`sampled ${n}: ${m} match`);

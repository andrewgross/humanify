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
const rawLits: { pos: number; v: string }[] = [];
traverseMod.default(rAst, {
  StringLiteral: (p: { node: { start: number; value: string } }) => { if (p.node.start != null) rawLits.push({ pos: p.node.start, v: p.node.value }); },
  TemplateElement: (p: { node: { start: number; value: { cooked: string | null } } }) => { if (p.node.start != null) rawLits.push({ pos: p.node.start, v: p.node.value.cooked ?? "" }); },
});
rawLits.sort((a, b) => a.pos - b.pos);
const pos: number[] = rawLits.map((x) => x.pos);
function stmtOfRaw(p: number): number {
  let lo = 0, hi = rBody.length - 1;
  while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (rBody[mid].start <= p) lo = mid; else hi = mid - 1; }
  return lo;
}
for (const f of ["vendor/lodash/module-1275db54.js", "vendor/zod/module-b9b02094.js", "vendor/jose/module-aa4d483d.js"]) {
  const text = fs.readFileSync(`${ROOT}/${f}`, "utf8");
  const a = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
  const vals: string[] = [];
  traverseMod.default(a, {
    StringLiteral: (p: { node: { value: string } }) => { vals.push(p.node.value); },
    TemplateElement: (p: { node: { value: { cooked: string | null } } }) => { vals.push(p.node.value.cooked ?? ""); },
  });
  const uniq = [...new Set(vals)].filter((v) => v.length > 3);
  console.log(`\n=== ${f}: ${text.length} B, ${vals.length} lits, ${uniq.length} uniq(len>3)`);
  const stmtHits = new Map<number, number>();
  let notFound = 0;
  for (const v of uniq) {
    const needle = JSON.stringify(v).slice(1, -1);
    const at = raw.indexOf(needle);
    if (at < 0) { notFound++; continue; }
    stmtHits.set(stmtOfRaw(at), (stmtHits.get(stmtOfRaw(at)) ?? 0) + 1);
  }
  console.log("not found in raw:", notFound, "of", uniq.length);
  const hits = [...stmtHits.entries()].sort((x, y) => x[0] - y[0]);
  console.log("raw stmt hits:", hits.slice(0, 12), "..." , "total stmts hit:", hits.length);
  if (hits.length > 0) {
    const lo = hits[0][0], hi2 = hits[hits.length - 1][0];
    console.log("raw stmt range:", lo, "..", hi2, "contiguous:", hi2 - lo + 1 === hits.length);
    console.log("span bytes:", rBody[lo].start, "..", rBody[hi2].end);
  }
}

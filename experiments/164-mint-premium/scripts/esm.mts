import * as fs from "node:fs";
import * as parser from "@babel/parser";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
// find top-level statements whose source contains "__esm(" at depth 0-ish: just check the statement text starts a __esm var
const esmIdx: number[] = [];
sbody.forEach((s, i) => {
  const t = shipped.slice(s.start, s.end);
  if (/= __esm\(/.test(t) || /=__esm\(/.test(t)) esmIdx.push(i);
});
console.log("shipped statements containing '__esm(':", esmIdx.length);
console.log("first 10:", esmIdx.slice(0, 10).join(","));
console.log("last 10:", esmIdx.slice(-10).join(","));
console.log("body length:", sbody.length);
// era spans of those indices
const eras = er.bodySpans as [number, number][];
for (const i of [esmIdx[0], esmIdx[1], esmIdx[esmIdx.length - 1]]) {
  const [a, b] = eras[i];
  console.log(`era span of shipped stmt ${i}: (${a},${b}) len ${b - a}`);
}
// what does era span text look like? we don't have era text, but lengths tell
// distribution: how many esm statements before vs after 30888
const before = esmIdx.filter((i) => i <= 30888).length;
console.log("esm stmts <= 30888:", before, " > 30888:", esmIdx.length - before);

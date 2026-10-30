import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const regions = er.regions as { statements: number[]; raw_spans: [number, number][]; claims: (string | null)[] }[];
const r = regions[0];
console.log("region 0: statements", JSON.stringify(r.statements), "claims", r.claims.slice(0, 5));
for (const [s, e] of r.raw_spans) {
  console.log(`raw [${s},${e}]:`, JSON.stringify(raw.slice(s, Math.min(s + 100, e))));
}
console.log("raw ordinals:", r.raw_spans.map(([s]) => s));
// where are the corresponding era spans?
for (const st of r.statements) {
  const [a, b] = (er.bodySpans as [number, number][])[st];
  console.log(`era stmt ${st}: (${a},${b}) len ${b - a}`);
}
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
for (const st of r.statements) {
  const sp = sbody[st];
  console.log(`shipped stmt ${st}:`, shipped.slice(sp.start, Math.min(sp.start + 100, sp.end)).replace(/\n/g, " "));
}

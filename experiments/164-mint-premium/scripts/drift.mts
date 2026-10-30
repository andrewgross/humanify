import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const regions = er.regions as { statements: number[]; raw_spans: [number, number][] }[];
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
// index shipped statements by distinctive string literals
const litIdx = new Map<string, number[]>();
sbody.forEach((s, i) => {
  const t = shipped.slice(s.start, s.end);
  for (const m of t.matchAll(/"([^"\n]{12,80})"/g)) {
    const l = m[1];
    const a = litIdx.get(l) ?? [];
    a.push(i);
    litIdx.set(l, a);
  }
});
let ok = 0, bad = 0, nolit = 0, ambiguous = 0;
const deltas: number[] = [];
for (const r of regions) {
  const [s] = r.raw_spans[0];
  const t = raw.slice(s, Math.min(s + 4000, r.raw_spans[0][1]));
  const lits = [...t.matchAll(/"([^"\n]{12,80})"/g)].map((m) => m[1]);
  let hit = -1;
  for (const l of lits) {
    const cands = (litIdx.get(l) ?? []).filter((i) => i <= 35902);
    if (cands.length === 1) { hit = cands[0]; break; }
  }
  if (hit < 0) { nolit++; continue; }
  const d = hit - r.statements[0];
  deltas.push(d);
  if (d === 6) ok++; else { bad++; if (bad <= 10) console.log(`drift ${d}: region stmt ${r.statements[0]} pinned ${hit}`); }
}
console.log(`regions pinned ${ok + bad}, drift==6: ${ok}, other: ${bad}, no unique literal: ${nolit}`);
const dist = new Map<number, number>();
for (const d of deltas) dist.set(d, (dist.get(d) ?? 0) + 1);
console.log([...dist.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10));

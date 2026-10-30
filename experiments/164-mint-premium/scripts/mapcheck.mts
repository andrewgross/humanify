import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.expression.body.body;
const rawOrd = new Map<string, number>();
body.forEach((s, i) => rawOrd.set(`${s.start}:${s.end}`, i));
console.log("raw container", body.length);
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const regions = er.regions as { statements: number[]; raw_spans: [number, number][]; claims: (string | null)[] }[];
console.log("regions", regions.length);
// implied shift per region: eraStmt - rawOrd
let shifts: number[] = [];
let mismatched = 0, joined = 0;
for (const r of regions.slice(0, 200000)) {
  for (let k = 0; k < r.raw_spans.length; k++) {
    const ro = rawOrd.get(`${r.raw_spans[k][0]}:${r.raw_spans[k][1]}`);
    if (ro === undefined) { joined++; continue; }
    const sh = r.statements[k] - ro;
    shifts.push(sh);
  }
}
console.log("pairs", shifts.length, "raw-span-not-in-container", joined);
shifts.sort((a, b) => a - b);
console.log("shift min", shifts[0], "p50", shifts[Math.floor(shifts.length/2)], "max", shifts[shifts.length-1]);
// distribution of shifts
const dist = new Map<number, number>();
for (const s of shifts) dist.set(s, (dist.get(s) ?? 0) + 1);
console.log("distinct shifts", dist.size, [...dist.entries()].slice(0, 20));

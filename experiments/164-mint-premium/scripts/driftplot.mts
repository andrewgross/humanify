import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const regions = er.regions as { statements: number[]; raw_spans: [number, number][] }[];
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const rawOrd = new Map<string, number>();
rbody.forEach((s, i) => rawOrd.set(`${s.start}:${s.end}`, i));
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const sast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (sast.program.body[0] as any).expression.body.body;
const litIdx = new Map<string, number[]>();
sbody.forEach((s, i) => {
  const t = shipped.slice(s.start, s.end);
  for (const m of t.matchAll(/"([^"\n]{12,80})"/g)) {
    const a = litIdx.get(m[1]) ?? []; a.push(i); litIdx.set(m[1], a);
  }
});
const pts: [number, number, number][] = []; // rawOrd, dumpedStmt, trueStmt
for (const r of regions) {
  const [s] = r.raw_spans[0];
  const ro = rawOrd.get(`${r.raw_spans[0][0]}:${r.raw_spans[0][1]}`);
  if (ro === undefined) continue;
  const t = raw.slice(s, Math.min(s + 4000, r.raw_spans[0][1]));
  let hit = -1;
  for (const m of t.matchAll(/"([^"\n]{12,80})"/g)) {
    const cands = (litIdx.get(m[1]) ?? []).filter((i) => i <= 35902);
    if (cands.length === 1) { hit = cands[0]; break; }
  }
  if (hit < 0) continue;
  pts.push([ro, r.statements[0], hit]);
}
pts.sort((a, b) => a[0] - b[0]);
console.log("pinned", pts.length);
// drift = true - dumped. plot drift vs rawOrd at intervals
const N = pts.length;
for (let k = 0; k < N; k += Math.max(1, Math.floor(N / 40))) {
  const [ro, du, tr] = pts[k];
  console.log(`rawOrd ${String(ro).padStart(6)} dumped ${String(du).padStart(6)} true ${String(tr).padStart(6)} drift ${tr - du}`);
}

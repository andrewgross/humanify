import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const sast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const sbody: any[] = (sast.program.body[0] as any).expression.body.body;
function skeleton(src: string, node: any): string {
  let t = src.slice(node.start, node.end);
  // strip comments (shouldn't be any in minified), normalize booleans
  t = t.replace(/"((?:\\.|[^"\\])*)"/g, "'S'").replace(/'((?:\\.|[^'\\])*)'/g, "'S'");
  t = t.replace(/`(?:\\.|[^`\\])*`/g, "'T'");
  t = t.replace(/!0/g, "true").replace(/!1/g, "false");
  t = t.replace(/;}/g, "}").replace(/\b0x[0-9a-fA-F]+\b/g, "N").replace(/\b\d+(\.\d+)?\b/g, "N");
  t = t.replace(/[A-Za-z_$][A-Za-z0-9_$]*/g, "I");
  t = t.replace(/\s+/g, "");
  return t;
}
const rkeys = rbody.map((s) => skeleton(raw, s));
const skeys = sbody.map((s) => skeleton(shipped, s));
const rmap = new Map<string, number[]>();
rkeys.forEach((k, i) => { const a = rmap.get(k) ?? []; a.push(i); rmap.set(k, a); });
// greedy alignment: for each shipped statement, find raw candidate; use a monotone matching
let i = 0, j = 0, matched = 0;
const pairs: [number, number][] = [];
while (i < rbody.length && j < sbody.length) {
  if (rkeys[i] === skeys[j]) { pairs.push([i, j]); matched++; i++; j++; continue; }
  // look ahead in shipped for a raw match (insertions)
  let k = 1;
  for (; k <= 25 && j + k < sbody.length; k++) if (rkeys[i] === skeys[j + k]) break;
  let d = 1;
  for (; d <= 25 && i + d < rbody.length; d++) if (rkeys[i + d] === skeys[j]) break;
  if (k <= 25 && (d > 25 || k <= d)) { j += k; continue; }   // insertion(s) in shipped
  if (d <= 25) { i += d; continue; }                          // deletion in raw
  i++; j++; // give up on this pair
}
console.log(`matched ${matched}, raw leftover ${rbody.length - i >= 0 ? rbody.length - matched : 0}, shipped leftover ${sbody.length - matched}`);
// deletions and insertions counts
const rawUsed = new Set(pairs.map((p) => p[0]));
const shpUsed = new Set(pairs.map((p) => p[1]));
let del = 0, ins = 0;
for (let x = 0; x < rbody.length; x++) if (!rawUsed.has(x)) del++;
for (let y = 0; y < sbody.length; y++) if (!shpUsed.has(y)) ins++;
console.log(`unmatched raw (deletions) ${del}, unmatched shipped (insertions) ${ins}`);
// insertions by shipped position: histogram
const insList: number[] = [];
for (let y = 0; y < sbody.length; y++) if (!shpUsed.has(y)) insList.push(y);
// contiguous runs
const runs: [number, number][] = [];
for (const y of insList) {
  const last = runs[runs.length - 1];
  if (last && y === last[1] + 1) last[1] = y; else runs.push([y, y]);
}
console.log("insertion runs:", runs.length, "total", insList.length);
for (const [a, b] of runs.slice(0, 20)) {
  console.log(`  shipped ${a}..${b}: ${shipped.slice(sbody[a].start, sbody[a].start + 80).replace(/\n/g, " ")}`);
}
fs.writeFileSync("/tmp/exp164-align-insertions.json", JSON.stringify(insList));

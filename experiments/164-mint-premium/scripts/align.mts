import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const sast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const sbody: any[] = (sast.program.body[0] as any).expression.body.body;
function key(src: string, node: any): string {
  const t = src.slice(node.start, node.end);
  const lits = [...t.matchAll(/"((?:\\.|[^"\\])*)"/g)].map((m) => m[1]).filter((s) => s.length > 3).sort();
  return `${node.type}|${lits.join("¦")}|${Math.round(t.length / 50)}`;
}
const rkeys = rbody.map((s) => key(raw, s));
const skeys = sbody.map((s) => key(shipped, s));
// two-pointer alignment with lookahead
let i = 0, j = 0;
let matched = 0, ins = 0, del = 0, breakAt: number[] = [];
const insPositions: { j: number; text: string }[] = [];
while (i < rbody.length && j < sbody.length) {
  if (rkeys[i] === skeys[j]) { matched++; i++; j++; continue; }
  // lookahead: insertion in shipped (skip up to 12)
  let found = -1;
  for (let k = 1; k <= 12 && j + k < sbody.length; k++) if (rkeys[i] === skeys[j + k]) { found = k; break; }
  if (found > 0) {
    for (let k = 0; k < found; k++) insPositions.push({ j: j + k, text: shipped.slice(sbody[j + k].start, sbody[j + k].start + 90).replace(/\n/g, " ") });
    ins += found; j += found; breakAt.push(j); continue;
  }
  // deletion in raw (skip up to 12)
  let dfound = -1;
  for (let k = 1; k <= 12 && i + k < rbody.length; k++) if (rkeys[i + k] === skeys[j]) { dfound = k; break; }
  if (dfound > 0) { del += dfound; i += dfound; breakAt.push(j); continue; }
  // mismatch: skip one on each side (rewrite zone)
  breakAt.push(j);
  i++; j++;
}
console.log(`raw ${rbody.length}, shipped ${sbody.length}, matched ${matched}, shipped-only (insertions) ${ins}, raw-only (deletions) ${del}, unmatched pairs ${rbody.length + sbody.length - 2 * matched - ins - del}`);
console.log("insertion zones:", breakAt.length);
for (const p of insPositions.slice(0, 15)) console.log(`  +shipped ${p.j}: ${p.text}`);
fs.writeFileSync("/tmp/exp164-insertions.json", JSON.stringify(insPositions));

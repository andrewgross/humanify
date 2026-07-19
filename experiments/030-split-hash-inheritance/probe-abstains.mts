import fs from "node:fs";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { statementHash } from "../../src/split/statement-hash.js";
const V = "/Users/andrewgross/Development/unpacked-claude-code/versions";
function body(code: string) {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse");
  const w = findWrapperFunction(ast);
  if (!w) throw new Error("wrapper");
  return (w.functionPath.node.body as any).body;
}
const l85 = JSON.parse(fs.readFileSync(`${V}/claude-code-2.1.85/.humanify/split-ledger.json`, "utf8"));
const b85 = body(fs.readFileSync(`${V}/claude-code-2.1.85/.humanify/humanified.js`, "utf8"));
const h85: string[] = b85.map(statementHash);
const pf = new Map<string, string[]>();
h85.forEach((h, i) => { const l = pf.get(h) ?? []; l.push(l85.order[i]); pf.set(h, l); });
const b86 = body(fs.readFileSync(`${V}/claude-code-2.1.86/.humanify/humanified.js`, "utf8"));
const h86: string[] = b86.map(statementHash);
const cc = new Map<string, number>();
for (const h of h86) cc.set(h, (cc.get(h) ?? 0) + 1);
let hit = 0, miss = 0, unequalUnanimous = 0, unequalUnanimousBig = 0, nonUnanimous = 0;
let missBig = 0, missBigBytes = 0;
const bigSizes: number[] = [];
for (let i = 0; i < b86.length; i++) {
  const h = h86[i];
  const files = pf.get(h);
  const sz = (b86[i].end ?? 0) - (b86[i].start ?? 0);
  if (!files) { miss++; if (sz >= 200) { missBig++; missBigBytes += sz; } continue; }
  const allSame = files.every((f) => f === files[0]);
  if (files.length !== cc.get(h)) {
    if (allSame) { unequalUnanimous++; if (sz >= 200) { unequalUnanimousBig++; bigSizes.push(sz); } }
    continue;
  }
  if (!allSame) { nonUnanimous++; continue; }
  hit++;
}
console.log(JSON.stringify({ hit, miss_noPriorHash: miss, missBig, missBigKB: Math.round(missBigBytes / 1024), unequalUnanimous, unequalUnanimousBig, nonUnanimous }, null, 1));
console.log("big unequal sizes:", bigSizes.sort((a, b) => b - a).slice(0, 10));

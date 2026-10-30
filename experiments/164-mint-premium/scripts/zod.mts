import * as fs from "node:fs";
import * as parser from "@babel/parser";
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const ledger = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));
const order: string[] = ledger.order;
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const sbody: any[] = (ast.program.body[0] as any).expression.body.body;
for (const i of [1126, 1127, 1128, 1129, 1130]) {
  const s = sbody[i];
  console.log(`shipped ${i} -> ${order[i]}`);
  console.log("   ", shipped.slice(s.start, Math.min(s.start + 130, s.end)).replace(/\n/g, " "));
}
// where does zod's module-b9b02094 file live: which ordinals map to it
const stmts: number[] = [];
order.forEach((f, i) => { if (f === "vendor/zod/module-b9b02094.js") stmts.push(i); });
console.log("ordinals -> vendor/zod/module-b9b02094.js:", stmts.length, "range", stmts[0], "..", stmts[stmts.length - 1], "contiguous:", stmts[stmts.length-1] - stmts[0] + 1 === stmts.length);
// what is the statement just before the range?
const pre = stmts[0] - 1;
const ps = sbody[pre];
console.log(`stmt ${pre} -> ${order[pre]}:`, shipped.slice(ps.start, Math.min(ps.start + 150, ps.end)).replace(/\n/g, " "));

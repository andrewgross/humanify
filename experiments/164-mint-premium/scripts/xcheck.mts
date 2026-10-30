import * as fs from "node:fs";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const ledger = JSON.parse(fs.readFileSync(`${ROOT}/.humanify/split-ledger.json`, "utf8"));
const order: string[] = ledger.order;
const junk: { name: string; stmt: number; file: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"));
// does the row's file agree with order[row.stmt]?
let agree = 0, disagree = 0;
const samples: string[] = [];
for (const j of junk) {
  const l = order[j.stmt] ?? "?";
  if (l === j.file) agree++;
  else { disagree++; if (samples.length < 10) samples.push(`${j.name} stmt ${j.stmt} row-file ${j.file} ledger ${l}`); }
}
console.log(`junk rows: file agrees with ledger ${agree}, disagrees ${disagree}`);
for (const s of samples) console.log("  ", s);
// count order entries starting vendor/
let v = 0;
for (const f of order) if (f.startsWith("vendor/")) v++;
console.log("order entries with vendor/:", v);
// zod module file statements in ledger
const z = order.map((f, i) => (f === "vendor/zod/module-b9b02094.js" ? i : -1)).filter((i) => i >= 0);
console.log("vendor/zod/module-b9b02094.js ordinals:", z);

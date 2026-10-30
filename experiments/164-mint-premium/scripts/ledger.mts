import * as fs from "node:fs";
const ledger = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));
console.log("ledger keys:", Object.keys(ledger));
const order: string[] = ledger.order;
const files: string[] = ledger.files;
console.log("files:", files.length, "order entries:", order.length);
const vendorFiles = files.filter((f) => f.startsWith("vendor/"));
console.log("vendor files:", vendorFiles.length, "src files:", files.length - vendorFiles.length);
// statements per file
const counts = new Map<string, number>();
for (const f of order) counts.set(f, (counts.get(f) ?? 0) + 1);
const vendorStmts = [...counts.entries()].filter(([f]) => f.startsWith("vendor/")).reduce((a, [, c]) => a + c, 0);
console.log("statements in vendor files:", vendorStmts, "of", order.length);
// any extra keys on the ledger (reasons?)
for (const k of Object.keys(ledger)) {
  const v = (ledger as any)[k];
  if (Array.isArray(v)) continue;
  console.log(k, JSON.stringify(v).slice(0, 300));
}

import * as fs from "node:fs";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const ledger = JSON.parse(fs.readFileSync(`${ROOT}/.humanify/split-ledger.json`, "utf8"));
const order: string[] = ledger.order;
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const frozen = er.frozen as [number, number, string][];
const fstmts = [...new Set(frozen.map((f) => f[0]))];
const files = new Map<string, number>();
for (const s of fstmts) { const f = order[s] ?? "?"; files.set(f, (files.get(f) ?? 0) + 1); }
console.log("frozen statements' ledger files:");
for (const [f, c] of [...files.entries()].sort((a, b) => b[1] - a[1])) console.log(`  ${c}\t${f}`);
// frozen statements that are in vendor files:
const inVendor = fstmts.filter((s) => (order[s] ?? "").startsWith("vendor/"));
console.log("frozen statements in vendor files:", inVendor.length, "of", fstmts.length);
// vendor statements NOT frozen
const vendorStmts: number[] = [];
order.forEach((f, i) => { if (f.startsWith("vendor/")) vendorStmts.push(i); });
const fset = new Set(fstmts);
console.log("vendor statements:", vendorStmts.length, "of which frozen:", vendorStmts.filter((s) => fset.has(s)).length);

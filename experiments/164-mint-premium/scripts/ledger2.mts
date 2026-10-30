import * as fs from "node:fs";
for (const ROOT of ["/tmp/eval-work/exp161-names-hop/2.1.216", "/work/exp164-mint-census/dump-run/tree"]) {
  const ledger = JSON.parse(fs.readFileSync(`${ROOT}/.humanify/split-ledger.json`, "utf8"));
  const order: string[] = ledger.order;
  const files: string[] = ledger.files;
  const vendorFiles = files.filter((f) => f.startsWith("vendor/"));
  const counts = new Map<string, number>();
  for (const f of order) counts.set(f, (counts.get(f) ?? 0) + 1);
  const vendorStmts = [...counts.entries()].filter(([f]) => f.startsWith("vendor/")).reduce((a, [, c]) => a + c, 0);
  console.log(ROOT);
  console.log(`  files ${files.length}, vendor files ${vendorFiles.length}, order ${order.length}, vendor statements ${vendorStmts}`);
  // fossil modules key
  const fm = (ledger as any).fossilModules;
  console.log("  fossilModules:", JSON.stringify(fm).slice(0, 200));
}

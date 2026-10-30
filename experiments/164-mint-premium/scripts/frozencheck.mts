import * as fs from "node:fs";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const frozen = er.frozen as [number, number, string][];
console.log("frozen rows:", frozen.length);
const frozenByStmt = new Map<number, Set<string>>();
for (const [s, , n] of frozen) { const a = frozenByStmt.get(s) ?? new Set(); a.add(n); frozenByStmt.set(s, a); }
const stmts = new Set(frozen.map((f) => f[0]));
console.log("frozen distinct statements:", stmts.size, "min", Math.min(...stmts), "max", Math.max(...stmts));
const junk: { name: string; stmt: number; file: string; cls: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-classified-exp161-names-hop.json", "utf8"));
let both = 0, stmtOnly = 0, neither = 0;
const vendorJunk = junk.filter((j) => j.file.startsWith("vendor/"));
const srcJunk = junk.filter((j) => !j.file.startsWith("vendor/"));
console.log("junk in vendor files:", vendorJunk.length, "src:", srcJunk.length);
for (const j of vendorJunk) {
  if (frozenByStmt.get(j.stmt)?.has(j.name)) both++;
  else if (frozenByStmt.has(j.stmt)) stmtOnly++;
  else neither++;
}
console.log(`vendor junk: name+stmt frozen ${both}, stmt-only ${stmtOnly}, neither ${neither}`);
let sboth = 0, sstmtOnly = 0, sneither = 0;
for (const j of srcJunk) {
  if (frozenByStmt.get(j.stmt)?.has(j.name)) sboth++;
  else if (frozenByStmt.has(j.stmt)) sstmtOnly++;
  else sneither++;
}
console.log(`src junk: name+stmt frozen ${sboth}, stmt-only ${sstmtOnly}, neither ${sneither}`);

import * as fs from "node:fs";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const frozen = er.frozen as [number, number, string][];
// frozen rows at statements 1120-1135
console.log("frozen rows at stmts 1100-1140:", frozen.filter((f) => f[0] >= 1100 && f[0] <= 1140).length);
// frozen by name: do the junk names appear anywhere?
const junk: { name: string; stmt: number; file: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"));
const frozenNames = new Map<string, number[]>();
frozen.forEach(([s, , n]) => { const a = frozenNames.get(n) ?? []; a.push(s); frozenNames.set(n, a); });
let byName = 0;
for (const j of junk) if (frozenNames.has(j.name)) byName++;
console.log("junk rows whose name appears in frozen (any stmt):", byName, "of", junk.length);
// the zod junk names specifically
const zod = junk.filter((j) => j.file.includes("zod"));
console.log("zod junk rows:", zod.length, "names in frozen:", zod.filter((j) => frozenNames.has(j.name)).length);
console.log("zod names:", [...new Set(zod.map((j) => j.name))].slice(0, 30).join(","));
// what ARE the frozen names at 1128?
console.log("frozen at 1128:", frozen.filter((f) => f[0] === 1128).map((f) => f[2]).slice(0, 20));
// sites at 1127/1128/1129?
const sites = new Map((er.sites as any[]).map((s) => [s.statement, s]));
for (const i of [1127, 1128, 1129]) {
  const site = sites.get(i);
  console.log(`site ${i}:`, site ? `prints ${site.prints.length} captured ${site.prints.filter((p: any) => p.captured).length}` : "none");
}

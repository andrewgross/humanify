import * as fs from "node:fs";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const ld = realDiag.libraryDiscovery;
const claims = ld.claims as { start: number; end: number; package: string; rule: string; round: number }[];
const restorations = ld.restorations as { hash: string; package: string }[];
const apps = ld.restorationApplications as { hash: string; package: string; start: number; end: number }[];
const junk: { name: string; stmt: number; file: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"));
const pkgOf = (f: string) => { const p = f.split("/"); return p[0] === "vendor" ? (p[1].startsWith("@") ? `${p[1]}/${p[2]}` : p[1]) : "src"; };
// claims by rule
const byRule = new Map<string, number>();
for (const c of claims) byRule.set(c.rule, (byRule.get(c.rule) ?? 0) + 1);
console.log("claims by rule:", [...byRule.entries()].sort((a, b) => b[1] - a[1]));
// packages: claims by rule, restorations, apps, junk
const pkgs = new Set<string>([...claims.map((c) => c.package), ...junk.map((j) => pkgOf(j.file))]);
const rows: [string, Map<string, number>, number, number, number][] = [];
for (const p of pkgs) {
  const cr = new Map<string, number>();
  for (const c of claims) if (c.package === p) cr.set(c.rule, (cr.get(c.rule) ?? 0) + 1);
  const r = restorations.filter((x) => x.package === p).length;
  const a = apps.filter((x) => x.package === p).length;
  const j = junk.filter((x) => pkgOf(x.file) === p).length;
  if (r + a + j > 0) rows.push([p, cr, r, a, j]);
}
rows.sort((x, y) => y[4] - x[4]);
console.log("\npackage | claims by rule | restorations | applications | junk");
for (const [p, cr, r, a, j] of rows.slice(0, 25)) {
  const crs = [...cr.entries()].sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k}:${v}`).join(" ");
  console.log(`${p} | ${crs} | ${r} | ${a} | ${j}`);
}
console.log("\ntotal junk", junk.length, "restorations", restorations.length, "applications", apps.length);

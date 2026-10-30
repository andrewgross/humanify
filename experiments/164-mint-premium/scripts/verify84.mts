import * as fs from "node:fs";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const er = dumpDiag.eraRestoration;
const sites = er.sites as { statement: number; prints: { hash: string; captured: boolean }[] }[];
const realRest = new Set((realDiag.libraryDiscovery.restorations as { hash: string }[]).map((r) => r.hash));
let sitesCount = sites.length, captured = 0, capturedInReal = 0, maxStmt = 0;
const stmts = new Set<number>();
for (const s of sites) {
  stmts.add(s.statement);
  maxStmt = Math.max(maxStmt, s.statement);
  for (const p of s.prints) if (p.captured) { captured++; if (realRest.has(p.hash)) capturedInReal++; }
}
console.log(`sites ${sitesCount}, distinct statements ${stmts.size}, max statement ${maxStmt}`);
console.log(`captured prints ${captured}, of which in real restorations ${capturedInReal}`);
// region statements max
const regions = er.regions as { statements: number[] }[];
let regionMax = 0, regionStmts = 0;
for (const r of regions) for (const s of r.statements) { regionMax = Math.max(regionMax, s); regionStmts++; }
console.log(`regions ${regions.length}, region statements ${regionStmts}, max region statement ${regionMax}`);
// junk rows' stmt range
const junk: { stmt: number; file: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"));
let jmax = 0, jmin = 1e9;
for (const j of junk) { jmax = Math.max(jmax, j.stmt); jmin = Math.min(jmin, j.stmt); }
console.log(`junk rows ${junk.length}, stmt range ${jmin}..${jmax}`);
// how many junk stmts are in a region / site
const regionSet = new Set(regions.flatMap((r) => r.statements));
const siteSet = stmts;
let inR = 0, inS = 0;
for (const j of junk) { if (regionSet.has(j.stmt)) inR++; if (siteSet.has(j.stmt)) inS++; }
console.log(`junk stmts in a region ${inR}, in a site ${inS}`);

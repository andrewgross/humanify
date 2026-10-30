import * as fs from "node:fs";
import * as readline from "node:readline";
const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
console.log("indexSlice:", dumpDiag.libraryDiscovery.indexSlice, "reason:", dumpDiag.libraryDiscovery.indexSliceReason);
const slice = dumpDiag.libraryDiscovery.indexSlice as string;
const path = `/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28/names.${slice}.jsonl`;
console.log("names file:", path, fs.existsSync(path));
// load the names table: hash -> rows
const byHash = new Map<string, unknown[]>();
const rl = readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity });
let n = 0;
for await (const line of rl) {
  if (!line.trim()) continue;
  const row = JSON.parse(line);
  const h = row.hash ?? row.print ?? row.printHash;
  if (!h) { if (n < 2) console.log("row keys:", Object.keys(row), JSON.stringify(row).slice(0, 300)); }
  else { const a = byHash.get(h) ?? []; a.push(row); byHash.set(h, a); }
  n++;
}
console.log("names rows", n, "distinct hashes", byHash.size);
fs.writeFileSync("/tmp/exp164-names-byhash.json", JSON.stringify([...byHash.entries()].slice(0, 3)));
// the 351 H2-capture sites' hashes
const classified: { name: string; stmt: number; file: string; cls: string }[] = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-classified-exp161-names-hop.json", "utf8"));
const er = dumpDiag.eraRestoration;
const sites = new Map((er.sites as any[]).map((s) => [s.statement, s]));
const h2 = classified.filter((r) => r.cls === "H2-capture");
console.log("H2 rows:", h2.length);
let inIndex = 0, notIn = 0;
const inIdx: string[] = [];
for (const r of h2) {
  const site = sites.get(r.stmt);
  if (!site) continue;
  const hashes = site.prints.map((p: any) => p.hash);
  const found = hashes.filter((h: string) => byHash.has(h));
  if (found.length) { inIndex++; inIdx.push(`${r.name} @ ${r.file} stmt ${r.stmt} hashes ${hashes.slice(0,3).join(",")} FOUND ${found.length}`); }
  else notIn++;
}
console.log(`H2 sites with a hash IN the names table: ${inIndex}, not in: ${notIn}`);
for (const s of inIdx.slice(0, 10)) console.log("  IN:", s);

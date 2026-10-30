import * as fs from "node:fs";
import * as readline from "node:readline";

const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const er = dump.eraRestoration;
const eraHashes = new Set<string>();
for (const s of er.sites as any[]) for (const p of s.prints) eraHashes.add(p.hash);
const restRows = realDiag.libraryDiscovery.restorations as any[];
const captured = new Set(restRows.map((r) => r.hash ?? r.print ?? r.printHash));
const apps = realDiag.libraryDiscovery.restorationApplications as any[];
const appHashes = new Set(apps.map((a) => a.hash));

const IDX = "/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28";
for (const slice of ["bun-bun-cjs-min", "bun-node-cjs-min"]) {
  const inTable = new Set<string>();
  const rl = readline.createInterface({ input: fs.createReadStream(`${IDX}/names.${slice}.jsonl`), crlfDelay: Infinity });
  for await (const line of rl) {
    const h = line.slice(9, 25);
    if (eraHashes.has(h) || captured.has(h)) inTable.add(h);
  }
  let cap = 0, era = 0;
  for (const h of captured) if (inTable.has(h)) cap++;
  for (const h of eraHashes) if (inTable.has(h)) era++;
  console.log(`${slice}: era hashes present ${era}/${eraHashes.size}, captured hashes present ${cap}/${captured.size}`);
}
// the units tables: which slice holds the captured hashes as unit rows?
const appInEra = [...appHashes].filter(h => eraHashes.has(h)).length;
console.log(`application hashes ${appHashes.size}, also walked by Site A: ${appInEra}`);

import * as fs from "node:fs";
import * as readline from "node:readline";

const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));

// ALL Site A print hashes (the era's walked prints)
const eraHashes = new Set<string>();
for (const s of er.sites as any[]) for (const p of s.prints) eraHashes.add(p.hash);
// the captured map's hashes (the route's raw-statement prints)
const restRows = realDiag.libraryDiscovery.restorations as any[];
const captured = new Set(restRows.map((r) => r.hash ?? r.print ?? r.printHash));

const slice = dump.libraryDiscovery.indexSlice as string;
const path = `/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28/names.${slice}.jsonl`;
const inTable = new Set<string>();
const rl = readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity });
for await (const line of rl) {
  const h = line.slice(9, 25);
  if (eraHashes.has(h)) inTable.add(h);
  // captured is small; check membership cheaply too
}
let capIn = 0;
for (const h of captured) if (inTable.has(h)) capIn++;
console.log(`Site A print hashes: ${eraHashes.size}; in the index names table: ${inTable.size}`);
console.log(`captured hashes: ${captured.size}; of them in the names table: ${capIn}`);
// how many of the captured hashes are among the era hashes at all?
let capInEra = 0;
for (const h of captured) if (eraHashes.has(h)) capInEra++;
console.log(`captured hashes also walked by Site A: ${capInEra}`);

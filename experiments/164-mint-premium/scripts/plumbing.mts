import * as fs from "node:fs";
import * as readline from "node:readline";

const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const classified: { name: string; stmt: number; file: string; cls: string }[] = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/dump-classified-exp161-names-hop.json", "utf8"),
);
const er = dumpDiag.eraRestoration;
const sites = new Map((er.sites as any[]).map((s) => [s.statement, s]));

// pass 1: the hashes the H2-capture sites walked
const h2 = classified.filter((r) => r.cls === "H2-capture");
const wantHashes = new Set<string>();
for (const r of h2) {
  const site = sites.get(r.stmt);
  if (!site) continue;
  for (const p of site.prints) wantHashes.add(p.hash);
}
console.log(`H2-capture rows ${h2.length}, distinct site hashes ${wantHashes.size}`);

// pass 2: scan the names table for exactly those hashes
const slice = dumpDiag.libraryDiscovery.indexSlice as string;
const path = `/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28/names.${slice}.jsonl`;
const byHash = new Map<string, any[]>();
const rl = readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity });
let n = 0;
for await (const line of rl) {
  if (!line.trim()) continue;
  n++;
  const idx = line.indexOf('"');
  // cheap pre-filter on the raw line before parsing
  let hit = false;
  for (const h of wantHashes) if (line.includes(h)) { hit = true; break; }
  if (!hit) continue;
  const row = JSON.parse(line);
  const h = row.hash ?? row.print ?? row.printHash;
  if (wantHashes.has(h)) { const a = byHash.get(h) ?? []; a.push(row); byHash.set(h, a); }
}
console.log(`names rows scanned ${n}; wanted hashes found ${byHash.size} of ${wantHashes.size}`);
if (byHash.size) console.log("sample row:", JSON.stringify([...byHash.values()][0][0]).slice(0, 400));

// does the run's captured restoration table hold any of them?
const restRows = realDiag.libraryDiscovery.restorations as any[];
const restHashes = new Set(restRows.map((r) => r.hash ?? r.print ?? r.printHash));
let inRest = 0;
for (const h of byHash.keys()) if (restHashes.has(h)) inRest++;
console.log(`of the found hashes, present in the run's captured restoration rows: ${inRest}`);

// per junk row: does ANY of its site's prints' index answers name the junk slot?
let answered = 0, hashNoSlot = 0, noAnswer = 0;
const answeredEx: string[] = [];
for (const r of h2) {
  const site = sites.get(r.stmt);
  if (!site) continue;
  let ok = false, sawRow = false;
  for (const p of site.prints) {
    const rows = byHash.get(p.hash);
    if (!rows?.length) continue;
    sawRow = true;
    const slotIdx = p.slots.findIndex((slot: any) => slot[2] === r.name);
    if (slotIdx < 0) continue;
    for (const row of rows) {
      const answer = row.answer ?? row.names ?? row.restored;
      const list = typeof answer === "string" ? null : answer;
      if (!list) { if (answeredEx.length < 3) answeredEx.push(`unparsed answer for ${p.hash.slice(0,8)}: ${JSON.stringify(row).slice(0,200)}`); continue; }
      for (const ent of list) {
        const s = typeof ent === "number" ? ent : ent.slot;
        const nm = typeof ent === "number" ? null : ent.name;
        if (s === slotIdx) { ok = true; if (answeredEx.length < 6) answeredEx.push(`${r.name} @ ${r.file} stmt ${r.stmt}: slot ${slotIdx} -> ${nm}`); }
      }
    }
  }
  if (ok) answered++;
  else if (sawRow) hashNoSlot++;
  else noAnswer++;
}
console.log(`\nH2-capture junk rows: index answer NAMES the slot ${answered}, hash present but slot unnamed ${hashNoSlot}, no hash in table ${noAnswer}`);
for (const e of answeredEx) console.log("  ", e);
fs.writeFileSync("/tmp/exp164-h2-hashrows.json", JSON.stringify([...byHash.entries()].slice(0, 40)));

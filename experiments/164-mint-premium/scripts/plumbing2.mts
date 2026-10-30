import * as fs from "node:fs";
import * as readline from "node:readline";

const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const classified: { name: string; stmt: number; file: string; cls: string }[] = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/dump-classified-exp161-names-hop.json", "utf8"),
);
const er = dumpDiag.eraRestoration;
const sites = new Map((er.sites as any[]).map((s) => [s.statement, s]));

// pass 1: every junk row's site print hashes
const wantHashes = new Set<string>();
for (const r of classified) {
  const site = sites.get(r.stmt);
  if (!site) continue;
  for (const p of site.prints) wantHashes.add(p.hash);
}
console.log(`junk rows ${classified.length}, distinct site print hashes ${wantHashes.size}`);

// pass 2: scan the provenance jsonl — each line starts {"hash":"<16hex>"
const slice = dumpDiag.libraryDiscovery.indexSlice as string;
const path = `/work/library-index/cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28/names.${slice}.jsonl`;
const byHash = new Map<string, any[]>();
const rl = readline.createInterface({ input: fs.createReadStream(path), crlfDelay: Infinity });
let n = 0;
for await (const line of rl) {
  n++;
  const h = line.slice(9, 25);
  if (!wantHashes.has(h)) continue;
  const a = byHash.get(h) ?? [];
  a.push(JSON.parse(line));
  byHash.set(h, a);
}
console.log(`names rows scanned ${n}; wanted hashes found ${byHash.size} of ${wantHashes.size}`);

// is the hash in the run's captured restoration rows?
const restRows = realDiag.libraryDiscovery.restorations as any[];
const restHashes = new Set(restRows.map((r) => r.hash ?? r.print ?? r.printHash));
let inRest = 0;
for (const h of byHash.keys()) if (restHashes.has(h)) inRest++;
console.log(`found hashes also in the run's captured restoration rows: ${inRest} (the capture missed the rest)`);

// per junk row, per bucket: does an index answer name the junk slot?
const tally = new Map<string, { named: number; hashUnnamed: number; noHash: number; noSite: number }>();
const examples = new Map<string, string[]>();
for (const r of classified) {
  const site = sites.get(r.stmt);
  let cls = r.cls;
  let state: "named" | "hashUnnamed" | "noHash" | "noSite";
  if (!site) state = "noSite";
  else {
    let sawRow = false, ok = false;
    for (const p of site.prints) {
      const rows = byHash.get(p.hash);
      if (!rows?.length) continue;
      sawRow = true;
      const slotIdx = p.slots.findIndex((slot: any) => slot[2] === r.name);
      if (slotIdx < 0) continue;
      for (const row of rows) {
        for (const ent of row.names ?? []) {
          if (ent.slot === slotIdx) {
            ok = true;
            if ((examples.get(cls) ?? []).length < 5)
              examples.set(cls, [...(examples.get(cls) ?? []), `${r.name} @ ${r.file} stmt ${r.stmt}: slot ${slotIdx} -> ${ent.name}`]);
          }
        }
      }
    }
    state = ok ? "named" : sawRow ? "hashUnnamed" : "noHash";
  }
  const t = tally.get(cls) ?? { named: 0, hashUnnamed: 0, noHash: 0, noSite: 0 };
  tally.set(cls, { ...t, [state]: (t as any)[state] + 1 });
}
console.log("\n--- per bucket: does the index's own names table answer the slot? ---");
for (const [c, t] of tally) console.log(`${c}: named ${t.named}, hash-present-slot-unnamed ${t.hashUnnamed}, hash-absent ${t.noHash}, no site ${t.noSite}`);
for (const [c, list] of examples) { console.log(`\n${c} named examples:`); for (const e of list) console.log("  ", e); }
fs.writeFileSync("/tmp/exp164-index-byhash.json", JSON.stringify([...byHash.entries()].slice(0, 60)));

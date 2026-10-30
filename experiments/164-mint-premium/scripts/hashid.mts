import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const shipped = fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/humanified.js", "utf8");
const led = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));

const rawAst = parser.parse(fs.readFileSync(RAW, "utf8"), { sourceType: "unambiguous", errorRecovery: true });
const shipAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rawBody = (rawAst.program.body[0] as any).expression.body.body;
const body = (shipAst.program.body[0] as any).expression.body.body;
const rawBySpan = new Map<string, any>();
rawBody.forEach((s: any) => rawBySpan.set(`${s.start}:${s.end}`, s));
const eraHash = body.map(statementHash);

// the era image of each raw container statement: two-pointer content alignment
// (raw statement -> first era ordinal whose hash matches, monotone)
const eraImageOfRaw = new Map<number, number>();
let e = 0;
for (let r = 0; r < rawBody.length; r++) {
  const rh = statementHash(rawBody[r]);
  let found = -1;
  for (let k = e; k < Math.min(e + 60, eraHash.length); k++) if (eraHash[k] === rh) { found = k; break; }
  if (found >= 0) { eraImageOfRaw.set(r, found); e = found + 1; }
}
console.log(`raw statements with a unique-forward era image: ${eraImageOfRaw.size} of ${rawBody.length}`);

// the applications whose span is a whole group statement's raw span
const apps = realDiag.libraryDiscovery.restorationApplications as any[];
let tested = 0, same = 0;
const samples: string[] = [];
for (const a of apps) {
  const key = `${a.start}:${a.end}`;
  const rawIdx = rawBody.findIndex((s: any) => s.start === a.start && s.end === a.end);
  if (rawIdx < 0) continue;
  const eraIdx = eraImageOfRaw.get(rawIdx);
  if (eraIdx === undefined) continue;
  tested++;
  const lh = led.hashes[eraIdx];
  if (lh === a.hash) { same++; if (samples.length < 5) samples.push(`era ${eraIdx}: app hash ${a.hash} == ledger ${lh}`); }
  else if (samples.length < 8) samples.push(`era ${eraIdx}: app ${a.hash} vs ledger ${lh} MISMATCH`);
}
console.log(`whole-print applications tested ${tested}, hash == ledger.hashes[eraOrdinal]: ${same}`);
for (const s of samples) console.log("  ", s);

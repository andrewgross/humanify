// Pick the smallest capture row per request SHAPE (field set + callee
// presence) across the four pairs, plus every miss, for the committed
// replay fixture. Copies each row's cache entry and TS replay row.
import fs from "node:fs";
import path from "node:path";
const G = "/work/rust-port/gates/wp4.1/capture"; // run from the repo root
const out = "test/parity/wp41-replay";
const pairs = ["2.1.85-2.1.86", "2.1.118-2.1.119", "2.1.197-2.1.198", "2.1.215-2.1.216"];
const best = new Map();
const misses = [];
for (const pair of pairs) {
  const rows = fs.readFileSync(`${G}/${pair}/requests.jsonl`, "utf8").split("\n").filter(Boolean);
  const ts = fs.readFileSync(`${G}/${pair}/ts-replay.jsonl`, "utf8").split("\n").filter(Boolean);
  rows.forEach((line, i) => {
    const row = JSON.parse(line);
    const replay = JSON.parse(ts[i]);
    const shape = `${Object.keys(row.request).sort().join(",")}|callees=${row.request.calleeSignatures.length > 0}|hit=${replay.hit}`;
    const cand = { line, tsLine: ts[i], key: row.cacheKey, hit: replay.hit, pair };
    if (!replay.hit) misses.push(cand);
    const prev = best.get(shape);
    if (!prev || line.length < prev.line.length) best.set(shape, cand);
  });
}
const all = [...best.values()].sort((a, b) => a.line.length - b.line.length);
const picked = all.filter((p) => p.line.length < 40000);
console.log(`dropped ${all.length - picked.length} shapes whose smallest row is >= 40KB`);
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "cache"), { recursive: true });
const reqLines = [];
const tsLines = [];
let seq = 0;
for (const p of picked) {
  const row = JSON.parse(p.line);
  const t = JSON.parse(p.tsLine);
  row.origin = `${p.pair}#${row.seq}`;
  row.seq = seq;
  t.seq = seq;
  seq++;
  reqLines.push(JSON.stringify(row));
  tsLines.push(JSON.stringify(t));
  if (p.hit) {
    const rel = path.join(p.key.slice(0, 2), `${p.key.slice(2)}.json`);
    fs.mkdirSync(path.join(out, "cache", p.key.slice(0, 2)), { recursive: true });
    fs.copyFileSync(path.join("/tmp/wp41/cache", rel), path.join(out, "cache", rel));
  }
}
fs.writeFileSync(path.join(out, "requests.jsonl"), `${reqLines.join("\n")}\n`);
fs.writeFileSync(path.join(out, "ts-replay.jsonl"), `${tsLines.join("\n")}\n`);
const bytes = reqLines.reduce((a, l) => a + l.length, 0);
console.log(`shapes=${picked.length} misses=${picked.filter((p) => !p.hit).length} requestBytes=${bytes}`);

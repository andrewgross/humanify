/**
 * Task 1 — does each of 197→198's removed vendor files have a counterpart in
 * the other release under a DIFFERENT name?
 *
 * What this predicate actually tests (rule 3, stated in one sentence): for each
 * file present on only one side, it ranks every file on the other side by
 * Jaccard overlap of their sets of quoted string literals >= 6 chars, and
 * reports the best candidate and its score.
 *
 * Why string literals: a vendored file here is 1-4 lines of minified text whose
 * identifiers are rerolled by the minifier every build (exp046 §A4), so
 * identifiers are noise. Grammar/library payloads are mostly literals, and a
 * literal set is order- and rename-insensitive. It is a RANKER, not a verdict —
 * every pair it proposes gets read by hand.
 *
 * Deliberately NOT structuralHash: it erases string values (pitfall rule 8).
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { jaccard, literalSet, overlap } from "./literals.js";

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (p.endsWith(".js")) out.push(p);
  }
  return out;
}

const [aRoot, bRoot] = process.argv.slice(2);
if (!aRoot || !bRoot) {
  console.error("usage: match-addremove.ts <priorVendorDir> <freshVendorDir>");
  process.exit(1);
}

type Entry = { rel: string; lits: Set<string>; bytes: number };
function load(root: string): Map<string, Entry> {
  const m = new Map<string, Entry>();
  for (const p of walk(root)) {
    const text = readFileSync(p, "utf8");
    const rel = relative(root, p);
    m.set(rel, { rel, lits: literalSet(text), bytes: text.length });
  }
  return m;
}

const A = load(aRoot);
const B = load(bRoot);
const removed = [...A.keys()].filter((k) => !B.has(k)).sort();
const added = [...B.keys()].filter((k) => !A.has(k)).sort();

console.log(
  `TOTAL  A=${A.size} B=${B.size}  removed=${removed.length} added=${added.length}`
);
console.log("");

// Best candidate for each removed file, searched over ALL of B (not just the
// added set) -- a removed file whose content sits in a file that also existed
// before is a different story than one that landed in a new name.
const rows: {
  rel: string;
  best: string;
  score: number;
  inAdded: boolean;
  aB: number;
  bB: number;
}[] = [];
for (const r of removed) {
  const src = A.get(r)!;
  let best = "";
  let score = -1;
  for (const [k, cand] of B) {
    const s = jaccard(src.lits, cand.lits);
    if (s > score) {
      score = s;
      best = k;
    }
  }
  rows.push({
    rel: r,
    best,
    score,
    inAdded: added.includes(best),
    aB: src.bytes,
    bB: B.get(best)?.bytes ?? 0
  });
}

rows.sort((x, y) => y.score - x.score);
console.log(
  "removed file -> best content match in fresh tree (Jaccard on literals)"
);
console.log("score  addedSide  priorBytes  freshBytes  removed -> best");
for (const r of rows) {
  console.log(
    `${r.score.toFixed(3)}  ${r.inAdded ? "NEW " : "kept"}  ${String(r.aB).padStart(8)}  ${String(r.bB).padStart(8)}  ${r.rel} -> ${r.best}`
  );
}

const buckets = [
  ["1.000 exact literal set", (s: number) => s === 1],
  [">=0.90", (s: number) => s >= 0.9 && s < 1],
  ["0.50-0.90", (s: number) => s >= 0.5 && s < 0.9],
  ["0.10-0.50", (s: number) => s >= 0.1 && s < 0.5],
  ["<0.10 (no counterpart)", (s: number) => s < 0.1]
] as const;
console.log("");
console.log("| bucket | files |");
console.log("| ------ | ----: |");
for (const [label, pred] of buckets) {
  console.log(`| ${label} | ${rows.filter((r) => pred(r.score)).length} |`);
}

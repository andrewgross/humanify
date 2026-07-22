/**
 * Residual-noise decomposition after the statement-twin tier: what is the
 * remaining noise made of? Buckets per remaining noise statement (fresh
 * output vs archive prior): unique-twin (1:1 — the tier's own population;
 * these should be zero if the tier fully captured) vs equal-count vs
 * unequal. For unique-twin residual, print samples with the FIRST
 * differing line so the flip kind is visible.
 *
 *   npx tsx residual-lever1.ts <freshHumanified.js> <priorHumanified.js>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

function counts(list: { hash: string }[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const s of list) m.set(s.hash, (m.get(s.hash) ?? 0) + 1);
  return m;
}

function firstDiffLine(a: string, b: string): string {
  const la = a.split("\n");
  const lb = b.split("\n");
  for (let i = 0; i < Math.min(la.length, lb.length); i++) {
    if (la[i] !== lb[i]) {
      return `L${i}:\n  prior: ${lb[i].trim().slice(0, 130)}\n  fresh: ${la[i].trim().slice(0, 130)}`;
    }
  }
  return "(length differs)";
}

const [freshPath, priorPath] = process.argv.slice(2);
const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));
const fc = counts(fresh);
const pc = counts(prior);
const priorByHash = new Map<string, { text: string }[]>();
for (const s of prior) {
  const list = priorByHash.get(s.hash) ?? [];
  if (list.length === 0) priorByHash.set(s.hash, list);
  list.push(s);
}

let noiseSt = 0;
let noiseLn = 0;
const buckets = {
  unique: { st: 0, ln: 0 },
  equal: { st: 0, ln: 0 },
  unequal: { st: 0, ln: 0 }
};
const samples: string[] = [];
for (const s of fresh) {
  const twins = priorByHash.get(s.hash);
  if (!twins) continue;
  if (twins.some((t) => t.text === s.text)) continue;
  noiseSt++;
  noiseLn += s.lines;
  const f = fc.get(s.hash) ?? 0;
  const p = pc.get(s.hash) ?? 0;
  if (f === 1 && p === 1) {
    buckets.unique.st++;
    buckets.unique.ln += s.lines;
    if (samples.length < 12 && s.lines > 3) {
      samples.push(
        `[${s.lines}ln] head: ${s.text.split("\n", 1)[0].slice(0, 110)}\n${firstDiffLine(s.text, twins[0].text)}`
      );
    }
  } else if (f === p) {
    buckets.equal.st++;
    buckets.equal.ln += s.lines;
  } else {
    buckets.unequal.st++;
    buckets.unequal.ln += s.lines;
  }
}
console.log(`noise: ${noiseSt} st / ${noiseLn} ln`);
console.log(
  `  unique-twin residual: ${buckets.unique.st} st / ${buckets.unique.ln} ln`
);
console.log(
  `  equal-count:          ${buckets.equal.st} st / ${buckets.equal.ln} ln`
);
console.log(
  `  unequal:              ${buckets.unequal.st} st / ${buckets.unequal.ln} ln`
);
console.log(`\n=== unique-twin residual samples (first divergence) ===`);
for (const s of samples) console.log(`\n${s}`);

/**
 * exp074 ceiling: what would a STEM-CORROBORATED path inheritance hold still?
 *
 *   npx tsx experiments/074-path-stability/ceiling.mts
 *
 * Diagnosis (measured): path churn is NOT ordinal/discovery-order noise.
 * Matched modules inherit paths perfectly (2,182 matched, ZERO moved).
 * The churn comes from modules the matcher misses — and the dominant one
 * (`access-property.js`, 3,204 churned require lines) kept the SAME stem
 * across runs and only changed FOLDER, because an unmatched module is
 * placed by folder inference from scratch.
 *
 * Candidate rule: an unmatched fresh module whose stem matches exactly
 * one unmatched prior module's stem, with content overlap ≥ T, inherits
 * that prior path. This prices it and checks precision at several T.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { matchFossilModules } from "../../src/split/fossil-match.js";

interface LedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}
const L = (v: string): LedgerModule[] =>
  JSON.parse(
    fs.readFileSync(
      `/tmp/eval-work/exp070-r1/${v}/.humanify/split-ledger.json`,
      "utf8"
    )
  ).fossilModules;

const prior = L("2.1.85-rebased");
const fresh = L("2.1.86");
const { matches } = matchFossilModules(
  prior.map((m) => ({ hashes: m.hashes, imports: m.imports })),
  fresh.map((m) => ({ hashes: m.hashes, imports: m.imports }))
);
const matchedPrior = new Set(matches.values());
const stemOf = (f: string) => path.basename(f, ".js");

function overlap(a: string[], b: string[]): number {
  const c = new Map<string, number>();
  for (const h of a) c.set(h, (c.get(h) ?? 0) + 1);
  let inter = 0;
  for (const h of b) {
    const n = c.get(h) ?? 0;
    if (n > 0) {
      inter++;
      c.set(h, n - 1);
    }
  }
  return inter / (a.length + b.length - inter);
}

// churned require lines per target path, from the emitted trees
const A = "/tmp/eval-work/exp070-r1/2.1.85-rebased/src";
const B = "/tmp/eval-work/exp070-r1/2.1.86/src";
function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}
const aFiles = new Set(walk(A));
const churnByTarget = new Map<string, number>();
for (const f of walk(B)) {
  if (!aFiles.has(f)) continue;
  const priorLines = new Set(
    fs.readFileSync(path.join(A, f), "utf8").split("\n")
  );
  for (const line of fs.readFileSync(path.join(B, f), "utf8").split("\n")) {
    if (!line.includes("require(") || priorLines.has(line)) continue;
    for (const m of line.matchAll(/require\("([^"]+)"\)/g)) {
      const t = `src/${path.normalize(path.join(path.dirname(f), m[1]))}`;
      churnByTarget.set(t, (churnByTarget.get(t) ?? 0) + 1);
    }
  }
}

// unmatched sides, indexed by stem
const unmatchedFresh = fresh
  .map((m, i) => ({ m, i }))
  .filter(({ i }) => !matches.has(i));
const unmatchedPrior = prior
  .map((m, i) => ({ m, i }))
  .filter(({ i }) => !matchedPrior.has(i));
const priorByStem = new Map<string, { m: LedgerModule; i: number }[]>();
for (const e of unmatchedPrior) {
  const s = stemOf(e.m.file);
  const l = priorByStem.get(s) ?? [];
  l.push(e);
  priorByStem.set(s, l);
}
const freshByStem = new Map<string, number>();
for (const e of unmatchedFresh) {
  const s = stemOf(e.m.file);
  freshByStem.set(s, (freshByStem.get(s) ?? 0) + 1);
}

console.log(
  `fresh ${fresh.length} | matched ${matches.size} | unmatched ${unmatchedFresh.length}`
);
console.log(`prior unmatched ${unmatchedPrior.length}\n`);
console.log(
  `  T     pairs   churned-lines-held   median-overlap  paths-changed`
);
for (const T of [0.0, 0.3, 0.5, 0.7, 0.8]) {
  let pairs = 0;
  let held = 0;
  let changed = 0;
  const ovs: number[] = [];
  for (const e of unmatchedFresh) {
    const s = stemOf(e.m.file);
    if ((freshByStem.get(s) ?? 0) !== 1) continue; // unique on fresh side
    const cands = priorByStem.get(s) ?? [];
    if (cands.length !== 1) continue; // unique on prior side
    const ov = overlap(cands[0].m.hashes, e.m.hashes);
    if (ov < T) continue;
    pairs++;
    ovs.push(ov);
    if (cands[0].m.file !== e.m.file) {
      changed++;
      held += churnByTarget.get(e.m.file) ?? 0;
    }
  }
  ovs.sort((a, b) => a - b);
  const med = ovs[Math.floor(ovs.length / 2)] ?? 0;
  console.log(
    `  ${T.toFixed(1)}  ${String(pairs).padStart(6)}   ${String(held).padStart(18)}   ${med.toFixed(3).padStart(14)}  ${String(changed).padStart(13)}`
  );
}
console.log(
  `\ntotal churned require lines in existing files: ${[...churnByTarget.values()].reduce((s, n) => s + n, 0)}`
);

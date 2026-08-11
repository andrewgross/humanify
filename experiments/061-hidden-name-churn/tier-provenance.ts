/**
 * 061 Task 0 — which tier decided the churned names?
 *
 *   npx tsx experiments/061-hidden-name-churn/tier-provenance.ts <priorSrc> <freshSrc> <freshDiag.json>
 *
 * Joins the paired name-only churn (055's predicate) against the fresh
 * run's strategy trails: for every fresh-side identifier that appears in a
 * name-only line pair, find the trail(s) whose settled newName matches and
 * tally settledBy. A name matched by multiple trails distributes its
 * weight evenly — per-binding loc joining would be exact but the tree's
 * emitted locs are not the trail's pre-split locs, so this is a
 * name-level approximation; treat <5% wobble as noise.
 */
import * as fs from "node:fs";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, DIAG] = process.argv.slice(2);
if (!PRIOR || !FRESH || !DIAG) {
  console.error(
    "usage: tier-provenance.ts <priorSrc> <freshSrc> <freshDiag.json>"
  );
  process.exit(1);
}

function pairsIn(a: string, b: string): [string, string][] {
  const pa = a.split("\n");
  const pb = b.split("\n");
  const sa = new Set(pa);
  const sb = new Set(pb);
  const rem = pa.filter((l) => !sb.has(l));
  const add = pb.filter((l) => !sa.has(l));
  const k = Math.min(rem.length, add.length);
  const out: [string, string][] = [];
  for (let i = 0; i < k; i++) out.push([rem[i], add[i]]);
  return out;
}

// 1. Collect fresh-side churned identifiers with occurrence weights.
const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });
const churned = new Map<string, number>();
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a);
    const tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const freshIds: string[] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text) freshIds.push(tb[i].text);
    }
    if (!ok) continue;
    for (const id of freshIds) churned.set(id, (churned.get(id) ?? 0) + 1);
  }
}

// 2. Index trails by settled name.
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));
const byName = new Map<string, string[]>();
for (const t of diag.strategyTrails.trails) {
  const last = t.trail?.[t.trail.length - 1];
  const settledName = last?.newName;
  if (!settledName) continue;
  const l = byName.get(settledName) ?? [];
  l.push(t.settledBy ?? last.strategy ?? "unknown");
  byName.set(settledName, l);
}

// 3. Join, distributing weight across ambiguous trail matches.
const tierWeight = new Map<string, number>();
let unmatched = 0;
let totalW = 0;
for (const [name, w] of churned) {
  totalW += w;
  const tiers = byName.get(name);
  if (!tiers || tiers.length === 0) {
    unmatched += w;
    continue;
  }
  for (const tier of tiers) {
    tierWeight.set(tier, (tierWeight.get(tier) ?? 0) + w / tiers.length);
  }
}

console.log(
  `churned fresh identifiers: ${churned.size} unique, ${totalW} occurrences`
);
console.log(
  `unmatched in trails: ${unmatched} occurrences (${((100 * unmatched) / totalW).toFixed(1)}%)`
);
console.log("settledBy tier, weighted by occurrences:");
for (const [tier, w] of [...tierWeight.entries()].sort((a, b) => b[1] - a[1])) {
  console.log(
    `  ${tier.padEnd(18)} ${w.toFixed(1).padStart(8)}  ${((100 * w) / totalW).toFixed(1)}%`
  );
}

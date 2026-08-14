/**
 * 070 Task 0 — cross-version fossil-module matcher.
 *
 *   npx tsx experiments/070-fossil-split/init-match.ts \
 *     <priorBundle> <priorLedger> <freshBundle> <freshLedger> [label] [--dump out.json]
 *
 * Matches module inits across versions by WRITE-SET SHAPE (the segment's
 * identifier-blind statementHash multiset) and IMPORT-EDGE CONTEXT
 * (imports/importers mapped through already-made matches) — NEVER by
 * position (a tie that edges cannot break stays unmatched; the i36/Pd8
 * lesson: duplicated modules are distinct fossils distinguished only by
 * who imports them).
 *
 * Tiers:
 *   A  unique-signature: exact hash-multiset key, unique on both sides
 *   B  edge-corroborated (iterated to fixpoint): among candidates with
 *      hash overlap >= 0.5 (Jaccard), accept the unique best by
 *      (matched-edge agreement, then overlap), requiring positive edge
 *      evidence or overlap >= 0.8 with a unique candidate
 *
 * Census of the rest: fresh-new / prior-deleted (no candidate at 0.5),
 * ambiguous twins (identical signatures, edges silent), low-overlap,
 * merged/split (hashes >= 0.8 contained in the union of matched
 * opposite segments).
 */
import * as fs from "node:fs";
import { extractFossils, type FossilModule } from "./fossil-lib.js";

const args = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const dumpIdx = process.argv.indexOf("--dump");
const DUMP = dumpIdx >= 0 ? process.argv[dumpIdx + 1] : null;
const [PB, PL, FB, FL, LABEL = ""] = args;
if (!PB || !PL || !FB || !FL) {
  console.error(
    "usage: init-match.ts <priorBundle> <priorLedger> <freshBundle> <freshLedger> [label] [--dump out.json]"
  );
  process.exit(1);
}

const prior = extractFossils(PB, PL);
const fresh = extractFossils(FB, FL);

function sigKey(m: FossilModule): string {
  return m.hashes.join("|");
}
function overlap(a: FossilModule, b: FossilModule): number {
  const ca = new Map<string, number>();
  for (const h of a.hashes) ca.set(h, (ca.get(h) ?? 0) + 1);
  let inter = 0;
  const cb = new Map<string, number>();
  for (const h of b.hashes) cb.set(h, (cb.get(h) ?? 0) + 1);
  for (const [h, n] of ca) inter += Math.min(n, cb.get(h) ?? 0);
  const union = a.hashes.length + b.hashes.length - inter;
  return union === 0 ? 0 : inter / union;
}

const pMatched = new Map<number, number>(); // prior idx -> fresh idx
const fMatched = new Map<number, number>();
const tierOf = new Map<number, string>(); // fresh idx -> tier

// ── Tier A: unique signature ──────────────────────────────────────────────
{
  const bySigP = new Map<string, number[]>();
  prior.modules.forEach((m, i) => {
    const k = sigKey(m);
    (bySigP.get(k) ?? bySigP.set(k, []).get(k)!).push(i);
  });
  const bySigF = new Map<string, number[]>();
  fresh.modules.forEach((m, i) => {
    const k = sigKey(m);
    (bySigF.get(k) ?? bySigF.set(k, []).get(k)!).push(i);
  });
  for (const [k, ps] of bySigP) {
    const fsIdx = bySigF.get(k);
    if (ps.length === 1 && fsIdx && fsIdx.length === 1) {
      pMatched.set(ps[0], fsIdx[0]);
      fMatched.set(fsIdx[0], ps[0]);
      tierOf.set(fsIdx[0], "unique-signature");
    }
  }
}

// importers index (who imports me), per side
function importersOf(mods: FossilModule[]): Map<number, number[]> {
  const rev = new Map<number, number[]>();
  mods.forEach((m, i) => {
    for (const imp of m.imports) {
      (rev.get(imp) ?? rev.set(imp, []).get(imp)!).push(i);
    }
  });
  return rev;
}
const pImporters = importersOf(prior.modules);
const fImporters = importersOf(fresh.modules);

function edgeAgreement(pi: number, fi: number): number {
  let agree = 0;
  const pm = prior.modules[pi];
  const fmSet = new Set(fresh.modules[fi].imports);
  for (const imp of pm.imports) {
    const mapped = pMatched.get(imp);
    if (mapped !== undefined && fmSet.has(mapped)) agree++;
  }
  const pInc = pImporters.get(pi) ?? [];
  const fIncSet = new Set(fImporters.get(fi) ?? []);
  for (const imp of pInc) {
    const mapped = pMatched.get(imp);
    if (mapped !== undefined && fIncSet.has(mapped)) agree++;
  }
  return agree;
}

// ── Tier B: edge-corroborated, iterate to fixpoint ────────────────────────
let rounds = 0;
for (;;) {
  rounds++;
  let made = 0;
  const unmatchedP = prior.modules
    .map((_, i) => i)
    .filter((i) => !pMatched.has(i));
  const unmatchedF = fresh.modules
    .map((_, i) => i)
    .filter((i) => !fMatched.has(i));
  for (const pi of unmatchedP) {
    interface Cand {
      fi: number;
      ov: number;
      agree: number;
    }
    const cands: Cand[] = [];
    for (const fi of unmatchedF) {
      if (fMatched.has(fi)) continue;
      const ov = overlap(prior.modules[pi], fresh.modules[fi]);
      if (ov >= 0.5) cands.push({ fi, ov, agree: edgeAgreement(pi, fi) });
    }
    if (cands.length === 0) continue;
    cands.sort((a, b) => b.agree - a.agree || b.ov - a.ov);
    const best = cands[0];
    const second = cands[1];
    const uniqueBest =
      !second || best.agree > second.agree || best.ov > second.ov + 1e-9;
    const licensed =
      (best.agree >= 1 && uniqueBest) ||
      (cands.length === 1 && best.ov >= 0.8);
    if (licensed) {
      pMatched.set(pi, best.fi);
      fMatched.set(best.fi, pi);
      tierOf.set(best.fi, best.agree >= 1 ? "edge-corroborated" : "high-overlap-unique");
      made++;
    }
  }
  if (made === 0) break;
}

// ── census of the unmatched ───────────────────────────────────────────────
function census(side: "prior" | "fresh") {
  const mods = side === "prior" ? prior.modules : fresh.modules;
  const other = side === "prior" ? fresh.modules : prior.modules;
  const matched = side === "prior" ? pMatched : fMatched;
  const otherMatched = side === "prior" ? fMatched : pMatched;
  const bySigOther = new Map<string, number>();
  other.forEach((m) => {
    const k = sigKey(m);
    bySigOther.set(k, (bySigOther.get(k) ?? 0) + 1);
  });
  const out = { twins: 0, lowOverlap: 0, none: 0, mergedSplit: 0 };
  for (let i = 0; i < mods.length; i++) {
    if (matched.has(i)) continue;
    const m = mods[i];
    if ((bySigOther.get(sigKey(m)) ?? 0) > 0) {
      out.twins++;
      continue;
    }
    let bestOv = 0;
    for (let j = 0; j < other.length; j++) {
      const ov = overlap(m, other[j]);
      if (ov > bestOv) bestOv = ov;
    }
    if (bestOv >= 0.5) {
      out.lowOverlap++; // candidate existed but edges couldn't license it
      continue;
    }
    // merged/split: hashes largely contained in MATCHED opposite segments
    const otherHashes = new Set<string>();
    for (const [oi] of side === "prior"
      ? Array.from(otherMatched.entries())
      : Array.from(otherMatched.entries())) {
      for (const h of other[oi]?.hashes ?? []) otherHashes.add(h);
    }
    const contained = m.hashes.filter((h) => otherHashes.has(h)).length;
    if (m.hashes.length > 0 && contained / m.hashes.length >= 0.8)
      out.mergedSplit++;
    else out.none++;
  }
  return out;
}

const tierCounts = new Map<string, number>();
for (const [, t] of tierOf) tierCounts.set(t, (tierCounts.get(t) ?? 0) + 1);
const pc = census("prior");
const fc = census("fresh");

const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%` : "n/a";
console.log(`=== 070 init match — ${LABEL} ===`);
console.log(
  `  prior modules ${prior.modules.length} (eager ${prior.unattributed}), fresh ${fresh.modules.length} (eager ${fresh.unattributed})`
);
console.log(
  `  matched ${pMatched.size}  ${pct(pMatched.size, prior.modules.length)} of prior, ${pct(pMatched.size, fresh.modules.length)} of fresh  (rounds ${rounds})`
);
for (const [t, n] of [...tierCounts.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`    ${t.padEnd(20)} ${n}`);
console.log(
  `  prior unmatched: twins ${pc.twins}, low-overlap ${pc.lowOverlap}, merged/split ${pc.mergedSplit}, none ${pc.none}`
);
console.log(
  `  fresh unmatched: twins ${fc.twins}, low-overlap ${fc.lowOverlap}, merged/split ${fc.mergedSplit}, none ${fc.none}`
);
console.log(
  `ROW|${LABEL}|${prior.modules.length}|${fresh.modules.length}|${pMatched.size}|${tierCounts.get("unique-signature") ?? 0}|${tierCounts.get("edge-corroborated") ?? 0}|${tierCounts.get("high-overlap-unique") ?? 0}|${fc.twins}|${fc.none}`
);

if (DUMP) {
  fs.writeFileSync(
    DUMP,
    JSON.stringify(
      {
        label: LABEL,
        matches: [...pMatched.entries()],
        tiers: [...tierOf.entries()],
        priorModules: prior.modules.length,
        freshModules: fresh.modules.length,
        freshStatementsOfModule: fresh.modules.map((m) => m.statements),
        freshDeclared: fresh.modules.map((m) => [...m.declared]),
        priorEager: prior.eagerZone,
        freshEager: fresh.eagerZone
      },
      null,
      1
    )
  );
  console.log(`dumped ${DUMP}`);
}

/**
 * 063 Task -1 — mechanism-derived ceilings BEFORE any lever is coded
 * (measurement-pitfalls rule 5/6; exp044 is why).
 *
 *   npx tsx experiments/063-name-contention/ceilings.ts <priorSrc> <freshSrc> <freshDiag.json>
 *
 * Three ceilings, all counted on the SAME churned-pair population the 055
 * ledger prices (paired name-only lines inside REAL statements):
 *
 *   A. adjudication  — occurrences whose fresh id is a DECORATED HINT
 *      LANDING (vote-suggest hint accepted but collision-decorated).
 *      Perfect adjudication turns these exact; their lines heal.
 *   B. mis-match     — occurrences decided by exact-match/binding-cascade
 *      where a contradictory vote sat on the same trail (the i36 flag).
 *   C. ordinal carry — occurrences where prior and fresh ids differ ONLY
 *      in a lib_<hash> instance ordinal (`lib_ab12cd34_2` → `_3`, or the
 *      trailing `-N`/`_N` after the same hash stem).
 */
import * as fs from "node:fs";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, DIAG] = process.argv.slice(2);
if (!PRIOR || !FRESH || !DIAG) {
  console.error("usage: ceilings.ts <priorSrc> <freshSrc> <freshDiag.json>");
  process.exit(1);
}

// ── decorated hint landings + contradictory-vote trails from the diag ──────
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));
const DECO_TAIL = /^(Val|Var|Ref|Item|Data|Result|Value)?\d*$/;
const decoratedLandings = new Set<string>();
/** decorated final -> the undecorated hint it came from */
const hintOfFinal = new Map<string, string>();
const contradictoryFinals = new Set<string>();
for (const t of diag.strategyTrails.trails) {
  const tr = t.trail ?? [];
  const applied = tr.filter(
    (a: { outcome: string }) => a.outcome === "applied"
  );
  const final = applied[applied.length - 1]?.newName;
  if (!final) continue;
  let hint: string | undefined;
  const votes = new Set<string>();
  for (const a of tr) {
    if (a.strategy === "vote-suggest") hint = a.newName;
    if (a.outcome === "vote" && a.newName) votes.add(a.newName);
  }
  if (
    hint &&
    final !== hint &&
    final.startsWith(hint) &&
    DECO_TAIL.test(final.slice(hint.length)) &&
    final.slice(hint.length) !== ""
  ) {
    decoratedLandings.add(final);
    hintOfFinal.set(final, hint);
  }
  const settled = t.settledBy ?? "";
  if (
    (settled === "binding-cascade" || settled === "exact-match") &&
    votes.size > 0 &&
    [...votes].some((v) => v !== final)
  ) {
    contradictoryFinals.add(final);
  }
}

// ── churned paired occurrences (identical predicate to the 055 ledger) ─────
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
const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

const LIB_ORD = /^(lib_[0-9a-f]{8})[-_]?(\d*)$/;
let occTotal = 0;
let ceilingA = 0;
let ceilingAStrict = 0;
let ceilingB = 0;
let ceilingC = 0;
const pairsAStrict = new Set<number>();
const pairsA = new Set<number>();
const pairsB = new Set<number>();
const pairsC = new Set<number>();
let pairId = 0;
const exA: string[] = [];
const exC: string[] = [];
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a);
    const tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const diffs: { prior: string; fresh: string }[] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text)
        diffs.push({ prior: ta[i].text, fresh: tb[i].text });
    }
    if (!ok || diffs.length === 0) continue;
    pairId++;
    for (const d of diffs) {
      occTotal++;
      if (decoratedLandings.has(d.fresh)) {
        ceilingA++;
        pairsA.add(pairId);
        // STRICT: the line heals only if the undecorated hint IS the
        // prior-side name — otherwise an exact landing still churns.
        if (hintOfFinal.get(d.fresh) === d.prior) {
          ceilingAStrict++;
          pairsAStrict.add(pairId);
          if (exA.length < 6) exA.push(`${d.prior} -> ${d.fresh}`);
        }
      }
      if (contradictoryFinals.has(d.fresh)) {
        ceilingB++;
        pairsB.add(pairId);
      }
      const mp = LIB_ORD.exec(d.prior);
      const mf = LIB_ORD.exec(d.fresh);
      if (mp && mf && mp[1] === mf[1] && mp[2] !== mf[2]) {
        ceilingC++;
        pairsC.add(pairId);
        if (exC.length < 6) exC.push(`${d.prior} -> ${d.fresh}`);
      }
    }
  }
}

console.log(`churned occurrences total: ${occTotal} (${pairId} pairs)`);
console.log(
  `A. decorated-hint-landing occurrences: ${ceilingA} on ${pairsA.size} pairs (loose upper bound)`
);
console.log(
  `A-STRICT (hint == prior, heals if exact): ${ceilingAStrict} on ${pairsAStrict.size} pairs -> ceiling ${2 * pairsAStrict.size} ledger lines`
);
console.log(`   e.g. ${exA.join(" | ")}`);
console.log(
  `B. contradictory-vote holder occurrences: ${ceilingB} on ${pairsB.size} pairs -> ceiling ${2 * pairsB.size} ledger lines`
);
console.log(
  `C. lib-ordinal-only occurrences: ${ceilingC} on ${pairsC.size} pairs -> ceiling ${2 * pairsC.size} ledger lines`
);
console.log(`   e.g. ${exC.join(" | ")}`);

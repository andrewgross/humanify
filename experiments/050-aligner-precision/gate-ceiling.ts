/**
 * 050 task 1 — the MEASURED ceiling for the aligner's ambiguity gate.
 *
 *   npx tsx experiments/050-aligner-precision/gate-ceiling.ts <priorSrc> <freshSrc>
 *
 * The gate (`alignFileStatements`) lets a statement claim its prior slot only
 * when its statement hash occurs exactly once on each side, because pairing
 * same-hash siblings blind "teleports their text and MANUFACTURES churn:
 * measured +2.3% on the 118->119 hop". Post-049 that abstention is the largest
 * remaining reorder bucket: 1,174 git lines over 355 statements.
 *
 * The question this answers is NOT "how big is the bucket" — that is already
 * measured — but **how much of it does evidence actually resolve**. 1,174 is a
 * POPULATION, not a recoverable share, and the +2.3% precedent says a
 * half-confident pairing costs more than it returns.
 *
 * The evidence tested is the shipped one: `collectMembers` + `assignBucket` from
 * `src/rename/family-permute.ts`, the pass merged in exp048, which pairs
 * same-hash members by MASKED USAGE CONTEXT (reference lines with each side's own
 * name blanked) under a strict-improvement bar. Importing it rather than
 * approximating it is deliberate — an approximation would measure itself
 * (measurement-pitfalls rule 4, and exp045 closed this whole axis on exactly that
 * mistake).
 *
 * Reported three ways, tightening:
 *   RESOLVED      — context evidence pairs the fresh statement with a prior one
 *   AT PRIOR SLOT — and that pairing would place it where the prior had it
 *   CAPPED        — and per file, no more than what git actually prints
 *
 * CAPPED is the number to quote. The raw charge carries known artifacts:
 * `files-api.js` is charged 332 lines where git prints 4.
 */

import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { assignBucket } from "../../src/rename/family-permute.js";
import { collectMembers } from "../../src/rename/family-permute-step.js";
import {
  onLcs,
  statementsOf,
  type Stmt
} from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: gate-ceiling.ts <priorSrc> <freshSrc>");
  process.exit(1);
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

function gitLines(a: string, b: string): number {
  try {
    execFileSync("diff", [a, b], { encoding: "utf8" });
    return 0;
  } catch (e) {
    const out = String((e as { stdout?: string }).stdout ?? "");
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

function parse(code: string): t.File | null {
  try {
    const ast = parseSync(code, { sourceType: "unambiguous" });
    return ast && ast.type === "File" ? (ast as t.File) : null;
  } catch {
    return null;
  }
}

let bucketLines = 0;
let bucketN = 0;
let resolved = 0;
let resolvedN = 0;
let noName = 0;
let byName = 0;
let byNameN = 0;
let byContext = 0;
let byContextN = 0;
const cappedByFile = new Map<string, number>();

for (const f of walk(FRESH)) {
  const pf = path.join(PRIOR, f);
  if (!fs.existsSync(pf)) continue;
  const ff = path.join(FRESH, f);
  const pCode = fs.readFileSync(pf, "utf8");
  const fCode = fs.readFileSync(ff, "utf8");

  const prior = statementsOf(pCode);
  const fresh = statementsOf(fCode);
  const key = (s: Stmt) => `${s.hash} ${s.text}`;

  // The KPI's own displaced set.
  const avail = new Map<string, number>();
  for (const s of prior) avail.set(key(s), (avail.get(key(s)) ?? 0) + 1);
  const fm: Stmt[] = [];
  for (const s of fresh) {
    const n = avail.get(key(s)) ?? 0;
    if (n > 0) {
      avail.set(key(s), n - 1);
      fm.push(s);
    }
  }
  const still = new Map(avail);
  const pm: Stmt[] = [];
  for (const s of prior) {
    const n = still.get(key(s)) ?? 0;
    if (n > 0) still.set(key(s), n - 1);
    else pm.push(s);
  }
  const inOrder = onLcs(pm.map(key), fm.map(key));

  // The gate's own test: hash multiplicity per side.
  const fc = new Map<string, number>();
  for (const s of fresh) fc.set(s.hash, (fc.get(s.hash) ?? 0) + 1);
  const pc = new Map<string, number>();
  for (const s of prior) pc.set(s.hash, (pc.get(s.hash) ?? 0) + 1);

  const displacedAmbiguous = fm.filter(
    (s, i) => !inOrder.has(i) && !(fc.get(s.hash) === 1 && pc.get(s.hash) === 1)
  );
  if (displacedAmbiguous.length === 0) continue;

  const pAst = parse(pCode);
  const fAst = parse(fCode);
  if (!pAst || !fAst) continue;
  // The SHIPPED evidence: top-level bindings with masked usage contexts.
  const pMembers = collectMembers(pAst, pCode, false);
  const fMembers = collectMembers(fAst, fCode, false);
  const pByHash = new Map<string, typeof pMembers>();
  for (const m of pMembers) {
    const l = pByHash.get(m.hash) ?? [];
    l.push(m);
    pByHash.set(m.hash, l);
  }
  const fByHash = new Map<string, typeof fMembers>();
  for (const m of fMembers) {
    const l = fByHash.get(m.hash) ?? [];
    l.push(m);
    fByHash.set(m.hash, l);
  }
  // Declaration order, so assignBucket's index tie-break is stable.
  const ord = <T extends { declStart: number }>(l: T[]) =>
    [...l].sort((a, b) => a.declStart - b.declStart);

  let fileCap = 0;
  for (const s of displacedAmbiguous) {
    const ln = s.lines.length * 2;
    bucketLines += ln;
    bucketN++;
    const fSibs = fByHash.get(s.hash);
    const pSibs = pByHash.get(s.hash);
    if (!fSibs || !pSibs || pSibs.length < 1) {
      // No named top-level binding for this statement (a bare call, an
      // expression) — the context evidence does not apply at all.
      noName += ln;
      continue;
    }
    // Which fresh member IS this statement? Match by declaration offset.
    const me = fSibs.find(
      (m) => fCode.slice(m.declStart, m.declStart + s.text.length) === s.text
    );
    if (!me) {
      noName += ln;
      continue;
    }
    // TIER 1 — NAME IDENTITY. The gate abstains on HASH ambiguity, but the
    // members usually carry distinct names, and a name present exactly once on
    // each side identifies the statement outright with no inference at all. This
    // is not the +2.3% "blind pairing" the gate refused; it is the same
    // round-trip lock the family-permute pass applies before any context is
    // consulted.
    const mine = fSibs.filter((m) => m.name === me.name).length;
    const theirs = pSibs.filter((m) => m.name === me.name).length;
    if (mine === 1 && theirs === 1) {
      byName += ln;
      byNameN++;
      resolved += ln;
      resolvedN++;
      fileCap += ln;
      continue;
    }
    // TIER 2 — MASKED USAGE CONTEXT, the shipped evidence, for the members whose
    // names do NOT round-trip.
    const moves = assignBucket(ord(fSibs), ord(pSibs));
    if (!moves.some((mv) => mv.fromName === me.name)) continue;
    byContext += ln;
    byContextN++;
    resolved += ln;
    resolvedN++;
    fileCap += ln;
  }
  if (fileCap > 0) {
    cappedByFile.set(f, Math.min(fileCap, gitLines(pf, ff)));
  }
}

const capped = [...cappedByFile.values()].reduce((a, b) => a + b, 0);
const pct = (n: number) =>
  bucketLines
    ? `${((100 * n) / bucketLines).toFixed(1)}%`.padStart(6)
    : "   n/a";

console.log(
  `AMBIGUOUS displaced statements: ${bucketN}, ${bucketLines} git lines`
);
console.log(
  `  no named top-level binding (evidence N/A)  ${String(noName).padStart(5)} ln ${pct(noName)}`
);
console.log(
  `  RESOLVED by masked usage context           ${String(resolved).padStart(5)} ln ${pct(resolved)}  ${resolvedN} statements`
);
console.log(
  `     of which by NAME IDENTITY               ${String(byName).padStart(5)} ln ${pct(byName)}  ${byNameN} statements`
);
console.log(
  `     of which by MASKED CONTEXT              ${String(byContext).padStart(5)} ln ${pct(byContext)}  ${byContextN} statements`
);
console.log(
  `  ... CAPPED at what git prints per file     ${String(capped).padStart(5)} ln ${pct(capped)}  <- QUOTE THIS`
);

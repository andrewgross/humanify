/**
 * 049 — is the residual reorder charge pinned by LOAD ORDER, or refused by the
 * aligner's AMBIGUITY GATE?
 *
 *   npx tsx experiments/049-reorder-drivers/ambiguity-split.ts <priorSrc> <freshSrc>
 *
 * `alignFileStatements` lets a statement claim its prior slot only when its
 * statement hash occurs EXACTLY ONCE on each side ("precision gate" — pairing
 * same-hash siblings teleports their text and manufactured +2.3% churn on
 * 118->119). So a displaced statement has two possible reasons, and they need
 * completely different fixes:
 *
 *   load-order  — a barrier stands between its prior slot and where it landed.
 *                 Irreducible without changing what may cross a barrier.
 *   ambiguous   — its hash has siblings, so the gate ABSTAINED. Reducible with
 *                 evidence that distinguishes siblings — which is exactly what
 *                 the just-merged family-permute pass uses for NAMES (masked
 *                 usage context).
 *
 * Counts the same displaced population the reorder KPI charges, using the
 * classifier's own statement split and LCS.
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import {
  onLcs,
  statementsOf,
  type Stmt
} from "../037-noise-source-decomposition/diff-composition.js";
import { identifyBunLazyInit } from "../../src/shared/bun-helpers.js";
import { analyzeLoadOrder } from "../../src/split/load-order.js";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";

const [PRIOR, FRESH] = process.argv.slice(2);
function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}
const files = walk(FRESH);
let pure: string | null = null;
for (const f of files) {
  pure = identifyBunLazyInit(fs.readFileSync(path.join(FRESH, f), "utf8"));
  if (pure) break;
}
function effectsOf(code: string): boolean[] {
  try {
    const ast = parseSync(code, { sourceType: "unambiguous" }) as any;
    const body = ast.program.body as t.Statement[];
    const facts = analyzeLoadOrder(body, {
      pureCallNames: pure ? new Set([pure]) : undefined
    });
    return body.map((_, i) => facts[i]?.effects ?? true);
  } catch {
    return [];
  }
}

let total = 0,
  ambig = 0,
  barrierBetween = 0,
  neither = 0,
  selfBarrier = 0;
let ambigN = 0,
  barrN = 0,
  neitherN = 0,
  selfN = 0;
const neitherEx: string[] = [];
const neitherByFile = new Map<string, number>();
for (const f of files) {
  const pf = path.join(PRIOR, f);
  if (!fs.existsSync(pf)) continue;
  const pCode = fs.readFileSync(pf, "utf8"),
    fCode = fs.readFileSync(path.join(FRESH, f), "utf8");
  const prior = statementsOf(pCode),
    fresh = statementsOf(fCode);
  const eff = effectsOf(fCode);
  const key = (s: Stmt) => `${s.hash} ${s.text}`;
  const avail = new Map<string, number>();
  for (const s of prior) avail.set(key(s), (avail.get(key(s)) ?? 0) + 1);
  const fm: Stmt[] = [];
  const fmIdx: number[] = [];
  fresh.forEach((s, i) => {
    const n = avail.get(key(s)) ?? 0;
    if (n > 0) {
      avail.set(key(s), n - 1);
      fm.push(s);
      fmIdx.push(i);
    }
  });
  const still = new Map(avail);
  const pm: Stmt[] = [];
  for (const s of prior) {
    const n = still.get(key(s)) ?? 0;
    if (n > 0) still.set(key(s), n - 1);
    else pm.push(s);
  }
  const inOrder = onLcs(pm.map(key), fm.map(key));
  // hash multiplicity per side, the gate's own test
  const fc = new Map<string, number>();
  for (const s of fresh) fc.set(s.hash, (fc.get(s.hash) ?? 0) + 1);
  const pc = new Map<string, number>();
  for (const s of prior) pc.set(s.hash, (pc.get(s.hash) ?? 0) + 1);
  const pIdx = new Map<string, number[]>();
  pm.forEach((s, i) => {
    const l = pIdx.get(key(s)) ?? [];
    l.push(i);
    pIdx.set(key(s), l);
  });
  fm.forEach((s, i) => {
    if (inOrder.has(i)) return;
    const ln = s.lines.length * 2;
    total += ln;
    // The statement itself being a load-time barrier pins it outright — checking
    // only for a barrier BETWEEN two positions misses the commonest case, a
    // `defineModuleExports(x, {...})` call that is the only barrier in its range.
    if (eff[fmIdx[i]]) {
      selfBarrier += ln;
      selfN++;
      return;
    }
    const unamb = fc.get(s.hash) === 1 && pc.get(s.hash) === 1;
    if (!unamb) {
      ambig += ln;
      ambigN++;
      return;
    }
    const from = pIdx.get(key(s))?.shift() ?? i;
    const lo = Math.min(from, i),
      hi = Math.max(from, i);
    let crosses = false;
    for (let j = lo; j <= hi && j < fm.length; j++)
      if (j !== i && eff[fmIdx[j]]) {
        crosses = true;
        break;
      }
    if (crosses) {
      barrierBetween += ln;
      barrN++;
    } else {
      neither += ln;
      neitherN++;
      neitherByFile.set(f, (neitherByFile.get(f) ?? 0) + ln);
      neitherEx.push(
        `  ${String(ln).padStart(4)}ln  prior[${from}] -> fresh[${i}]  ${f}  ${s.text.split("\n")[0].slice(0, 70).replace(/\s+/g, " ")}`
      );
    }
  });
}
/**
 * What git actually prints for one file — the reviewer-facing cost.
 *
 * The statement-LCS charge can exceed it wildly: exp045 measured
 * `table/skill-docs/files-api.js` at 332 charged lines where git prints 4,
 * because when one 332-line blob and two 1-line statements swap relative order,
 * the LCS may declare the blob moved while git reports the two small ones. So a
 * per-file cap turns the charge into a defensible upper bound on what fixing the
 * displacement could actually save a reviewer.
 */
function gitLines(rel: string): number {
  try {
    execFileSync("diff", [path.join(PRIOR, rel), path.join(FRESH, rel)], {
      encoding: "utf8"
    });
    return 0;
  } catch (e) {
    const out = (e as { stdout?: string }).stdout ?? "";
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

let capped = 0;
for (const [f, ln] of neitherByFile) capped += Math.min(ln, gitLines(f));

const p = (n: number) => `${((100 * n) / total).toFixed(1)}%`.padStart(6);
console.log(`reorder charge: ${total} git lines\n`);
console.log(
  `  IS A BARRIER ITSELF — cannot move          ${String(selfBarrier).padStart(5)} ln ${p(selfBarrier)}  ${selfN} statements`
);
console.log(
  `  AMBIGUOUS HASH — the gate abstained    ${String(ambig).padStart(5)} ln ${p(ambig)}  ${ambigN} statements`
);
console.log(
  `  BARRIER BETWEEN — load order pins it   ${String(barrierBetween).padStart(5)} ln ${p(barrierBetween)}  ${barrN} statements`
);
console.log(
  `  NEITHER — unambiguous and unblocked    ${String(neither).padStart(5)} ln ${p(neither)}  ${neitherN} statements`
);
console.log(
  `  NEITHER, CAPPED at what git prints per file    ${String(capped).padStart(5)} ln ${p(capped)}  <- the defensible ceiling`
);
console.log("\n  the NEITHER population in full:");
console.log(
  neitherEx.sort((a, b) => parseInt(b.trim()) - parseInt(a.trim())).join("\n")
);
console.log(
  `\n  "NEITHER" is the interesting bucket: the aligner had the evidence AND`
);
console.log(
  `  the freedom to restore these, and did not. That is a bug or a limit of`
);
console.log(`  its ordering pass, not an irreducible constraint.`);

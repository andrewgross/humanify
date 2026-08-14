/**
 * 069 Task 0 — strict pre-code ceilings for the anchoring candidates
 * (exp063 whole-pair method; sized skips are results).
 *
 *   npx tsx experiments/069-naming-stability/ceilings.ts \
 *     <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot> [label]
 *
 * Candidates priced on the 055 churned-pair population (paired name-only
 * lines inside REAL statements):
 *
 *   (a) edit-pair prior-name HINTS into asks — a pair heals only when
 *       EVERY churned identifier on it joins cleanly to an llm-terminal
 *       trail (per-binding precision required to hint), maps fresh→prior
 *       unanimously, and the prior is not below-floor. Decayed by
 *       CONTENTION: a prior name already worn by another binding in the
 *       same file cannot land exact (exp061: 86 of 187 hints decorated).
 *   (b) neighborhood anchoring — needs NO per-binding join precision:
 *       stabilizing every ask in the file helps any identifier whose
 *       candidate trails are ALL llm-terminal, ambiguous joins included.
 *       Its ceiling is therefore a superset of (a)'s. Compliance is
 *       unknowable offline (no prior word is delivered, only steadier
 *       context), so only the CEILING is stated, never a realized figure.
 *
 * Every number is an upper bound assuming perfect landings; realized-if-
 * shipped for (a) additionally shown at the measured 73% exact-landing
 * rate among uncontended hints (exp061-r1: 74 exact of 101 uncontended).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parse } from "@babel/parser";
import type * as t from "@babel/types";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";
import { isBelowFloorName } from "../../src/rename/minted-census.js";

const [PRIOR, FRESH, DIAG, OUT_ROOT, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH || !DIAG || !OUT_ROOT) {
  console.error(
    "usage: ceilings.ts <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot> [label]"
  );
  process.exit(1);
}

// ── statement spans (verified method: trail locs are valid line numbers
// in the saved humanified bundle; split-ledger order[] is parallel) ────────
const ledger = JSON.parse(
  fs.readFileSync(path.join(OUT_ROOT, ".humanify/split-ledger.json"), "utf8")
);
const order: string[] = ledger.order;
const bundleCode = fs.readFileSync(
  path.join(OUT_ROOT, ".humanify/humanified.js"),
  "utf8"
);
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));

const ast = parse(bundleCode, { sourceType: "unambiguous" });
const blocks: t.Statement[][] = [];
(function findBlocks(node: unknown): void {
  if (!node || typeof node !== "object") return;
  if (Array.isArray(node)) {
    for (const c of node) findBlocks(c);
    return;
  }
  const n = node as { type?: string; body?: unknown };
  if (
    n.type === "BlockStatement" &&
    Array.isArray(n.body) &&
    n.body.length === order.length
  ) {
    blocks.push(n.body as t.Statement[]);
  }
  for (const k of Object.keys(n)) {
    if (k === "loc" || k === "leadingComments" || k === "trailingComments")
      continue;
    findBlocks((n as Record<string, unknown>)[k]);
  }
})(ast.program);
if (blocks.length !== 1) {
  console.error(`expected one ${order.length}-statement block`);
  process.exit(1);
}
const spans = blocks[0].map((s) => ({
  start: s.loc?.start.line ?? -1,
  end: s.loc?.end.line ?? -1
}));
function stmtAtLine(line: number): number {
  let lo = 0;
  let hi = spans.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (line < spans[mid].start) hi = mid - 1;
    else if (line > spans[mid].end) lo = mid + 1;
    else return mid;
  }
  return -1;
}

// ── trails bucketed by statement ──────────────────────────────────────────
interface TrailRef {
  oldName: string;
  finalName: string;
  terminalBy: string;
}
const trailsByStmt = new Map<number, TrailRef[]>();
for (const tr of diag.strategyTrails.trails) {
  const applied = (tr.trail ?? []).filter(
    (a: { outcome: string }) => a.outcome === "applied"
  );
  const last = applied[applied.length - 1];
  if (!last?.newName) continue;
  const m = /^(\d+):/.exec(tr.loc ?? "");
  if (!m) continue;
  const k = stmtAtLine(Number(m[1]));
  if (k < 0) continue;
  const list = trailsByStmt.get(k) ?? [];
  list.push({
    oldName: tr.oldName,
    finalName: last.newName,
    terminalBy: tr.terminalBy ?? tr.settledBy ?? "unknown"
  });
  trailsByStmt.set(k, list);
}
const fileOf = (k: number) => order[k].replace(/^src\//, "");
const stmtsByFile = new Map<string, number[]>();
for (let k = 0; k < order.length; k++) {
  if (!order[k].startsWith("src/")) continue;
  const f = fileOf(k);
  const l = stmtsByFile.get(f) ?? [];
  l.push(k);
  stmtsByFile.set(f, l);
}
/** file -> every final name settled in it (for the contention check) */
const finalNamesByFile = new Map<string, Set<string>>();
for (const [f, ks] of stmtsByFile) {
  const s = new Set<string>();
  for (const k of ks)
    for (const tr of trailsByStmt.get(k) ?? []) {
      s.add(tr.finalName);
    }
  finalNamesByFile.set(f, s);
}

// ── churned occurrences ───────────────────────────────────────────────────
function pairsIn(a: string, b: string): [string, string][] {
  const pa = a.split("\n");
  const pb = b.split("\n");
  const sa = new Set(pa);
  const sb = new Set(pb);
  const rem = pa.filter((l) => !sb.has(l));
  const add = pb.filter((l) => !sa.has(l));
  const n = Math.min(rem.length, add.length);
  const out: [string, string][] = [];
  for (let i = 0; i < n; i++) out.push([rem[i], add[i]]);
  return out;
}
const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

interface Occ {
  pairId: number;
  file: string;
  freshId: string;
  priorId: string;
}
const occs: Occ[] = [];
let nextPair = 0;
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
    nextPair++;
    for (const d of diffs)
      occs.push({
        pairId: nextPair,
        file: s.file,
        freshId: d.fresh,
        priorId: d.prior
      });
  }
}

// unanimity of fresh→prior per file
const mappings = new Map<string, Set<string>>();
for (const o of occs) {
  const s = mappings.get(`${o.file}|${o.freshId}`) ?? new Set();
  s.add(o.priorId);
  mappings.set(`${o.file}|${o.freshId}`, s);
}

// ── per-occurrence disposition ────────────────────────────────────────────
type Disp =
  | "hintable" // clean llm join, unanimous, prior legal → candidate (a)
  | "hintable-contended" // as above but prior name already worn in-file
  | "anchor-only" // ambiguous join but ALL candidates llm-terminal → (b) only
  | "not-llm"
  | "non-unanimous"
  | "below-floor-prior"
  | "no-trail";
function disposition(o: Occ): Disp {
  if ((mappings.get(`${o.file}|${o.freshId}`)?.size ?? 0) !== 1)
    return "non-unanimous";
  if (isBelowFloorName(o.priorId)) return "below-floor-prior";
  const ks = stmtsByFile.get(o.file) ?? [];
  const cands = ks
    .flatMap((k) => trailsByStmt.get(k) ?? [])
    .filter((tr) => tr.finalName === o.freshId);
  if (cands.length === 0) return "no-trail";
  if (cands.length > 1) {
    return cands.every((tr) => tr.terminalBy === "llm")
      ? "anchor-only"
      : "not-llm";
  }
  if (cands[0].terminalBy !== "llm") return "not-llm";
  const worn = finalNamesByFile.get(o.file)?.has(o.priorId) ?? false;
  return worn ? "hintable-contended" : "hintable";
}
const dispByOcc = new Map<Occ, Disp>();
for (const o of occs) dispByOcc.set(o, disposition(o));

// ── pair-level ceilings ───────────────────────────────────────────────────
const byPair = new Map<number, Occ[]>();
for (const o of occs) {
  const l = byPair.get(o.pairId) ?? [];
  l.push(o);
  byPair.set(o.pairId, l);
}
const HINT = new Set<Disp>(["hintable"]);
const HINT_C = new Set<Disp>(["hintable", "hintable-contended"]);
const ANCHOR = new Set<Disp>(["hintable", "hintable-contended", "anchor-only"]);
let pairsAll = 0;
let aStrict = 0; // (a) uncontended only
let aWithContention = 0; // (a) if adjudication also existed
let bCeiling = 0; // (b) superset
const dispCount = new Map<Disp, number>();
for (const [, list] of byPair) {
  pairsAll++;
  const ds = list.map((o) => dispByOcc.get(o) as Disp);
  for (const d of ds) dispCount.set(d, (dispCount.get(d) ?? 0) + 1);
  if (ds.every((d) => HINT.has(d))) aStrict++;
  if (ds.every((d) => HINT_C.has(d))) aWithContention++;
  if (ds.every((d) => ANCHOR.has(d))) bCeiling++;
}
const EXACT_RATE = 0.73; // exp061-r1: 74 exact of 101 uncontended hints
console.log(`=== 069 ceilings — ${LABEL || FRESH} ===`);
console.log(`churned pairs: ${pairsAll} (${2 * pairsAll} ledger lines)`);
console.log("occurrence dispositions:");
for (const [k, v] of [...dispCount.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(22)} ${String(v).padStart(6)}`);
console.log(
  `(a) edit-pair hints, uncontended pairs:        ${aStrict} (${2 * aStrict} ln strict; ~${Math.round(2 * aStrict * EXACT_RATE)} ln at measured landing rate)`
);
console.log(
  `(a) incl. contended (needs adjudication too):  ${aWithContention} (${2 * aWithContention} ln)`
);
console.log(
  `(b) neighborhood anchoring master ceiling:     ${bCeiling} (${2 * bCeiling} ln — compliance unknowable offline)`
);
console.log(
  `ROW|${LABEL}|069ceiling|${pairsAll}|${aStrict}|${aWithContention}|${bCeiling}`
);

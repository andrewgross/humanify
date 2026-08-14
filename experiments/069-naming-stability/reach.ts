/**
 * 069 inverted reach analysis — start from the CHURN, ask what the
 * shipped mechanism could ever touch (the counterfactual audit found
 * its 116 landed hints were all already-stable names: reach∩churn = 0).
 *
 *   npx tsx experiments/069-naming-stability/reach.ts <runRoot> [label]
 *     runRoot: /tmp/eval-work/exp066-r1 (needs 2.1.85-rebased + 2.1.86
 *     trees, both .humanify bundles + split-ledgers, 2.1.86.diag.json)
 *
 * For every churning binding in the 055 paired name-only population,
 * walk the mechanism's gate funnel and record WHERE it dies:
 *   population   — llm module-batch / llm function-ask / not-llm / no-trail
 *   twin         — fresh statement hash present in prior (twin exists)
 *   headFresh    — masked head unique among fresh residue statements
 *   headPriorAll — masked head unique among ALL prior statements (shipped)
 *   headPriorFile— masked head unique within its prior FILE (relax (c))
 *   overlap      — ≥50% word-token overlap prior↔fresh statement
 *   nameOnly     — the pairing recovers this binding's prior name
 * Texts are humanified-bundle slices on BOTH sides — the format-parity
 * (b) variant by construction; the shipped (a) variant can only be
 * smaller (its measured value is the audit's zero).
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

const [RUN_ROOT, LABEL = ""] = process.argv.slice(2);
if (!RUN_ROOT) {
  console.error("usage: reach.ts <runRoot> [label]");
  process.exit(1);
}
const PRIOR_ROOT = path.join(RUN_ROOT, "2.1.85-rebased");
const FRESH_ROOT = path.join(RUN_ROOT, "2.1.86");
const DIAG = path.join(RUN_ROOT, "2.1.86.diag.json");

interface Side {
  order: string[];
  hashes: string[];
  spans: { start: number; end: number }[];
  slices: (string | null)[];
}
function loadSide(root: string): Side {
  const ledger = JSON.parse(
    fs.readFileSync(path.join(root, ".humanify/split-ledger.json"), "utf8")
  );
  const code = fs.readFileSync(
    path.join(root, ".humanify/humanified.js"),
    "utf8"
  );
  const ast = parse(code, { sourceType: "unambiguous" });
  const blocks: t.Statement[][] = [];
  (function find(node: unknown): void {
    if (!node || typeof node !== "object") return;
    if (Array.isArray(node)) {
      for (const c of node) find(c);
      return;
    }
    const n = node as { type?: string; body?: unknown };
    if (
      n.type === "BlockStatement" &&
      Array.isArray(n.body) &&
      n.body.length === ledger.order.length
    ) {
      blocks.push(n.body as t.Statement[]);
    }
    for (const k of Object.keys(n)) {
      if (k === "loc") continue;
      find((n as Record<string, unknown>)[k]);
    }
  })(ast.program);
  if (blocks.length !== 1) throw new Error(`wrapper not found in ${root}`);
  const spans = blocks[0].map((s) => ({
    start: s.loc?.start.line ?? -1,
    end: s.loc?.end.line ?? -1
  }));
  const slices = blocks[0].map((s) =>
    s.start != null && s.end != null ? code.slice(s.start, s.end) : null
  );
  return { order: ledger.order, hashes: ledger.hashes, spans, slices };
}
const prior = loadSide(PRIOR_ROOT);
const fresh = loadSide(FRESH_ROOT);
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));

function stmtAtLine(side: Side, line: number): number {
  let lo = 0;
  let hi = side.spans.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (line < side.spans[mid].start) hi = mid - 1;
    else if (line > side.spans[mid].end) lo = mid + 1;
    else return mid;
  }
  return -1;
}

// trails by statement + ask kind
interface TrailRef {
  oldName: string;
  finalName: string;
  terminalBy: string;
  stmt: number;
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
  const k = stmtAtLine(fresh, Number(m[1]));
  if (k < 0) continue;
  const list = trailsByStmt.get(k) ?? [];
  list.push({
    oldName: tr.oldName,
    finalName: last.newName,
    terminalBy: tr.terminalBy ?? tr.settledBy ?? "unknown",
    stmt: k
  });
  trailsByStmt.set(k, list);
}
const askByRename = new Map<string, string>();
for (const r of diag.renamed ?? []) {
  askByRename.set(
    `${r.name}→${r.newName}`,
    String(r.functionId ?? "").startsWith("module-binding-batch:")
      ? "module-batch"
      : "function-ask"
  );
}
const fileOfFresh = (k: number) => fresh.order[k].replace(/^src\//, "");
const stmtsByFile = new Map<string, number[]>();
for (let k = 0; k < fresh.order.length; k++) {
  if (!fresh.order[k].startsWith("src/")) continue;
  const f = fileOfFresh(k);
  const l = stmtsByFile.get(f) ?? [];
  l.push(k);
  stmtsByFile.set(f, l);
}

// churned occurrences on the emitted trees
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
composeDiff(path.join(PRIOR_ROOT, "src"), path.join(FRESH_ROOT, "src"), {
  samples,
  cap: 500_000
});
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

// ── gate machinery ────────────────────────────────────────────────────────
function maskedHead(text: string): string | null {
  const first = text.split("\n", 1)[0];
  const tokens = tokenizeLine(first);
  if (!tokens) return null;
  return tokens.map((t2) => (t2.kind === "ident" ? " " : t2.text)).join("");
}
function headIndex(slices: (string | null)[]): Map<string, number[]> {
  const byHead = new Map<string, number[]>();
  for (let i = 0; i < slices.length; i++) {
    const s = slices[i];
    if (s === null) continue;
    const h = maskedHead(s);
    if (h === null) continue;
    const l = byHead.get(h) ?? [];
    l.push(i);
    byHead.set(h, l);
  }
  return byHead;
}
const priorHeadsAll = headIndex(prior.slices);
// per-prior-file head index
const priorHeadsByFile = new Map<string, Map<string, number[]>>();
for (let i = 0; i < prior.order.length; i++) {
  const f = prior.order[i];
  const s = prior.slices[i];
  if (s === null) continue;
  const h = maskedHead(s);
  if (h === null) continue;
  let m = priorHeadsByFile.get(f);
  if (!m) {
    m = new Map();
    priorHeadsByFile.set(f, m);
  }
  const l = m.get(h) ?? [];
  l.push(i);
  m.set(h, l);
}
const priorHashCounts = new Map<string, number>();
for (const h of prior.hashes)
  priorHashCounts.set(h, (priorHashCounts.get(h) ?? 0) + 1);
const IDENT_RE = /[A-Za-z_$][\w$]*/g;
function wordTokens(text: string): Set<string> {
  return new Set(text.match(IDENT_RE) ?? []);
}
function overlapRatio(a: string, b: string): number {
  const wa = wordTokens(a);
  const wb = wordTokens(b);
  let inter = 0;
  for (const w of wa) if (wb.has(w)) inter++;
  return inter / Math.max(wa.size, wb.size, 1);
}
/** does the slice pairing recover freshId→priorId (name-only lines)? */
function pairRecovers(
  priorText: string,
  freshText: string,
  freshFinal: string,
  priorName: string
): boolean {
  for (const [p, f] of pairsIn(priorText, freshText)) {
    const tp = tokenizeLine(p);
    const tf = tokenizeLine(f);
    if (!tp || !tf || tp.length !== tf.length) continue;
    let clean = true;
    let hit = false;
    for (let i = 0; i < tp.length; i++) {
      if (tp[i].kind !== tf[i].kind) {
        clean = false;
        break;
      }
      if (tp[i].text === tf[i].text) continue;
      if (tp[i].kind !== "ident") {
        clean = false;
        break;
      }
      if (tf[i].text === freshFinal && tp[i].text === priorName) hit = true;
    }
    if (clean && hit) return true;
  }
  return false;
}

// ── the funnel, per occurrence ────────────────────────────────────────────
type Stage =
  | "no-trail"
  | "ambiguous-join"
  | "not-llm"
  | "llm-function-ask"
  | "twin-exists"
  | "head-not-unique-prior-all"
  | "overlap-fail"
  | "pairing-fails"
  | "REACHABLE-shipped"
  | "REACHABLE-perfile-only";
const stageCount = new Map<Stage, number>();
const stageByOcc = new Map<Occ, Stage>();
function classify(o: Occ): Stage {
  const ks = stmtsByFile.get(o.file) ?? [];
  const cands = ks
    .flatMap((k) => trailsByStmt.get(k) ?? [])
    .filter((tr) => tr.finalName === o.freshId);
  if (cands.length === 0) return "no-trail";
  if (cands.length > 1) return "ambiguous-join";
  const tr = cands[0];
  if (tr.terminalBy !== "llm") return "not-llm";
  const ask = askByRename.get(`${tr.oldName}→${tr.finalName}`);
  if (ask !== "module-batch") return "llm-function-ask";
  const k = tr.stmt;
  if (priorHashCounts.has(fresh.hashes[k])) return "twin-exists";
  const freshSlice = fresh.slices[k];
  if (freshSlice === null) return "pairing-fails";
  const head = maskedHead(freshSlice);
  if (head === null) return "pairing-fails";
  const allIdx = priorHeadsAll.get(head);
  const uniqueAll = allIdx?.length === 1;
  // per-file relaxation: unique within the prior file this statement's
  // FILE maps to (same emitted path — the layout is inherited)
  const perFile = priorHeadsByFile.get(fresh.order[k]);
  const fileIdx = perFile?.get(head);
  const uniqueFile = fileIdx?.length === 1;
  const priorIdx = uniqueAll ? allIdx[0] : uniqueFile ? fileIdx[0] : undefined;
  if (priorIdx === undefined) return "head-not-unique-prior-all";
  const priorSlice = prior.slices[priorIdx];
  if (priorSlice === null) return "pairing-fails";
  if (overlapRatio(priorSlice, freshSlice) < 0.5) return "overlap-fail";
  if (!pairRecovers(priorSlice, freshSlice, o.freshId, o.priorId))
    return "pairing-fails";
  return uniqueAll ? "REACHABLE-shipped" : "REACHABLE-perfile-only";
}
for (const o of occs) {
  const s = classify(o);
  stageByOcc.set(o, s);
  stageCount.set(s, (stageCount.get(s) ?? 0) + 1);
}

// whole-pair ceilings
const byPair = new Map<number, Occ[]>();
for (const o of occs) {
  const l = byPair.get(o.pairId) ?? [];
  l.push(o);
  byPair.set(o.pairId, l);
}
let pairsAll = 0;
let healShipped = 0;
let healPerFile = 0;
for (const [, list] of byPair) {
  pairsAll++;
  const ss = list.map((o) => stageByOcc.get(o) as Stage);
  if (ss.every((s) => s === "REACHABLE-shipped")) healShipped++;
  if (
    ss.every((s) => s === "REACHABLE-shipped" || s === "REACHABLE-perfile-only")
  )
    healPerFile++;
}
console.log(`=== 069 inverted reach — ${LABEL || RUN_ROOT} ===`);
console.log(
  `churned pairs ${pairsAll} (${2 * pairsAll} ln), occurrences ${occs.length}`
);
console.log("occurrence funnel (where each churner dies for the mechanism):");
for (const [k, v] of [...stageCount.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(28)} ${String(v).padStart(6)}`);
console.log(
  `whole-pair ceilings: shipped-scope ${healShipped} (${2 * healShipped} ln); +per-file relaxation ${healPerFile} (${2 * healPerFile} ln)`
);
console.log(`ROW|${LABEL}|069reach|${pairsAll}|${healShipped}|${healPerFile}`);

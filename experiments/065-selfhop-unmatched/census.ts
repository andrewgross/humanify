/**
 * 065 — the unmatched-by-reason census.
 *
 *   npx tsx experiments/065-selfhop-unmatched/census.ts \
 *     <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot>
 *
 * Three questions on one run's artifacts (run on BOTH exp061-lever
 * repeats for cluster-level stability):
 *
 * A. Every llm-settled binding (the "3,333"), clustered by why the
 *    cascade never decided: ask kind (module batch vs function ask),
 *    vote state and abstain reasons, and old/new name shape.
 * B. The ambiguity frontier (exp064's ~855): for churned occurrences
 *    whose settled name matches MULTIPLE trails in the file, classify
 *    each collision set — same statement-hash family (true twins),
 *    cross-hash (independent name reuse), and minted-shape share.
 * C. Strict whole-pair ceilings per cluster (exp063/064 method): a pair
 *    counts only when EVERY churned identifier on it belongs to the
 *    cluster; for the twin-swap ceiling, additionally every priorId
 *    must already exist among the colliding family's final names (a
 *    correct identity assignment would heal it by redistribution).
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

const [PRIOR, FRESH, DIAG, OUT_ROOT] = process.argv.slice(2);
if (!PRIOR || !FRESH || !DIAG || !OUT_ROOT) {
  console.error(
    "usage: census.ts <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot>"
  );
  process.exit(1);
}

const ledger = JSON.parse(
  fs.readFileSync(path.join(OUT_ROOT, ".humanify/split-ledger.json"), "utf8")
);
const order: string[] = ledger.order;
const hashes: string[] = ledger.hashes;
const bundleCode = fs.readFileSync(
  path.join(OUT_ROOT, ".humanify/humanified.js"),
  "utf8"
);
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));

// ── statement spans (same verified method as exp061 loc-provenance) ──────
const ast = parse(bundleCode, { sourceType: "unambiguous" });
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
    n.body.length === order.length
  )
    blocks.push(n.body as t.Statement[]);
  for (const k of Object.keys(n)) {
    if (k === "loc") continue;
    find((n as Record<string, unknown>)[k]);
  }
})(ast.program);
if (blocks.length !== 1) {
  console.error(`span recovery failed: ${blocks.length} candidate blocks`);
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

// ── trails, bucketed ─────────────────────────────────────────────────────
interface TrailRef {
  oldName: string;
  finalName: string;
  terminalBy: string;
  stmt: number;
  attempts: {
    strategy: string;
    outcome: string;
    newName?: string;
    reason?: string;
  }[];
}
const trailsByStmt = new Map<number, TrailRef[]>();
const allTrails: TrailRef[] = [];
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
  const ref: TrailRef = {
    oldName: tr.oldName,
    finalName: last.newName,
    terminalBy: tr.terminalBy ?? tr.settledBy ?? "unknown",
    stmt: k,
    attempts: tr.trail ?? []
  };
  allTrails.push(ref);
  const l = trailsByStmt.get(k) ?? [];
  l.push(ref);
  trailsByStmt.set(k, l);
}

// ── A. cluster every llm-settled binding ─────────────────────────────────
const askByRename = new Map<string, string>();
for (const r of diag.renamed ?? []) {
  askByRename.set(
    `${r.name}→${r.newName}`,
    String(r.functionId ?? "").startsWith("module-binding-batch:")
      ? "module-batch"
      : "function-ask"
  );
}
const MINT = /^(initialize|init|setup|load|create)[A-Z]?\w*?\d+$/;
const clusterA = new Map<string, number>();
let llmTotal = 0;
for (const tr of allTrails) {
  if (tr.terminalBy !== "llm") continue;
  llmTotal++;
  const ask = askByRename.get(`${tr.oldName}→${tr.finalName}`) ?? "no-row";
  const votes = tr.attempts.filter((a) => a.outcome === "vote");
  const abstains = tr.attempts.filter((a) => a.outcome === "abstained");
  let state: string;
  if (votes.length === 0 && abstains.length === 0) state = "bare";
  else if (votes.length === 0) state = `abstain:${abstains[0].reason ?? "?"}`;
  else {
    const names = new Set(votes.map((v) => v.newName));
    state = names.size === 1 ? "votes-unanimous" : "votes-mixed";
    if (abstains.length > 0)
      state += `+${abstains.map((a) => `${a.strategy}(${a.reason ?? "?"})`).sort()[0]}`;
  }
  const shape = MINT.test(tr.finalName) ? "minted-ordinal" : "word";
  const key = `${ask} | ${state} | ${shape}`;
  clusterA.set(key, (clusterA.get(key) ?? 0) + 1);
}
console.log(`\n=== A. llm-settled bindings: ${llmTotal} ===`);
for (const [k, v] of [...clusterA.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 20))
  console.log(`  ${String(v).padStart(5)}  ${k}`);

// ── churned occurrences (055 predicate) ──────────────────────────────────
function pairsIn(a: string, b: string): [string, string][] {
  const pa = a.split("\n");
  const pb = b.split("\n");
  const sa = new Set(pa);
  const sb = new Set(pb);
  const rem = pa.filter((l) => !sb.has(l));
  const add = pb.filter((l) => !sa.has(l));
  const out: [string, string][] = [];
  for (let i = 0; i < Math.min(rem.length, add.length); i++)
    out.push([rem[i], add[i]]);
  return out;
}
const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });
interface Occ {
  pairId: number;
  file: string;
  priorId: string;
  freshId: string;
}
const occs: Occ[] = [];
let pid = 0;
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
    pid++;
    for (const d of diffs)
      occs.push({
        pairId: pid,
        file: s.file,
        priorId: d.prior,
        freshId: d.fresh
      });
  }
}

// file-scoped trail index
const fileOf = (k: number) => order[k].replace(/^src\//, "");
const trailsByFileName = new Map<string, Map<string, TrailRef[]>>();
for (const [k, list] of trailsByStmt) {
  if (!order[k].startsWith("src/")) continue;
  const f = fileOf(k);
  let m = trailsByFileName.get(f);
  if (!m) {
    m = new Map();
    trailsByFileName.set(f, m);
  }
  for (const tr of list) {
    const l = m.get(tr.finalName) ?? [];
    l.push(tr);
    m.set(tr.finalName, l);
  }
}

// ── B. classify ambiguous collision sets ─────────────────────────────────
type Dispo =
  | "unique-join"
  | "ambig-twin-hash"
  | "ambig-cross-hash"
  | "no-trail";
const dispoOf = new Map<Occ, Dispo>();
const collisionStats = new Map<string, number>();
for (const o of occs) {
  const cands = trailsByFileName.get(o.file)?.get(o.freshId) ?? [];
  let d: Dispo;
  if (cands.length === 0) d = "no-trail";
  else if (cands.length === 1) d = "unique-join";
  else {
    const hs = new Set(cands.map((c) => hashes[c.stmt]));
    d = hs.size === 1 ? "ambig-twin-hash" : "ambig-cross-hash";
    const minted = cands.every((c) => MINT.test(c.finalName));
    collisionStats.set(
      `${d}|${minted ? "minted" : "word"}|n=${Math.min(cands.length, 9)}`,
      (collisionStats.get(
        `${d}|${minted ? "minted" : "word"}|n=${Math.min(cands.length, 9)}`
      ) ?? 0) + 1
    );
  }
  dispoOf.set(o, d);
}
const dc = new Map<Dispo, number>();
for (const o of occs)
  dc.set(dispoOf.get(o) as Dispo, (dc.get(dispoOf.get(o) as Dispo) ?? 0) + 1);
console.log(`\n=== B. churned occurrences: ${occs.length} ===`);
for (const [k, v] of [...dc.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(18)} ${String(v).padStart(6)}`);
console.log("  collision-set detail (disposition|shape|set size):");
for (const [k, v] of [...collisionStats.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 12))
  console.log(`    ${String(v).padStart(5)}  ${k}`);

// ── C. strict whole-pair ceilings ────────────────────────────────────────
const byPair = new Map<number, Occ[]>();
for (const o of occs) {
  const l = byPair.get(o.pairId) ?? [];
  l.push(o);
  byPair.set(o.pairId, l);
}
let twinSwapPairs = 0;
let ambigAllPairs = 0;
for (const [, list] of byPair) {
  const ds = list.map((o) => dispoOf.get(o));
  if (!ds.every((d) => d === "ambig-twin-hash" || d === "ambig-cross-hash"))
    continue;
  ambigAllPairs++;
  // twin-swap healable: every priorId already lives among the colliding
  // family's final names — identity redistribution alone would heal it.
  const healable = list.every((o) => {
    const cands = trailsByFileName.get(o.file)?.get(o.freshId) ?? [];
    if (cands.length < 2) return false;
    const family = trailsByFileName.get(o.file);
    return family?.has(o.priorId) ?? false;
  });
  if (healable) twinSwapPairs++;
}
console.log(`\n=== C. strict whole-pair ceilings (2 ledger ln per pair) ===`);
console.log(
  `  all-ambiguous pairs:              ${ambigAllPairs}  (${2 * ambigAllPairs} ln upper bound)`
);
console.log(
  `  identity-redistribution healable: ${twinSwapPairs}  (${2 * twinSwapPairs} ln)`
);

// ── D. are the llm-settled bindings in statements the matcher COULD have
//    matched? A statement whose rename-blind hash exists in the PRIOR
//    ledger had a byte-identical (modulo names) counterpart — its
//    identifiers reaching a fresh ask is a matcher abstention (ambiguous
//    family / cross-assignment), not new code.
const priorLedger = JSON.parse(
  fs.readFileSync(
    path.join(OUT_ROOT, "../2.1.85-rebased/.humanify/split-ledger.json"),
    "utf8"
  )
);
const priorHashCount = new Map<string, number>();
for (const h of priorLedger.hashes as string[])
  priorHashCount.set(h, (priorHashCount.get(h) ?? 0) + 1);
const freshHashCount = new Map<string, number>();
for (const h of hashes) freshHashCount.set(h, (freshHashCount.get(h) ?? 0) + 1);
const dCluster = new Map<string, number>();
for (const tr of allTrails) {
  if (tr.terminalBy !== "llm") continue;
  const h = hashes[tr.stmt];
  const p = priorHashCount.get(h) ?? 0;
  const f = freshHashCount.get(h) ?? 0;
  let cls: string;
  if (p === 0) cls = "stmt-hash-new (legit fresh ask)";
  else if (p === 1 && f === 1) cls = "stmt-hash 1:1 in BOTH (matcher miss)";
  else
    cls = `stmt-hash ambiguous family (prior=${Math.min(p, 5)}+,fresh=${Math.min(f, 5)}+)`;
  dCluster.set(cls, (dCluster.get(cls) ?? 0) + 1);
}
console.log(`\n=== D. llm-settled bindings vs statement-hash landscape ===`);
for (const [k, v] of [...dCluster.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 10))
  console.log(`  ${String(v).padStart(5)}  ${k}`);

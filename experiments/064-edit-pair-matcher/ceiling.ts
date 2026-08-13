/**
 * 064 Task 0 — strict pre-code ceiling for the edit-pair suggestion tier.
 *
 *   npx tsx experiments/064-edit-pair-matcher/ceiling.ts \
 *     <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot> [label]
 *
 * The lever under consideration: pair hash-flipped statements by masked
 * head + token overlap (the linkage diff-composition step 3 already
 * computes), then feed the positional prior names of clean name-only
 * lines as SUGGESTIONS for bindings that reach the LLM with a BARE
 * trail. exp063's lesson, applied before any code: occurrence censuses
 * overstate — only whole line-pairs heal, and only when EVERY churned
 * identifier on the pair is one the lever can fix.
 *
 * A pair counts toward the ceiling iff every churned identifier on it:
 *   1. joins cleanly (exactly one candidate trail, file-scoped) — an
 *      ambiguous join cannot be suggested with per-binding precision;
 *   2. settled at the LLM tier with a BARE trail (no votes — bindings
 *      WITH votes belong to exp061(b)'s existing channel);
 *   3. maps fresh→prior unanimously across the whole tree's churn
 *      (the same fresh id churning from two different prior ids is the
 *      exp057 FIFO-artifact signature — refused);
 *   4. has a prior name that is not below-floor (never suggested).
 *
 * Reported split by ask kind (diag.renamed functionId): only the
 * module-binding batch HAS a suggestedName channel today; function-ask
 * bindings would need new prompt surface, so their share is priced
 * separately.
 *
 * The join machinery is exp061's loc-provenance statement-scoped join,
 * trimmed to this question (originals unchanged, rule 4: same
 * instruments, no proxies).
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
    "usage: ceiling.ts <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot> [label]"
  );
  process.exit(1);
}

// ── artifacts + statement spans (verified method: trail locs are valid
// line numbers in the saved humanified bundle) ────────────────────────────
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
  bare: boolean;
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
  const lastIdx = (tr.trail ?? []).lastIndexOf(last);
  const list = trailsByStmt.get(k) ?? [];
  list.push({
    oldName: tr.oldName,
    finalName: last.newName,
    terminalBy: tr.terminalBy ?? tr.settledBy ?? "unknown",
    bare: (tr.trail ?? []).slice(0, lastIdx).length === 0
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

// ── ask kind per (oldName -> finalName) ───────────────────────────────────
const askByRename = new Map<string, string>();
for (const r of diag.renamed ?? []) {
  askByRename.set(
    `${r.name}→${r.newName}`,
    String(r.functionId ?? "").startsWith("module-binding-batch:")
      ? "module-batch"
      : "function-ask"
  );
}

// ── churned pairs from the REAL samples ───────────────────────────────────
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

// ── gate 3: unanimity of fresh→prior across the tree ──────────────────────
const mappings = new Map<string, Set<string>>();
for (const o of occs) {
  const s = mappings.get(`${o.file}|${o.freshId}`) ?? new Set();
  s.add(o.priorId);
  mappings.set(`${o.file}|${o.freshId}`, s);
}

// ── per-occurrence disposition ────────────────────────────────────────────
type Disp =
  | "bare-llm-module"
  | "bare-llm-function"
  | "llm-with-votes"
  | "not-llm"
  | "ambiguous-join"
  | "non-unanimous"
  | "below-floor-prior";
function disposition(o: Occ): Disp {
  if ((mappings.get(`${o.file}|${o.freshId}`)?.size ?? 0) !== 1)
    return "non-unanimous";
  if (isBelowFloorName(o.priorId)) return "below-floor-prior";
  const ks = stmtsByFile.get(o.file) ?? [];
  const cands = ks
    .flatMap((k) => trailsByStmt.get(k) ?? [])
    .filter((tr) => tr.finalName === o.freshId);
  if (cands.length !== 1) return "ambiguous-join";
  const tr = cands[0];
  if (tr.terminalBy !== "llm") return "not-llm";
  if (!tr.bare) return "llm-with-votes";
  const ask = askByRename.get(`${tr.oldName}→${tr.finalName}`);
  return ask === "module-batch" ? "bare-llm-module" : "bare-llm-function";
}

const dispByOcc = new Map<Occ, Disp>();
for (const o of occs) dispByOcc.set(o, disposition(o));

// ── pair-level ceiling ────────────────────────────────────────────────────
const byPair = new Map<number, Occ[]>();
for (const o of occs) {
  const l = byPair.get(o.pairId) ?? [];
  l.push(o);
  byPair.set(o.pairId, l);
}
let pairsAll = 0;
let healModule = 0;
let healEither = 0;
const dispCount = new Map<Disp, number>();
for (const [, list] of byPair) {
  pairsAll++;
  const ds = list.map((o) => dispByOcc.get(o) as Disp);
  for (const d of ds) dispCount.set(d, (dispCount.get(d) ?? 0) + 1);
  if (ds.every((d) => d === "bare-llm-module")) healModule++;
  if (ds.every((d) => d === "bare-llm-module" || d === "bare-llm-function"))
    healEither++;
}
console.log(`=== 064 strict ceiling — ${LABEL || FRESH} ===`);
console.log(`churned pairs: ${pairsAll} (${2 * pairsAll} ledger lines)`);
console.log("occurrence dispositions:");
for (const [k, v] of [...dispCount.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(20)} ${String(v).padStart(6)}`);
console.log(
  `pairs fully healable via EXISTING module-batch channel: ${healModule} (${2 * healModule} lines)`
);
console.log(
  `pairs fully healable if function asks got a channel too: ${healEither} (${2 * healEither} lines)`
);
console.log(`ROW|${LABEL}|064ceiling|${pairsAll}|${healModule}|${healEither}`);

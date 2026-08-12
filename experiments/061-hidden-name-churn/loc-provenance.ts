/**
 * 061 Task 0b — loc-level provenance join.
 *
 *   npx tsx experiments/061-hidden-name-churn/loc-provenance.ts \
 *     <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot>
 *
 * The name-level join (tier-provenance.ts) left 27.3% of churned occurrences
 * unmatched and attributed exact-match's 13.9% mostly to join ambiguity. This
 * join is statement-scoped instead of name-scoped, using three facts verified
 * against the saved artifacts:
 *
 *   1. Strategy-trail locs are parse-time positions in the beautified bundle,
 *      and renames do NOT reflow lines — trail line numbers are valid line
 *      numbers in the saved `.humanify/humanified.js` (verified at line
 *      428,797 of the 85→86 r1 run). A validation pass re-checks this per run
 *      and aborts if the settled name is absent from its loc line too often.
 *   2. The split-ledger's `order[]`/`emitNames[]` are parallel to the wrapper
 *      IIFE's body statements (validateLedger in cjs-emit.ts), and emitted
 *      statements are exact byte slices of the bundle — so a fresh-side
 *      statement text can be matched back to its bundle statement index.
 *   3. Therefore: churned line → sample statement → bundle statement k →
 *      the trails whose loc line falls inside k's span → the trail whose
 *      final applied name equals the churned fresh identifier.
 *
 * Attribution is by terminalBy (the LAST applied strategy — post passes
 * legitimately re-decide), with the settledBy distribution printed alongside
 * for comparison with the Task 0 table.
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
    "usage: loc-provenance.ts <priorSrc> <freshSrc> <freshDiag.json> <freshOutRoot>"
  );
  process.exit(1);
}

// ── artifacts ─────────────────────────────────────────────────────────────
const ledger = JSON.parse(
  fs.readFileSync(path.join(OUT_ROOT, ".humanify/split-ledger.json"), "utf8")
);
const order: string[] = ledger.order;
const bundleCode = fs.readFileSync(
  path.join(OUT_ROOT, ".humanify/humanified.js"),
  "utf8"
);
const bundleLines = bundleCode.split("\n");
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8"));

// ── 1. statement spans: the unique block whose length matches the ledger ──
const ast = parse(bundleCode, {
  sourceType: "unambiguous",
  errorRecovery: false
});
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
  console.error(
    `expected exactly one block with ${order.length} statements, found ${blocks.length}`
  );
  process.exit(1);
}
const body = blocks[0];
const spans = body.map((s) => ({
  start: s.loc?.start.line ?? -1,
  end: s.loc?.end.line ?? -1
}));

/** line → statement index (spans are ordered, non-overlapping). */
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

// ── 2. bucket trails by statement, with a validation pass ─────────────────
interface TrailRef {
  oldName: string;
  finalName: string;
  settledBy: string;
  terminalBy: string;
  llmTouched: boolean;
  /** every attempt before the terminal applied one */
  preAttempts: {
    strategy: string;
    outcome: string;
    newName?: string;
    reason?: string;
  }[];
}
const trailsByStmt = new Map<number, TrailRef[]>();
let trailsTotal = 0;
let trailsPlaced = 0;
let locLineHasName = 0;
let locLineChecked = 0;
for (const tr of diag.strategyTrails.trails) {
  trailsTotal++;
  const applied = (tr.trail ?? []).filter(
    (a: { outcome: string }) => a.outcome === "applied"
  );
  const last = applied[applied.length - 1];
  if (!last?.newName) continue;
  const m = /^(\d+):/.exec(tr.loc ?? "");
  if (!m) continue;
  const line = Number(m[1]);
  const k = stmtAtLine(line);
  if (k < 0) continue;
  trailsPlaced++;
  if (locLineChecked < 5000) {
    locLineChecked++;
    const re = new RegExp(
      `\\b${last.newName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`
    );
    if (re.test(bundleLines[line - 1] ?? "")) locLineHasName++;
  }
  const list = trailsByStmt.get(k) ?? [];
  const lastIdx = (tr.trail ?? []).lastIndexOf(last);
  list.push({
    oldName: tr.oldName,
    finalName: last.newName,
    settledBy: tr.settledBy ?? "unknown",
    terminalBy: tr.terminalBy ?? tr.settledBy ?? "unknown",
    llmTouched: (tr.trail ?? []).some(
      (a: { strategy: string }) => a.strategy === "llm"
    ),
    preAttempts: (tr.trail ?? []).slice(0, lastIdx)
  });
  trailsByStmt.set(k, list);
}
const locValidPct = (100 * locLineHasName) / Math.max(1, locLineChecked);
console.log(
  `trails: ${trailsTotal} total, ${trailsPlaced} placed into statements; ` +
    `loc-line validation: ${locValidPct.toFixed(1)}% of ${locLineChecked} sampled locs contain the settled name`
);
if (locValidPct < 90) {
  console.error("loc mapping validation FAILED (<90%) — aborting join");
  process.exit(1);
}

// ── 3. statement text index per file (normalized slices) ──────────────────
function normalize(text: string): string {
  return text
    .split("\n")
    .map((l) => l.trim())
    .join("\n")
    .trim();
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
const sliceCache = new Map<number, string>();
function stmtSliceNorm(k: number): string {
  let s = sliceCache.get(k);
  if (s === undefined) {
    const stmt = body[k];
    const raw =
      stmt.start != null && stmt.end != null
        ? bundleCode.slice(stmt.start, stmt.end)
        : "";
    s = normalize(raw);
    sliceCache.set(k, s);
  }
  return s;
}
const textIndexByFile = new Map<string, Map<string, number[]>>();
for (const [f, ks] of stmtsByFile) {
  const idx = new Map<string, number[]>();
  for (const k of ks) {
    const key = stmtSliceNorm(k);
    const l = idx.get(key) ?? [];
    l.push(k);
    idx.set(key, l);
  }
  textIndexByFile.set(f, idx);
}

// ── 4. churned occurrences from the REAL samples ──────────────────────────
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

interface Occ {
  /** one entry per paired churned line; occurrences sharing a pair share it */
  pairId: number;
  file: string;
  freshId: string;
  priorId: string;
  property: boolean;
  /** the churned line is a `require(...)` line — same path (the predicate
   * forces non-ident tokens identical), so this is pure import-alias drift */
  requireLine: boolean;
  freshLine: string;
  freshText: string;
}
const occs: Occ[] = [];
let nextPairId = 0;
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a);
    const tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const diffs: { i: number; prior: string; fresh: string }[] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text)
        diffs.push({ i, prior: ta[i].text, fresh: tb[i].text });
    }
    if (!ok) continue;
    if (diffs.length > 0) nextPairId++;
    for (const d of diffs) {
      const prev = tb[d.i - 1];
      occs.push({
        pairId: nextPairId,
        file: s.file,
        freshId: d.fresh,
        priorId: d.prior,
        property: !!prev && prev.kind !== "ident" && /\.\s*$/.test(prev.text),
        requireLine: /\brequire\s*\(/.test(b),
        freshLine: b,
        freshText: s.freshText
      });
    }
  }
}
console.log(`churned identifier occurrences: ${occs.length}`);

// ── 5. the join ───────────────────────────────────────────────────────────
const stem = (n: string) => n.replace(/\d+$/, "");
type JoinTier =
  | "stmt-exact"
  | "stmt-stem"
  | "file-exact"
  | "file-stem"
  | "global-exact"
  | "unmatched-property"
  | "unmatched";
const joinCounts = new Map<JoinTier, number>();
const tierBySettled = new Map<string, number>();
const tierByTerminal = new Map<string, number>();
const llmTouchedWeight = { yes: 0, no: 0 };
const bump = (m: Map<string, number>, k: string, w: number) =>
  m.set(k, (m.get(k) ?? 0) + w);

// global name index (fallback, mirrors the Task 0 join)
const globalByName = new Map<string, TrailRef[]>();
for (const list of trailsByStmt.values())
  for (const tr of list) {
    const l = globalByName.get(tr.finalName) ?? [];
    l.push(tr);
    globalByName.set(tr.finalName, l);
  }

const unmatchedExamples: string[] = [];
const globalExamples: string[] = [];
const cleanJoins: { occ: Occ; trail: TrailRef }[] = [];
const shapeByTier = new Map<string, Map<string, number>>();
const shapeOf = (o: Occ): string => {
  if (o.requireLine) return "require-alias";
  if (/^lib_[0-9a-f]+_?\d*$/.test(o.freshId)) return "lib-prefix";
  if (o.property) return "member-prop";
  return "plain";
};
const bumpShape = (tier: string, o: Occ) => {
  const m = shapeByTier.get(tier) ?? new Map<string, number>();
  m.set(shapeOf(o), (m.get(shapeOf(o)) ?? 0) + 1);
  shapeByTier.set(tier, m);
};
for (const o of occs) {
  // resolve the sample statement to bundle statement candidates
  const norm = normalize(o.freshText);
  const stmtKs = textIndexByFile.get(o.file)?.get(norm) ?? [];
  const stmtTrails = stmtKs.flatMap((k) => trailsByStmt.get(k) ?? []);
  const fileKs = stmtsByFile.get(o.file) ?? [];
  const pick = (cands: TrailRef[], tier: JoinTier): boolean => {
    if (cands.length === 0) return false;
    joinCounts.set(tier, (joinCounts.get(tier) ?? 0) + 1);
    bumpShape(tier, o);
    if (cands.length === 1) cleanJoins.push({ occ: o, trail: cands[0] });
    if (tier === "global-exact" && globalExamples.length < 12)
      globalExamples.push(
        `${o.file}: ${o.priorId} -> ${o.freshId} [${cands.length} trails] :: ${o.freshLine.trim().slice(0, 90)}`
      );
    for (const c of cands) {
      bump(tierBySettled, c.settledBy, 1 / cands.length);
      bump(tierByTerminal, c.terminalBy, 1 / cands.length);
      if (c.llmTouched) llmTouchedWeight.yes += 1 / cands.length;
      else llmTouchedWeight.no += 1 / cands.length;
    }
    return true;
  };
  if (
    pick(
      stmtTrails.filter((tr) => tr.finalName === o.freshId),
      "stmt-exact"
    )
  )
    continue;
  if (
    pick(
      stmtTrails.filter((tr) => stem(tr.finalName) === stem(o.freshId)),
      "stmt-stem"
    )
  )
    continue;
  const fileTrails = fileKs.flatMap((k) => trailsByStmt.get(k) ?? []);
  if (
    pick(
      fileTrails.filter((tr) => tr.finalName === o.freshId),
      "file-exact"
    )
  )
    continue;
  if (
    pick(
      fileTrails.filter((tr) => stem(tr.finalName) === stem(o.freshId)),
      "file-stem"
    )
  )
    continue;
  if (pick(globalByName.get(o.freshId) ?? [], "global-exact")) continue;
  const tier: JoinTier = o.property ? "unmatched-property" : "unmatched";
  joinCounts.set(tier, (joinCounts.get(tier) ?? 0) + 1);
  bumpShape(tier, o);
  if (tier === "unmatched" && unmatchedExamples.length < 15)
    unmatchedExamples.push(
      `${o.file}: ${o.priorId} -> ${o.freshId} [${shapeOf(o)}] :: ${o.freshLine.trim().slice(0, 90)}`
    );
}

// sample→statement resolution quality
let resolved = 0;
for (const o of occs)
  if ((textIndexByFile.get(o.file)?.get(normalize(o.freshText)) ?? []).length)
    resolved++;
console.log(
  `sample→statement resolution: ${resolved}/${occs.length} occurrences (${((100 * resolved) / Math.max(1, occs.length)).toFixed(1)}%)`
);

const total = occs.length;
console.log("\njoin tiers (occurrences):");
for (const [k, v] of [...joinCounts.entries()].sort((a, b) => b[1] - a[1]))
  console.log(
    `  ${k.padEnd(20)} ${String(v).padStart(6)}  ${((100 * v) / total).toFixed(1)}%`
  );

const joined = [...tierByTerminal.values()].reduce((a, b) => a + b, 0);
console.log(
  `\ndecider tier — terminalBy (weighted, joined=${joined.toFixed(0)}):`
);
for (const [k, v] of [...tierByTerminal.entries()].sort((a, b) => b[1] - a[1]))
  console.log(
    `  ${k.padEnd(20)} ${v.toFixed(1).padStart(8)}  ${((100 * v) / total).toFixed(1)}% of all, ${((100 * v) / joined).toFixed(1)}% of joined`
  );
console.log("\nsettledBy (weighted) — comparison with Task 0 name-level join:");
for (const [k, v] of [...tierBySettled.entries()].sort((a, b) => b[1] - a[1]))
  console.log(
    `  ${k.padEnd(20)} ${v.toFixed(1).padStart(8)}  ${((100 * v) / total).toFixed(1)}%`
  );
console.log(
  `\nLLM appears anywhere in trail (weighted): yes ${llmTouchedWeight.yes.toFixed(1)}, no ${llmTouchedWeight.no.toFixed(1)}`
);
console.log("\nline/identifier shape by join tier:");
for (const [tier, m] of shapeByTier) {
  const parts = [...m.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  console.log(`  ${tier.padEnd(20)} ${parts}`);
}
// ── 6. fall-through census on cleanly-joined occurrences ──────────────────
// For each churned occurrence with exactly ONE candidate trail: did the
// cascade see the prior tree's name (the churn's prior side) before the
// terminal tier re-decided? A "prior-corroborating vote" is any pre-terminal
// attempt proposing exactly the prior identifier.
const fallthrough = new Map<
  string,
  {
    n: number;
    bare: number;
    priorVote: number;
    priorStemVote: number;
    otherVotes: number;
  }
>();
const priorVoteExamples: string[] = [];
for (const { occ, trail } of cleanJoins) {
  const e = fallthrough.get(trail.terminalBy) ?? {
    n: 0,
    bare: 0,
    priorVote: 0,
    priorStemVote: 0,
    otherVotes: 0
  };
  e.n++;
  if (trail.preAttempts.length === 0) e.bare++;
  else if (trail.preAttempts.some((a) => a.newName === occ.priorId)) {
    e.priorVote++;
    if (trail.terminalBy === "llm" && priorVoteExamples.length < 10)
      priorVoteExamples.push(
        `${occ.file}: ${occ.priorId} -> ${occ.freshId} (old ${trail.oldName}); pre: ${trail.preAttempts
          .map(
            (a) =>
              `${a.strategy}:${a.outcome}${a.newName ? `=${a.newName}` : ""}`
          )
          .join(" | ")
          .slice(0, 160)}`
      );
  } else if (
    trail.preAttempts.some(
      (a) => a.newName !== undefined && stem(a.newName) === stem(occ.priorId)
    )
  )
    e.priorStemVote++;
  else e.otherVotes++;
  fallthrough.set(trail.terminalBy, e);
}
console.log(
  `\nfall-through census (cleanly-joined occurrences, n=${cleanJoins.length}):`
);
console.log(
  "  terminal tier          n   bare-trail  prior-vote  prior-stem  other-votes"
);
for (const [k, v] of [...fallthrough.entries()].sort((a, b) => b[1].n - a[1].n))
  console.log(
    `  ${k.padEnd(20)} ${String(v.n).padStart(5)} ${String(v.bare).padStart(9)} ${String(v.priorVote).padStart(11)} ${String(v.priorStemVote).padStart(11)} ${String(v.otherVotes).padStart(12)}`
  );
console.log(
  "\nllm-terminal churned occurrences where a pre-LLM vote proposed the prior name:"
);
for (const e of priorVoteExamples) console.log(`  ${e}`);

// Among llm-terminal prior-vote(+stem) occurrences: which strategy cast the
// corroborating vote, what pin/abstain outcomes blocked it, and how many
// distinct names the votes proposed (1 = unanimous, the pin tiers' precondition).
const voteStrategy = new Map<string, number>();
const blockReason = new Map<string, number>();
const distinctVoteNames = new Map<number, number>();
let priorVoteLlm = 0;
for (const { occ, trail } of cleanJoins) {
  if (trail.terminalBy !== "llm") continue;
  const corr = trail.preAttempts.filter(
    (a) =>
      a.newName === occ.priorId ||
      (a.newName && stem(a.newName) === stem(occ.priorId))
  );
  if (corr.length === 0) continue;
  priorVoteLlm++;
  for (const a of corr) bump(voteStrategy, `${a.strategy}:${a.outcome}`, 1);
  for (const a of trail.preAttempts)
    if (a.outcome === "abstained")
      bump(blockReason, `${a.strategy}(${a.reason ?? "?"})`, 1);
  const names = new Set(
    trail.preAttempts.filter((a) => a.newName).map((a) => a.newName)
  );
  distinctVoteNames.set(
    names.size,
    (distinctVoteNames.get(names.size) ?? 0) + 1
  );
}
console.log(`\nprior-corroborated llm-terminal occurrences: ${priorVoteLlm}`);
console.log("  corroborating vote came from:");
for (const [k, v] of [...voteStrategy.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`    ${k.padEnd(28)} ${v}`);
console.log("  abstain outcomes present on those trails:");
for (const [k, v] of [...blockReason.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`    ${k.padEnd(28)} ${v}`);
console.log("  distinct names among the trail's votes:");
for (const [k, v] of [...distinctVoteNames.entries()].sort(
  (a, b) => a[0] - b[0]
))
  console.log(`    ${k} name(s): ${v}`);

// ── 7. offline ceiling for the vote-derived suggestion lever ──────────────
// Simulate: an unpinned binding that reaches the LLM gets a suggestedName
// derived from its recorded votes — unique top name ranked by (exact votes,
// then total votes); ties abstain; below-floor prior names excluded (the
// mint-poisoning class stays refused). Count churned occurrences where the
// simulated suggestion equals the prior side — the lever's best case
// assuming the LLM honors every hint. Then convert to whole line-pairs
// (the 055 ledger's unit): a pair heals only when EVERY churned identifier
// on it is fixed.
function simulateSuggestion(trail: TrailRef): string | null {
  const tallies = new Map<string, { exact: number; total: number }>();
  for (const a of trail.preAttempts) {
    if (a.outcome !== "vote" || !a.newName) continue;
    if (isBelowFloorName(a.newName)) continue;
    const e = tallies.get(a.newName) ?? { exact: 0, total: 0 };
    e.total++;
    if (a.strategy === "exact-match") e.exact++;
    tallies.set(a.newName, e);
  }
  let best: string | null = null;
  let bestKey: [number, number] = [-1, -1];
  let tied = false;
  for (const [name, c] of tallies) {
    const key: [number, number] = [c.exact, c.total];
    if (key[0] > bestKey[0] || (key[0] === bestKey[0] && key[1] > bestKey[1])) {
      best = name;
      bestKey = key;
      tied = false;
    } else if (key[0] === bestKey[0] && key[1] === bestKey[1]) {
      tied = true;
    }
  }
  return tied ? null : best;
}

type Disposition =
  | "fix-exact"
  | "fix-stem"
  | "hint-other"
  | "no-hint"
  | "not-llm"
  | "not-clean";
const dispositionByOcc = new Map<Occ, Disposition>();
for (const { occ, trail } of cleanJoins) {
  if (trail.terminalBy !== "llm") {
    dispositionByOcc.set(occ, "not-llm");
    continue;
  }
  const hint = simulateSuggestion(trail);
  if (hint === null) dispositionByOcc.set(occ, "no-hint");
  else if (hint === occ.priorId) dispositionByOcc.set(occ, "fix-exact");
  else if (stem(hint) === stem(occ.priorId))
    dispositionByOcc.set(occ, "fix-stem");
  else dispositionByOcc.set(occ, "hint-other");
}
const dispCount = new Map<Disposition, number>();
for (const o of occs) {
  const d = dispositionByOcc.get(o) ?? "not-clean";
  dispCount.set(d, (dispCount.get(d) ?? 0) + 1);
}
console.log("\nvote-suggestion lever — occurrence dispositions:");
for (const [k, v] of [...dispCount.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(12)} ${String(v).padStart(6)}`);

// line-pair ceiling: every occurrence on the pair must be fix-exact
const byPair = new Map<number, Occ[]>();
for (const o of occs) {
  const l = byPair.get(o.pairId) ?? [];
  l.push(o);
  byPair.set(o.pairId, l);
}
let pairsAll = 0;
let pairsHealed = 0;
let pairsHealedStem = 0;
for (const [, list] of byPair) {
  pairsAll++;
  const ds = list.map((o) => dispositionByOcc.get(o) ?? "not-clean");
  if (ds.every((d) => d === "fix-exact")) pairsHealed++;
  if (ds.every((d) => d === "fix-exact" || d === "fix-stem")) pairsHealedStem++;
}
console.log(
  `\nline-pair ceiling: ${pairsHealed} of ${pairsAll} churned pairs fully healed ` +
    `(${2 * pairsHealed} ledger lines; ${pairsHealedStem} pairs / ${2 * pairsHealedStem} lines counting stem-equal)`
);

// ── 8. which ASK produced the llm-terminal churn ──────────────────────────
// diag.renamed records every LLM rename with its functionId
// ("module-binding-batch:..." vs a function's id). Join by
// (oldName -> finalName) to classify the ask, crossed with vote presence.
const askByRename = new Map<string, string>();
for (const r of diag.renamed ?? []) {
  const kind = String(r.functionId ?? "").startsWith("module-binding-batch:")
    ? "module-batch"
    : "function-ask";
  askByRename.set(`${r.name}→${r.newName}`, kind);
}
const askMatrix = new Map<string, number>();
for (const { occ, trail } of cleanJoins) {
  if (trail.terminalBy !== "llm") continue;
  const ask =
    askByRename.get(`${trail.oldName}→${trail.finalName}`) ?? "no-renamed-row";
  const hasVotes = trail.preAttempts.some((a) => a.outcome === "vote");
  const corroborated = trail.preAttempts.some(
    (a) =>
      a.newName === occ.priorId ||
      (a.newName && stem(a.newName) === stem(occ.priorId))
  );
  const key = `${ask} / ${hasVotes ? (corroborated ? "votes-prior" : "votes-other") : "bare"}`;
  askMatrix.set(key, (askMatrix.get(key) ?? 0) + 1);
}
console.log("\nllm-terminal churn by ask kind × vote state (clean joins):");
for (const [k, v] of [...askMatrix.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(36)} ${String(v).padStart(5)}`);

console.log("\nglobal-exact examples (name found only OUTSIDE the file):");
for (const e of globalExamples) console.log(`  ${e}`);
console.log("\nunmatched (non-property) examples:");
for (const e of unmatchedExamples) console.log(`  ${e}`);

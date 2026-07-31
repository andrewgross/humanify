/**
 * 055 task 0 — how much of the REAL column is not real, and does the
 * decomposition add up at all?
 *
 *   npx tsx experiments/055-residual-recount/real-ledger.ts <priorSrc> <freshSrc> [label]
 *
 * Two questions, because the second decides whether to believe the first.
 *
 * ## A. What is inside REAL
 *
 * `diff-composition` calls a statement `naming` only when its `statementHash`
 * matches a prior statement's. A name-only line inside a statement whose hash
 * FLIPPED is charged to real change, where no noise KPI can see it. exp054
 * removed 5,026 git lines and the noise buckets accounted for 450 — the other
 * 4,576 came out of this column, which is the existence proof that it holds
 * name churn.
 *
 * So every REAL sample with both sides present is line-diffed and each charged
 * line put in one of:
 *
 *   NAME-ONLY   the two lines tokenize to the same token stream with every
 *               non-identifier token byte-identical, and at least one
 *               identifier differs. Uses `tokenizeLine` from diff-reconcile —
 *               the same tokenizer the pass itself uses, not a proxy (rule 4).
 *               A line it cannot tokenize self-contained counts as EDITED.
 *   EDITED      anything else: a changed literal, operator, property, call
 *               shape, or a differing token count.
 *
 * That predicate tests "these two lines differ in identifier tokens and in
 * nothing else". It does NOT test that the difference is a rename — a call
 * rerouted to a different helper reads identically, which is why task 1 reads
 * them by hand.
 *
 * Statements present on only one side (genuinely added or removed code) are
 * real by construction and are counted separately, never folded in.
 *
 * ## B. Does the decomposition add up
 *
 * Every bucket is compared against GROUND TRUTH: the line count GNU `diff`
 * prints for the same two files. exp049 found the decomposition matches git
 * only by CANCELLATION — 11.6% per-file error that cancels 19.6x in aggregate —
 * so this reports the aggregate identity AND the per-file error distribution,
 * because a total that matches says nothing about whether any single file's
 * attribution is trustworthy.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import {
  computeNormalDiff,
  tokenizeLine
} from "../../src/rename/diff-reconcile.js";
import { composeFile } from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: real-ledger.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

/** Lines a normal-format diff prints. */
function diffLines(a: string, b: string): number {
  let n = 0;
  for (const line of computeNormalDiff(a, b).split("\n")) {
    if (
      line.startsWith("< ") ||
      line.startsWith("> ") ||
      line === "<" ||
      line === ">"
    ) {
      n++;
    }
  }
  return n;
}

/** True when the two lines differ ONLY in identifier tokens. */
function nameOnly(a: string, b: string): boolean {
  const ta = tokenizeLine(a);
  const tb = tokenizeLine(b);
  if (!ta || !tb || ta.length !== tb.length) return false;
  let sawIdentDiff = false;
  for (let i = 0; i < ta.length; i++) {
    if (ta[i].kind !== tb[i].kind) return false;
    if (ta[i].text === tb[i].text) continue;
    if (ta[i].kind !== "ident") return false;
    sawIdentDiff = true;
  }
  return sawIdentDiff;
}

/**
 * The changed lines the TALLY charges for an edited pair, paired up.
 *
 * `editedLineCounts` — the function `diff-composition` actually bills with — is
 * SET-based, not LCS: a fresh line counts as changed when it appears nowhere in
 * the prior text, and vice versa. An LCS walk selects a different set of lines,
 * so attributing over one while the tally bills the other does not reconcile
 * (measured: 56,526 attributed against 25,441 billed). This partitions exactly
 * the billed lines, so the parts sum to the whole by construction.
 *
 * Pairing the two filtered lists positionally is an approximation — the tally
 * itself only counts them and never pairs them — so a mis-pair moves a line
 * between NAME-ONLY and EDITED but never changes the total.
 */
function billedPairs(
  priorText: string,
  freshText: string
): { pairs: [string, string][]; unpaired: number } {
  const freshLines = freshText.split("\n");
  const priorLines = priorText.split("\n");
  const priorSet = new Set(priorLines);
  const freshSet = new Set(freshLines);
  const added = freshLines.filter((l) => !priorSet.has(l));
  const removed = priorLines.filter((l) => !freshSet.has(l));
  const k = Math.min(added.length, removed.length);
  const pairs: [string, string][] = [];
  for (let i = 0; i < k; i++) pairs.push([removed[i], added[i]]);
  return { pairs, unpaired: added.length + removed.length - 2 * k };
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

// ── A. what is inside REAL ────────────────────────────────────────────────
const samples: NoiseSample[] = [];
const tally = composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

let nameOnlyLn = 0;
let editedLn = 0;
let oneSidedLn = 0;
const examples: string[] = [];
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) {
    oneSidedLn += s.lines;
    continue;
  }
  const billed = billedPairs(s.priorText, s.freshText);
  editedLn += billed.unpaired; // a line with no counterpart is real change
  for (const [a, b] of billed.pairs) {
    if (nameOnly(a, b)) {
      nameOnlyLn += 2; // a changed line costs a delete and an add
      if (examples.length < 25) {
        examples.push(
          `${s.file}\n      - ${a.trim().slice(0, 150)}\n      + ${b.trim().slice(0, 150)}`
        );
      }
    } else editedLn += 2;
  }
}

// ── B. does it add up ─────────────────────────────────────────────────────
const priorFiles = new Set(walk(PRIOR));
let truth = 0;
let modelled = 0;
let absErr = 0;
const perFileErr: number[] = [];
for (const f of walk(FRESH)) {
  if (!priorFiles.has(f)) continue;
  const a = fs.readFileSync(path.join(PRIOR, f), "utf8");
  const b = fs.readFileSync(path.join(FRESH, f), "utf8");
  if (a === b) continue;
  const g = diffLines(a, b);
  const t = composeFile(a, b);
  const m = t.real + t.naming + t.alias + t.reorder + t.fileAddRemove;
  truth += g;
  modelled += m;
  absErr += Math.abs(m - g);
  if (g > 0) perFileErr.push(Math.abs(m - g) / g);
}
perFileErr.sort((x, y) => x - y);
const median = perFileErr[Math.floor(perFileErr.length / 2)] ?? 0;
const p90 = perFileErr[Math.floor(perFileErr.length * 0.9)] ?? 0;

const pad = (n: number, w = 9) => String(n).padStart(w);
const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%` : "n/a";

console.log(
  `=== 055 REAL-COLUMN LEDGER — ${LABEL || `${PRIOR} -> ${FRESH}`} ===`
);
console.log(`  composeDiff REAL total          ${pad(tally.real)}`);
console.log(
  `    NAME-ONLY (hidden noise)      ${pad(nameOnlyLn)}  ${pct(nameOnlyLn, tally.real)}`
);
console.log(
  `    EDITED (genuine)              ${pad(editedLn)}  ${pct(editedLn, tally.real)}`
);
console.log(
  `    one-sided add/remove          ${pad(oneSidedLn)}  ${pct(oneSidedLn, tally.real)}`
);
console.log(
  `  classified noise                ${pad(tally.naming + tally.alias + tally.reorder)}`
);
console.log(
  `ROW|${LABEL}|real|${tally.real}|${nameOnlyLn}|${editedLn}|${oneSidedLn}|${tally.naming + tally.alias + tally.reorder}`
);
console.log("");
console.log(`  === accounting vs GNU diff ===`);
console.log(`  ground truth (diff)             ${pad(truth)}`);
console.log(
  `  decomposition total             ${pad(modelled)}   (${pct(modelled - truth, truth)} off)`
);
console.log(
  `  sum of |per-file error|         ${pad(absErr)}   cancels ${truth ? (absErr / Math.max(1, Math.abs(modelled - truth))).toFixed(1) : "n/a"}x`
);
console.log(
  `  per-file relative error: median ${(100 * median).toFixed(1)}%, p90 ${(100 * p90).toFixed(1)}%, files ${perFileErr.length}`
);
console.log(
  `ROW|${LABEL}|accounting|${truth}|${modelled}|${absErr}|${(100 * median).toFixed(1)}|${(100 * p90).toFixed(1)}`
);
console.log(`\n  NAME-ONLY lines inside REAL statements — first 25:`);
for (const e of examples) console.log(`    ${e}`);

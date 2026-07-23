/**
 * exp036 task A — ceiling: decompose the diff-ledger's family-bucket
 * noise into what each idea can actually recover.
 *
 *   zeroable      — a prior member with the SAME canonical occurrence
 *                   signature exists (multiset matching): an optimal
 *                   assignment reproduces the prior byte-exactly.
 *                   Idea 1's ceiling.
 *   decoration    — nearest mate differs only in same-stem /
 *                   digit-tail decorations (ValVal, _2, fsPromises57).
 *                   Idea 3's ceiling.
 *   vendor-number — nearest mate differs only in numbered vendor-copy
 *                   names (React124 vs ReactJSX4). Idea 4's ceiling.
 *   name-churn    — the pool's name INVENTORY changed (synonym
 *                   re-draws: importSummary vs importResult). Reachable
 *                   only by idea 2-style prior pinning, not assignment.
 *   membership    — bucket counts differ between sides; the surplus is
 *                   partly real change. Not fixable by assignment.
 *
 * Signatures are conservative: WORD tokens in property position stay
 * verbatim (renames cannot touch them); all other WORD tokens get
 * first-occurrence canonical numbers. Words inside string literals get
 * canonicalized symmetrically on both sides (same-hash members share
 * literals, so this can only UNDERSTATE the zeroable class — safe
 * direction for a ceiling).
 *
 *   npx tsx ceiling-family-assignment.ts <fresh.js> <prior.js> [pairLabel]
 */
import * as fs from "node:fs";
import { nameStem } from "../../src/rename/prior-name-snap.js";
import { type Stmt, statementsOf } from "../034-eval-harness/statements.js";

const WORD = /[A-Za-z_$][\w$]*/g;
const fmt = (n: number) => n.toLocaleString("en-US");

interface Tok {
  words: string[];
  seps: string[];
}

function tokenize(text: string): Tok {
  const words: string[] = [];
  const seps: string[] = [];
  let last = 0;
  for (const m of text.matchAll(WORD)) {
    seps.push(text.slice(last, m.index));
    words.push(m[0]);
    last = (m.index ?? 0) + m[0].length;
  }
  seps.push(text.slice(last));
  return { words, seps };
}

function isPropertyPosition(tok: Tok, i: number): boolean {
  const before = tok.seps[i].trimEnd();
  if (before.endsWith(".") || before.endsWith("?.")) return true;
  const after = tok.seps[i + 1].trimStart();
  if (after.startsWith(":") && !after.startsWith("::")) return true;
  return false;
}

/** Canonical occurrence signature: property words verbatim, everything
 * else numbered by first occurrence, separators verbatim. */
function signature(text: string): string {
  const tok = tokenize(text);
  const index = new Map<string, number>();
  const parts: string[] = [];
  for (let i = 0; i < tok.words.length; i++) {
    parts.push(tok.seps[i]);
    const w = tok.words[i];
    if (isPropertyPosition(tok, i)) {
      parts.push(w);
    } else {
      let n = index.get(w);
      if (n === undefined) {
        n = index.size;
        index.set(w, n);
      }
      parts.push(`#${n}`);
    }
  }
  parts.push(tok.seps[tok.words.length]);
  return parts.join("");
}

function editedLines(a: Stmt, b: Stmt): number {
  const bSet = new Set(b.text.split("\n"));
  let n = 0;
  for (const line of a.text.split("\n")) if (!bSet.has(line)) n++;
  return n;
}

const digitTailEqual = (a: string, b: string) =>
  a.replace(/[\d_]+$/, "") === b.replace(/[\d_]+$/, "") &&
  a.replace(/[\d_]+$/, "").length > 0;

const stemEqual = (a: string, b: string) => {
  const sa = nameStem(a);
  return sa.length > 0 && sa === nameStem(b);
};

function vendorNumbered(a: string, b: string): boolean {
  if (!/^[A-Za-z_$][\w$]*\d+$/.test(a) || !/^[A-Za-z_$][\w$]*\d+$/.test(b)) {
    return false;
  }
  const pa = a.toLowerCase().replace(/\d+$/, "");
  const pb = b.toLowerCase().replace(/\d+$/, "");
  const shared =
    pa.length >= 4 &&
    (pa.startsWith(pb.slice(0, 4)) || pb.startsWith(pa.slice(0, 4)));
  return shared;
}

/** Classify a non-zeroable member against its nearest mate by the kinds
 * of word flips between them. */
function flipClass(fresh: Stmt, mate: Stmt): string {
  const ft = tokenize(fresh.text);
  const mt = tokenize(mate.text);
  if (ft.words.length !== mt.words.length) return "name-churn";
  let decoration = 0;
  let vendor = 0;
  let other = 0;
  for (let i = 0; i < ft.words.length; i++) {
    const a = ft.words[i];
    const b = mt.words[i];
    if (a === b) continue;
    if (stemEqual(a, b) || digitTailEqual(a, b)) decoration++;
    else if (vendorNumbered(a, b)) vendor++;
    else other++;
  }
  if (other > 0) return "name-churn";
  if (vendor > 0) return "vendor-number";
  if (decoration > 0) return "decoration";
  return "name-churn";
}

function main(): void {
  const [freshPath, priorPath, label] = process.argv.slice(2);
  if (!freshPath || !priorPath) {
    console.error(
      "usage: ceiling-family-assignment.ts <fresh.js> <prior.js> [pairLabel]"
    );
    process.exit(1);
  }
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const freshByHash = new Map<string, Stmt[]>();
  for (const s of fresh) {
    const list = freshByHash.get(s.hash) ?? [];
    if (list.length === 0) freshByHash.set(s.hash, list);
    list.push(s);
  }

  const classes = new Map<string, { st: number; editedLn: number }>();
  const samples = new Map<string, string[]>();
  const bump = (cls: string, editedLn: number, sample?: string) => {
    const e = classes.get(cls) ?? { st: 0, editedLn: 0 };
    e.st++;
    e.editedLn += editedLn;
    classes.set(cls, e);
    if (sample) {
      const list = samples.get(cls) ?? [];
      if (list.length < 5) list.push(sample);
      samples.set(cls, list);
    }
  };

  let totalSt = 0;
  let totalEditedLn = 0;

  for (const [hash, members] of freshByHash) {
    const twins = priorByHash.get(hash);
    if (!twins) continue;
    const isFamily = members.length > 1 || twins.length > 1;
    if (!isFamily) continue;

    // Proper multiset byte-matching first: clean members consume twins.
    const pool = [...twins];
    const noisy: Stmt[] = [];
    for (const m of members) {
      const i = pool.findIndex((t) => t.text === m.text);
      if (i >= 0) pool.splice(i, 1);
      else noisy.push(m);
    }
    if (noisy.length === 0) continue;

    // Multiset signature matching over the REMAINING pool = zeroable.
    const bySig = new Map<string, number>();
    for (const t of pool) {
      const sig = signature(t.text);
      bySig.set(sig, (bySig.get(sig) ?? 0) + 1);
    }
    for (const m of noisy) {
      totalSt++;
      // Nearest mate by edited lines approximates the current cost.
      let mate: Stmt | null = null;
      let cost = m.lines;
      for (const t of twins) {
        const e = editedLines(m, t);
        if (e < cost || mate === null) {
          cost = e;
          mate = t;
        }
      }
      totalEditedLn += cost;

      if (pool.length === 0) {
        bump("membership", cost, headOf(m));
        continue;
      }
      const sig = signature(m.text);
      const available = bySig.get(sig) ?? 0;
      if (available > 0) {
        bySig.set(sig, available - 1);
        bump("zeroable", cost, headOf(m));
        continue;
      }
      bump(mate ? flipClass(m, mate) : "membership", cost, headOf(m));
    }
  }

  // Token-level flip tally across ALL family-noise nearest-mate pairs —
  // decoration and vendor flips hide inside mixed statements, so their
  // honest ceilings are per-flip, not per-statement.
  const flipTally = { decoration: 0, vendor: 0, other: 0 };
  for (const [hash, members] of freshByHash) {
    const twins = priorByHash.get(hash);
    if (!twins || (members.length <= 1 && twins.length <= 1)) continue;
    for (const m of members) {
      if (twins.some((t) => t.text === m.text)) continue;
      let mate: Stmt | null = null;
      let cost = Number.POSITIVE_INFINITY;
      for (const t of twins) {
        const e = editedLines(m, t);
        if (e < cost) {
          cost = e;
          mate = t;
        }
      }
      if (!mate) continue;
      const ft = tokenize(m.text);
      const mt = tokenize(mate.text);
      if (ft.words.length !== mt.words.length) continue;
      for (let i = 0; i < ft.words.length; i++) {
        const a = ft.words[i];
        const b = mt.words[i];
        if (a === b) continue;
        if (stemEqual(a, b) || digitTailEqual(a, b)) flipTally.decoration++;
        else if (vendorNumbered(a, b)) flipTally.vendor++;
        else flipTally.other++;
      }
    }
  }

  const order = [
    "zeroable",
    "decoration",
    "vendor-number",
    "name-churn",
    "membership"
  ];
  console.log(`=== family-assignment ceiling${label ? ` (${label})` : ""} ===`);
  console.log(
    `  ${"TOTAL family-noise".padEnd(22)} ${fmt(totalSt).padStart(6)} st  ${fmt(totalEditedLn).padStart(7)} editedLn  100.00%`
  );
  for (const cls of order) {
    const e = classes.get(cls);
    if (!e) continue;
    const pct = totalEditedLn
      ? ((100 * e.editedLn) / totalEditedLn).toFixed(2)
      : "0.00";
    console.log(
      `  ${cls.padEnd(22)} ${fmt(e.st).padStart(6)} st  ${fmt(e.editedLn).padStart(7)} editedLn  ${pct.padStart(6)}%`
    );
  }
  console.log(
    `\n  flip pairs vs nearest mate: decoration ${fmt(flipTally.decoration)} · vendor-numbered ${fmt(flipTally.vendor)} · other ${fmt(flipTally.other)}`
  );
  for (const cls of order) {
    const list = samples.get(cls);
    if (!list?.length) continue;
    console.log(`\n  -- ${cls} samples --`);
    for (const s of list) console.log(`     ${s}`);
  }
}

function headOf(s: Stmt): string {
  return s.text.split("\n")[0].slice(0, 110);
}

main();

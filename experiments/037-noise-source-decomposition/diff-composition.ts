/**
 * Diff composition (exp037): decompose the REAL on-disk git diff of a split tree
 * into real change vs each noise mechanism, in GIT LINE units so the parts sum to
 * roughly the churn a human sees.
 *
 * Per common file, top-level statements are matched three ways:
 *   1. exact (hash+text) present on both sides  -> unchanged content. If it sits
 *      at a different position it still churns: REORDER (LCS over the common
 *      subsequence; anything off the LCS is displaced).
 *   2. same hash, different text -> pure NAMING churn (structure identical modulo
 *      renaming). Charged the ACTUAL differing line count (line-level LCS between
 *      the two statement texts), not whole-statement mass. Split out further:
 *      ALIAS churn when the statement is a `const x = require("...")` header line.
 *   3. hash present on only one side -> REAL change (added or removed lines).
 * Files present on only one side are counted whole (added/removed).
 *
 * Usage: npx tsx diff-composition.ts <priorSrcDir> <freshSrcDir> [label]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  editedLineCounts,
  maskedHead,
  tokenSet
} from "../034-eval-harness/diff-ledger.js";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: string[];
  isRequire: boolean;
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** `const x = require("...")` — the import header lines, where a changed alias
 * with an unchanged path is pure alias churn. */
function isRequireDecl(s: t.Statement): boolean {
  if (!t.isVariableDeclaration(s) || s.declarations.length !== 1) return false;
  const init = s.declarations[0].init;
  return (
    t.isCallExpression(init) &&
    t.isIdentifier(init.callee, { name: "require" }) &&
    init.arguments.length === 1 &&
    t.isStringLiteral(init.arguments[0])
  );
}

function statementsOf(code: string): Stmt[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  return ast.program.body.map((s) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text.length ? text.split("\n") : [],
      isRequire: isRequireDecl(s)
    };
  });
}

/** Length of the longest common subsequence of two line arrays (rolling DP). */
function lcsLen(a: string[], b: string[]): number {
  if (a.length === 0 || b.length === 0) return 0;
  // Guard pathological sizes: fall back to multiset intersection (a lower bound
  // on LCS, so it slightly OVER-counts churn for giant statements).
  if (a.length * b.length > 25_000_000) {
    const counts = new Map<string, number>();
    for (const l of b) counts.set(l, (counts.get(l) ?? 0) + 1);
    let common = 0;
    for (const l of a) {
      const n = counts.get(l) ?? 0;
      if (n > 0) {
        common++;
        counts.set(l, n - 1);
      }
    }
    return common;
  }
  let prev = new Array<number>(b.length + 1).fill(0);
  let cur = new Array<number>(b.length + 1).fill(0);
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      cur[j] =
        a[i - 1] === b[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], cur[j - 1]);
    }
    [prev, cur] = [cur, prev];
    cur.fill(0);
  }
  return prev[b.length];
}

/** git-style churn between two texts: added + deleted lines. */
function lineChurn(a: string[], b: string[]): number {
  return a.length + b.length - 2 * lcsLen(a, b);
}

/** Indices of `fresh` on the LCS of the two key sequences (order-stable ones). */
function onLcs(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  if (n === 0 || m === 0 || n * m > 25_000_000) {
    return new Set(fresh.map((_, i) => i)); // give up: treat as in-order
  }
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      dp[i][j] =
        prior[i - 1] === fresh[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
    }
  }
  const keep = new Set<number>();
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (prior[i - 1] === fresh[j - 1]) {
      keep.add(j - 1);
      i--;
      j--;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) i--;
    else j--;
  }
  return keep;
}

/** Git-line churn of a split tree diff, split by what caused each line. */
export interface Tally {
  real: number;
  naming: number;
  alias: number;
  reorder: number;
  fileAddRemove: number;
}

/**
 * One noise instance, kept so the diff can be READ and not just totalled.
 *
 * The tallies below are the only thing the eval consumes, and a total cannot
 * show WHAT the noise is — which is the question anyone reviewing a release
 * actually asks. Collection is opt-in and never touches a tally, so scoring is
 * unaffected whether or not a sink is passed.
 */
export interface NoiseSample {
  kind: "reorder" | "naming" | "alias";
  file: string;
  /** git lines this instance charges. */
  lines: number;
  priorText?: string;
  freshText?: string;
}

/** Where samples go. `cap` bounds memory on a 900k-line tree. */
export interface NoiseSink {
  file: string;
  samples: NoiseSample[];
  cap: number;
}

function keep(sink: NoiseSink | undefined, s: NoiseSample): void {
  if (sink && sink.samples.length < sink.cap) sink.samples.push(s);
}

function classifyFile(
  priorCode: string,
  freshCode: string,
  tally: Tally,
  sink?: NoiseSink
): void {
  const prior = statementsOf(priorCode);
  const fresh = statementsOf(freshCode);

  // 1. exact (hash+text) pairing, FIFO by multiset
  const exactKey = (s: Stmt) => `${s.hash}\u0000${s.text}`;
  const priorExact = new Map<string, number>();
  for (const s of prior)
    priorExact.set(exactKey(s), (priorExact.get(exactKey(s)) ?? 0) + 1);
  const freshExactMatched: Stmt[] = [];
  const freshRest: Stmt[] = [];
  for (const s of fresh) {
    const k = exactKey(s);
    const n = priorExact.get(k) ?? 0;
    if (n > 0) {
      priorExact.set(k, n - 1);
      freshExactMatched.push(s);
    } else freshRest.push(s);
  }
  const priorRest: Stmt[] = [];
  const stillAvailable = new Map(priorExact);
  const priorExactMatched: Stmt[] = [];
  for (const s of prior) {
    const k = exactKey(s);
    const n = stillAvailable.get(k) ?? 0;
    if (n > 0) {
      stillAvailable.set(k, n - 1);
      priorRest.push(s); // unmatched leftover copy
    } else priorExactMatched.push(s);
  }

  // REORDER: exact-matched statements emitted out of order.
  const inOrder = onLcs(
    priorExactMatched.map(exactKey),
    freshExactMatched.map(exactKey)
  );
  freshExactMatched.forEach((s, i) => {
    if (!inOrder.has(i)) {
      tally.reorder += s.lines.length * 2; // delete + add
      keep(sink, {
        kind: "reorder",
        file: sink?.file ?? "",
        lines: s.lines.length * 2,
        freshText: s.text
      });
    }
  });

  // 2. same hash, different text -> NAMING churn (charged actual differing lines)
  const priorByHash = new Map<string, Stmt[]>();
  for (const s of priorRest) {
    const l = priorByHash.get(s.hash) ?? [];
    l.push(s);
    priorByHash.set(s.hash, l);
  }
  const novelFresh: Stmt[] = [];
  for (const s of freshRest) {
    const bucket = priorByHash.get(s.hash);
    if (bucket && bucket.length > 0) {
      const twin = bucket.shift() as Stmt;
      const churn = lineChurn(twin.lines, s.lines);
      const isAlias = s.isRequire && twin.isRequire;
      if (isAlias) tally.alias += churn;
      else tally.naming += churn;
      if (churn > 0)
        keep(sink, {
          kind: isAlias ? "alias" : "naming",
          file: sink?.file ?? "",
          lines: churn,
          priorText: twin.text,
          freshText: s.text
        });
    } else {
      novelFresh.push(s); // hash absent from prior — new OR edited
    }
  }
  const removed: Stmt[] = [];
  for (const l of priorByHash.values()) for (const s of l) removed.push(s);

  // 3. A hash-flipped statement is usually an EDITED version of a prior one, not
  // a wholesale add+remove. Pair it with the removed statement it came from
  // (same rename-blind head, >=50% token overlap — the diff-ledger's rule) and
  // charge only the lines a line-diff would print. Without this, one edited line
  // inside a 5k-line statement is charged as 5k lines of "real change" (the
  // statement-mass trap the 034 README documents).
  const removedByHead = new Map<string, Stmt[]>();
  for (const s of removed) {
    const k = maskedHead(s.text);
    const l = removedByHead.get(k) ?? [];
    l.push(s);
    removedByHead.set(k, l);
  }
  const usedRemoved = new Set<Stmt>();
  for (const s of novelFresh) {
    const sw = tokenSet(s.text);
    let best: Stmt | null = null;
    let bestScore = 0;
    for (const c of removedByHead.get(maskedHead(s.text)) ?? []) {
      if (usedRemoved.has(c)) continue;
      const cw = tokenSet(c.text);
      let inter = 0;
      for (const w of cw) if (sw.has(w)) inter++;
      const score = inter / Math.max(sw.size, cw.size, 1);
      if (score > bestScore) {
        best = c;
        bestScore = score;
      }
    }
    if (best && bestScore >= 0.5) {
      usedRemoved.add(best);
      const e = editedLineCounts(s.text, best.text);
      tally.real += e.fresh + e.prior;
    } else {
      tally.real += s.lines.length; // genuinely new code
    }
  }
  for (const s of removed) {
    if (!usedRemoved.has(s)) tally.real += s.lines.length; // genuinely removed
  }
}

/**
 * Decompose the on-disk diff between two split trees into real change and each
 * noise mechanism, in git lines. Exported so the eval harness can score emit
 * layout without duplicating the rule — its statement classification is
 * position-AWARE, which is exactly what `analyze.ts` cannot see.
 */
export function composeDiff(
  priorDir: string,
  freshDir: string,
  /** Opt-in: collect up to `cap` readable noise instances alongside the tally. */
  collect?: { samples: NoiseSample[]; cap: number }
): Tally {
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const tally: Tally = {
    real: 0,
    naming: 0,
    alias: 0,
    reorder: 0,
    fileAddRemove: 0
  };

  for (const f of freshFiles) {
    if (priorFiles.has(f)) {
      classifyFile(
        fs.readFileSync(path.join(priorDir, f), "utf8"),
        fs.readFileSync(path.join(freshDir, f), "utf8"),
        tally,
        collect
          ? { file: f, samples: collect.samples, cap: collect.cap }
          : undefined
      );
    } else {
      tally.fileAddRemove += fs
        .readFileSync(path.join(freshDir, f), "utf8")
        .split("\n").length;
    }
  }
  for (const f of priorFiles) {
    if (!freshFiles.has(f)) {
      tally.fileAddRemove += fs
        .readFileSync(path.join(priorDir, f), "utf8")
        .split("\n").length;
    }
  }
  return tally;
}

function main() {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const tally = composeDiff(priorDir, freshDir);
  const noise = tally.naming + tally.alias + tally.reorder;
  const total = noise + tally.real + tally.fileAddRemove;
  const pct = (n: number) => ((100 * n) / total).toFixed(1).padStart(5);
  console.log(`=== DIFF COMPOSITION${label ? ` — ${label}` : ""} ===`);
  console.log(`  accounted churn lines: ${total}`);
  console.log(
    `  REAL change            ${String(tally.real).padStart(7)}  ${pct(tally.real)}%`
  );
  console.log(
    `  new/removed files      ${String(tally.fileAddRemove).padStart(7)}  ${pct(tally.fileAddRemove)}%`
  );
  console.log(
    `  --- noise ---          ${String(noise).padStart(7)}  ${pct(noise)}%`
  );
  console.log(
    `    naming churn         ${String(tally.naming).padStart(7)}  ${pct(tally.naming)}%`
  );
  console.log(
    `    require-alias churn  ${String(tally.alias).padStart(7)}  ${pct(tally.alias)}%`
  );
  console.log(
    `    reorder churn        ${String(tally.reorder).padStart(7)}  ${pct(tally.reorder)}%`
  );
  console.log(
    `ROW|${label ?? ""}|${total}|${tally.real}|${tally.fileAddRemove}|${tally.naming}|${tally.alias}|${tally.reorder}`
  );
}

// Run the CLI only when executed directly, not when imported.
if (
  process.argv[1] &&
  import.meta.url.endsWith(process.argv[1].split("/").pop() ?? "")
) {
  main();
}

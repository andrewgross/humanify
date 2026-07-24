/**
 * exp038 Task A — the CEILING of dependency-aware emit order.
 *
 * Lever B v2 may reposition only FUNCTION DECLARATIONS. This measures what the
 * residual reorder churn of the current trees is actually made of, using the
 * real load-time dependency model (`src/split/load-order.ts`), and then
 * SIMULATES the best order that model permits — the honest ceiling, not a proxy.
 *
 * Two outputs per hop:
 *
 *  1. Classification of every displaced statement (git lines, x2 for del+add,
 *     the same unit `diff-composition.ts` reports):
 *       MOVABLE_FN   — function declaration; v2 already allows it to move.
 *       PURE_WRAPPER — `lazyInitializer(...)`-shaped, verified structurally;
 *                      captures a closure and returns, nothing observable.
 *       FREE_DECL    — other effect-free declaration with no load-time read of
 *                      a binding this file writes at load time.
 *       ORDER_BOUND  — effect-bearing, or reads a load-time-written binding.
 *  2. Simulated achievable churn: re-order each fresh file to its prior order
 *     under the dependency model and re-measure. `ceiling = 1 - achievable/now`.
 *
 * Usage: npx tsx reorder-ceiling.ts <priorSrcDir> <freshSrcDir> [label]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  analyzeLoadOrder,
  type LoadOrderFacts,
  orderRespectingLoadOrder
} from "../../src/split/load-order.js";
import { identifyBunLazyInit } from "../../src/shared/bun-helpers.js";
import { statementHash } from "../../src/split/statement-hash.js";

type Cls = "MOVABLE_FN" | "PURE_WRAPPER" | "FREE_DECL" | "ORDER_BOUND";

interface Stmt {
  text: string;
  lines: number;
  hash: string;
  node: t.Statement;
}

function walkDir(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkDir(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** Emitter-generated header lines (accessors, requires, directives). They are
 * emitted in sorted order ahead of the body and never participate in reorder. */
function isHeaderText(text: string): boolean {
  return (
    /^Object\.defineProperty\(module\.exports,/.test(text) ||
    /^(?:const|var|let) [$\w]+ = require\(/.test(text) ||
    /^["'][^"']*["'];?$/.test(text)
  );
}

interface ParsedFile {
  header: Stmt[];
  body: Stmt[];
}

function parseFile(code: string): ParsedFile {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return { header: [], body: [] };
  }
  if (!ast || ast.type !== "File") return { header: [], body: [] };
  const header: Stmt[] = [];
  const body: Stmt[] = [];
  let inHeader = true;
  for (const s of ast.program.body) {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    const stmt: Stmt = {
      text,
      lines: text ? text.split("\n").length : 0,
      hash: statementHash(s),
      node: s
    };
    if (inHeader && isHeaderText(text)) header.push(stmt);
    else {
      inHeader = false;
      body.push(stmt);
    }
  }
  return { header, body };
}

/** Indices of `fresh` that lie on the LCS with `prior` (i.e. NOT displaced). */
function onLcs(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  if (n === 0 || m === 0 || n * m > 25_000_000)
    return new Set(fresh.map((_, i) => i));
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++)
    for (let j = 1; j <= m; j++)
      dp[i][j] =
        prior[i - 1] === fresh[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
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

/** Exact-text FIFO pairing, as diff-composition does: the matched prior and
 * fresh statements, in their own order. */
function pairExact(
  prior: Stmt[],
  fresh: Stmt[]
): { pIdx: number[]; fIdx: number[] } {
  const counts = new Map<string, number>();
  for (const s of prior) counts.set(s.text, (counts.get(s.text) ?? 0) + 1);
  const fIdx: number[] = [];
  for (let i = 0; i < fresh.length; i++) {
    const n = counts.get(fresh[i].text) ?? 0;
    if (n > 0) {
      counts.set(fresh[i].text, n - 1);
      fIdx.push(i);
    }
  }
  const left = new Map(counts);
  const pIdx: number[] = [];
  for (let i = 0; i < prior.length; i++) {
    const n = left.get(prior[i].text) ?? 0;
    if (n > 0) left.set(prior[i].text, n - 1);
    else pIdx.push(i);
  }
  return { pIdx, fIdx };
}

/** Displaced fresh statements (indices into `fresh`) given an emission order. */
function displaced(prior: Stmt[], fresh: Stmt[], order: number[]): number[] {
  const ordered = order.map((i) => fresh[i]);
  const { pIdx, fIdx } = pairExact(prior, ordered);
  const keep = onLcs(
    pIdx.map((i) => prior[i].text),
    fIdx.map((i) => ordered[i].text)
  );
  const out: number[] = [];
  fIdx.forEach((i, k) => {
    if (!keep.has(k)) out.push(order[i]);
  });
  return out;
}

/** The prior-aligned ideal order, mirroring `orderByHashSequence` in
 * stable-split.ts including its unambiguous-hash precision gate. */
function desiredOrder(prior: Stmt[], fresh: Stmt[]): number[] {
  const pCount = new Map<string, number>();
  for (const s of prior) pCount.set(s.hash, (pCount.get(s.hash) ?? 0) + 1);
  const fCount = new Map<string, number>();
  for (const s of fresh) fCount.set(s.hash, (fCount.get(s.hash) ?? 0) + 1);
  const rankOf = new Map<string, number>();
  prior.forEach((s, rank) => {
    if (!rankOf.has(s.hash)) rankOf.set(s.hash, rank);
  });
  let prevRank = -1;
  const keyed = fresh.map((s, pos) => {
    const unambiguous = pCount.get(s.hash) === 1 && fCount.get(s.hash) === 1;
    const rank = unambiguous ? rankOf.get(s.hash) : undefined;
    if (rank !== undefined) {
      prevRank = rank;
      return { idx: pos, key: rank, pos };
    }
    return { idx: pos, key: prevRank + 0.5, pos };
  });
  keyed.sort((a, b) => a.key - b.key || a.pos - b.pos);
  return keyed.map((e) => e.idx);
}

/** `var x = pureWrapper(...)` — the lazy-init registration shape. */
function isPureWrapperDecl(s: t.Statement, pure: ReadonlySet<string>): boolean {
  if (!t.isVariableDeclaration(s)) return false;
  return s.declarations.some((d) => {
    if (!d.init || !t.isCallExpression(d.init)) return false;
    const c = d.init.callee;
    const name = t.isIdentifier(c)
      ? c.name
      : t.isMemberExpression(c) && t.isIdentifier(c.property)
        ? c.property.name
        : t.isSequenceExpression(c)
          ? calleeTail(c)
          : null;
    return name !== null && pure.has(name);
  });
}

function calleeTail(seq: t.SequenceExpression): string | null {
  const last = seq.expressions[seq.expressions.length - 1];
  if (t.isMemberExpression(last) && t.isIdentifier(last.property))
    return last.property.name;
  if (t.isIdentifier(last)) return last.name;
  return null;
}

function classify(
  idx: number,
  facts: LoadOrderFacts[],
  body: Stmt[],
  loadWritten: Set<string>,
  pure: ReadonlySet<string>
): Cls {
  const f = facts[idx];
  if (f.hoisted) return "MOVABLE_FN";
  if (f.effects) return "ORDER_BOUND";
  if (f.reads.some((n) => loadWritten.has(n))) return "ORDER_BOUND";
  if (isPureWrapperDecl(body[idx].node, pure)) return "PURE_WRAPPER";
  return "FREE_DECL";
}

/** Find the lazy-init helper STRUCTURALLY (its `x && (y = x(x = 0))` shape),
 * never by matching an identifier name. */
function findPureWrapperNames(dir: string, files: string[]): Set<string> {
  const names = new Set<string>();
  for (const f of files) {
    const code = fs.readFileSync(path.join(dir, f), "utf8");
    const name = identifyBunLazyInit(code);
    if (name) names.add(name);
  }
  return names;
}

interface Tally {
  now: number;
  achievable: number;
  cls: Record<Cls, { lines: number; n: number }>;
  wouldMove: number;
  files: number;
}

function emptyTally(): Tally {
  return {
    now: 0,
    achievable: 0,
    wouldMove: 0,
    files: 0,
    cls: {
      MOVABLE_FN: { lines: 0, n: 0 },
      PURE_WRAPPER: { lines: 0, n: 0 },
      FREE_DECL: { lines: 0, n: 0 },
      ORDER_BOUND: { lines: 0, n: 0 }
    }
  };
}

function processFile(
  priorCode: string,
  freshCode: string,
  pure: ReadonlySet<string>,
  tally: Tally
): void {
  const prior = parseFile(priorCode);
  const fresh = parseFile(freshCode);
  if (fresh.body.length === 0) return;

  const identity = fresh.body.map((_, i) => i);
  const nowDisplaced = displaced(prior.body, fresh.body, identity);
  if (nowDisplaced.length === 0) return;
  tally.files++;

  const facts = analyzeLoadOrder(
    fresh.body.map((s) => s.node),
    { pureCallNames: pure }
  );
  // Bindings this FILE writes while loading. Header requires are excluded: they
  // are emitted ahead of every body statement, so reading one constrains nothing.
  const loadWritten = new Set<string>();
  for (const f of facts) for (const w of f.writes) loadWritten.add(w);

  for (const i of nowDisplaced) {
    const c = classify(i, facts, fresh.body, loadWritten, pure);
    tally.cls[c].lines += fresh.body[i].lines * 2;
    tally.cls[c].n += 1;
    tally.now += fresh.body[i].lines * 2;
  }

  // Simulate: order to the prior under the dependency model, re-measure.
  const desired = desiredOrder(prior.body, fresh.body);
  const simulated = orderRespectingLoadOrder(identity, desired, facts);
  simulated.forEach((s, pos) => {
    if (s !== pos) tally.wouldMove++;
  });
  for (const i of displaced(prior.body, fresh.body, simulated)) {
    tally.achievable += fresh.body[i].lines * 2;
  }
}

function main() {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const priorFiles = new Set(walkDir(priorDir));
  const freshRel = walkDir(freshDir);
  const common = freshRel.filter((f) => priorFiles.has(f));
  const pure = findPureWrapperNames(freshDir, freshRel);
  const tally = emptyTally();

  for (const f of common) {
    processFile(
      fs.readFileSync(path.join(priorDir, f), "utf8"),
      fs.readFileSync(path.join(freshDir, f), "utf8"),
      pure,
      tally
    );
  }

  const pct = (n: number) =>
    tally.now ? `${((100 * n) / tally.now).toFixed(1).padStart(5)}%` : "    -";
  console.log(`=== REORDER CEILING${label ? ` — ${label}` : ""} ===`);
  console.log(`  verified pure wrappers: ${[...pure].join(", ") || "(none)"}`);
  console.log(`  residual reorder churn NOW: ${tally.now} git lines`);
  for (const c of [
    "MOVABLE_FN",
    "PURE_WRAPPER",
    "FREE_DECL",
    "ORDER_BOUND"
  ] as Cls[]) {
    const v = tally.cls[c];
    console.log(
      `    ${c.padEnd(13)} ${String(v.lines).padStart(7)} ln  ${String(v.n).padStart(5)} stmts  ${pct(v.lines)}`
    );
  }
  console.log(`  ACHIEVABLE under the model: ${tally.achievable} git lines`);
  const cut = tally.now - tally.achievable;
  console.log(
    `  ceiling: ${cut} lines removable (${tally.now ? ((100 * cut) / tally.now).toFixed(1) : "0"}% of residual reorder churn)`
  );
  console.log(
    `  files with churn: ${tally.files}, statements the model would move: ${tally.wouldMove}`
  );
  console.log(
    `ROW|${label ?? ""}|${tally.now}|${tally.achievable}|${tally.cls.MOVABLE_FN.lines}|${tally.cls.PURE_WRAPPER.lines}|${tally.cls.FREE_DECL.lines}|${tally.cls.ORDER_BOUND.lines}`
  );
}

main();

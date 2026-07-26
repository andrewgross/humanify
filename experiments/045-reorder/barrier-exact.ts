/**
 * The exact constrained share, using the SHIPPED load-order model rather than a
 * syntax approximation.
 *
 * Task A treated every top-level `ExpressionStatement` as an order barrier and
 * concluded 33–76% of reorder churn cannot legally move back. That over-counts:
 * `bundleLoadOrderFacts` (src/split/load-order.ts) computes `effects` per
 * statement, and an expression statement with no observable load-time effect is
 * movable. Task A therefore reported a LOWER bound on what is recoverable and
 * said so; this replaces the estimate with the model that actually runs.
 *
 * A reordered statement is CONSTRAINED when it is itself a barrier
 * (`effects: true`), or when a barrier sits between where it was and where it
 * landed — restoring its prior position would have to cross one, which the
 * boot-crash rule forbids.
 *
 * Usage: npx tsx barrier-exact.ts <priorSrcDir> <freshSrcDir> <label>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { bundleLoadOrderFacts } from "../../src/split/load-order.js";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  effects: boolean;
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

function statementsOf(code: string): Stmt[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  const body = ast.program.body as t.Statement[];
  // The same facts the emitter's aligner consults, on the same statements.
  const facts = bundleLoadOrderFacts(body, code);
  return body.map((s, i) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text ? text.split("\n").length : 0,
      effects: facts[i]?.effects ?? true
    };
  });
}

function onLcs(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  if (n === 0 || m === 0 || n * m > 25_000_000) {
    return new Set(fresh.map((_, i) => i));
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

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  let totalLn = 0;
  let barrierLn = 0;
  let blockedLn = 0;
  let freeLn = 0;
  let freeStmts = 0;

  for (const rel of walk(freshDir)) {
    const pp = path.join(priorDir, rel);
    if (!fs.existsSync(pp)) continue;
    const prior = statementsOf(fs.readFileSync(pp, "utf8"));
    const fresh = statementsOf(
      fs.readFileSync(path.join(freshDir, rel), "utf8")
    );
    const key = (s: Stmt) => `${s.hash} ${s.text}`;

    const counts = new Map<string, number>();
    for (const s of prior) counts.set(key(s), (counts.get(key(s)) ?? 0) + 1);
    const fMatched: Stmt[] = [];
    for (const s of fresh) {
      const k = key(s);
      const c = counts.get(k) ?? 0;
      if (c > 0) {
        counts.set(k, c - 1);
        fMatched.push(s);
      }
    }
    const still = new Map(counts);
    const pMatched: Stmt[] = [];
    for (const s of prior) {
      const k = key(s);
      const c = still.get(k) ?? 0;
      if (c > 0) still.set(k, c - 1);
      else pMatched.push(s);
    }
    const keep = onLcs(pMatched.map(key), fMatched.map(key));
    const priorPos = new Map<string, number>();
    pMatched.forEach((s, i) => {
      if (!priorPos.has(key(s))) priorPos.set(key(s), i);
    });

    fMatched.forEach((s, i) => {
      if (keep.has(i)) return;
      const ln = s.lines * 2;
      totalLn += ln;
      if (s.effects) {
        barrierLn += ln;
        return;
      }
      const was = priorPos.get(key(s));
      if (was === undefined) {
        freeLn += ln;
        freeStmts++;
        return;
      }
      const lo = Math.min(was, i);
      const hi = Math.max(was, i);
      let crosses = false;
      for (let k = lo; k <= hi && !crosses; k++) {
        if (fMatched[k]?.effects) crosses = true;
      }
      if (crosses) blockedLn += ln;
      else {
        freeLn += ln;
        freeStmts++;
      }
    });
  }

  const constrained = barrierLn + blockedLn;
  console.log(`=== EXACT BARRIER ANALYSIS — ${label ?? ""} ===`);
  console.log(`  reorder churn: ${totalLn} git lines`);
  console.log(
    `  CONSTRAINED ${constrained} ln (${((100 * constrained) / Math.max(totalLn, 1)).toFixed(1)}%)` +
      ` = barrier itself ${barrierLn} + blocked by one ${blockedLn}`
  );
  console.log(
    `  RECOVERABLE ${freeLn} ln (${((100 * freeLn) / Math.max(totalLn, 1)).toFixed(1)}%) in ${freeStmts} statements` +
      ` — movable, and emitted out of prior order anyway`
  );
  console.log(`ROW|${label ?? ""}|${totalLn}|${constrained}|${freeLn}`);
}

main();

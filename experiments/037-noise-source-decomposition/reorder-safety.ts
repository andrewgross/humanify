/**
 * Reorder-safety split (exp037 Lever B): of the ~14k reorder-churn lines, how
 * much is provably-safe-to-reorder declarations vs statements whose load-time
 * position could be semantically load-bearing?
 *
 *  - HOISTED   : FunctionDeclaration — fully hoisted, reorderable anywhere, zero
 *                semantic effect. The provably-safe core.
 *  - LAZY_DECL : var/let/const whose initializer is a function/arrow/class expr,
 *                a lazyInitializer(...) thunk, or a literal — assignment has no
 *                observable load-time effect; safe to reorder among declarations.
 *  - ANCHOR    : anything else (bare calls, expression statements, side-effectful
 *                initializers, if/for/try at top level) — keep pinned.
 *
 * Usage: npx tsx reorder-safety.ts <priorSrcDir> <freshSrcDir>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { statementHash } from "../../src/split/statement-hash.js";

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

type Cls = "HOISTED" | "LAZY_DECL" | "ANCHOR";

function classify(stmt: t.Statement): Cls {
  if (t.isFunctionDeclaration(stmt)) return "HOISTED";
  if (t.isClassDeclaration(stmt)) return "LAZY_DECL"; // TDZ but reorderable among decls
  if (t.isVariableDeclaration(stmt)) {
    const allSafe = stmt.declarations.every((d) => {
      if (!d.init) return true; // bare declaration
      const i = d.init;
      if (
        t.isFunctionExpression(i) ||
        t.isArrowFunctionExpression(i) ||
        t.isClassExpression(i) ||
        t.isLiteral(i) ||
        t.isIdentifier(i)
      )
        return true;
      // lazyInitializer(...) / (0, x.lazyInitializer)(...) thunk wrappers
      if (t.isCallExpression(i)) {
        const c = i.callee;
        const name = t.isIdentifier(c)
          ? c.name
          : t.isMemberExpression(c) && t.isIdentifier(c.property)
            ? c.property.name
            : t.isSequenceExpression(c) &&
                t.isMemberExpression(c.expressions[c.expressions.length - 1]) &&
                t.isIdentifier(
                  (
                    c.expressions[
                      c.expressions.length - 1
                    ] as t.MemberExpression
                  ).property
                )
              ? (
                  (
                    c.expressions[
                      c.expressions.length - 1
                    ] as t.MemberExpression
                  ).property as t.Identifier
                ).name
              : "";
        return /lazy|memoize|once|thunk|defineLazy/i.test(name);
      }
      return false;
    });
    return allSafe ? "LAZY_DECL" : "ANCHOR";
  }
  return "ANCHOR";
}

interface Stmt {
  key: string;
  lines: number;
  cls: Cls;
}
function statementsOfFile(code: string): Stmt[] {
  let ast;
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
      key: `${statementHash(s)}\x00${text}`,
      lines: text ? text.split("\n").length : 0,
      cls: classify(s)
    };
  });
}

function lcsFreshIndices(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++)
    for (let j = 1; j <= m; j++)
      dp[i][j] =
        prior[i - 1] === fresh[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
  const on = new Set<number>();
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (prior[i - 1] === fresh[j - 1]) {
      on.add(j - 1);
      i--;
      j--;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) i--;
    else j--;
  }
  return on;
}

function main() {
  const [priorDir, freshDir] = process.argv.slice(2);
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const common = [...freshFiles].filter((f) => priorFiles.has(f));
  const tally: Record<Cls, number> = { HOISTED: 0, LAZY_DECL: 0, ANCHOR: 0 };
  let total = 0;

  for (const f of common) {
    const prior = statementsOfFile(
      fs.readFileSync(path.join(priorDir, f), "utf8")
    );
    const fresh = statementsOfFile(
      fs.readFileSync(path.join(freshDir, f), "utf8")
    );
    const pc = new Map<string, number>();
    for (const s of prior) pc.set(s.key, (pc.get(s.key) ?? 0) + 1);
    const fc = new Map<string, number>();
    for (const s of fresh) fc.set(s.key, (fc.get(s.key) ?? 0) + 1);
    const stable = (k: string) => Math.min(pc.get(k) ?? 0, fc.get(k) ?? 0) > 0;
    const pSeq = prior.filter((s) => stable(s.key));
    const fSeq = fresh.filter((s) => stable(s.key));
    if (pSeq.length * fSeq.length > 4_000_000) continue;
    const on = lcsFreshIndices(
      pSeq.map((s) => s.key),
      fSeq.map((s) => s.key)
    );
    fSeq.forEach((s, idx) => {
      if (!on.has(idx)) {
        tally[s.cls] += s.lines;
        total += s.lines;
      }
    });
  }
  console.log(
    "=== reorder churn by safety class (fresh-side displaced lines) ==="
  );
  console.log(`  HOISTED   (function decl, always safe):  ${tally.HOISTED}`);
  console.log(`  LAZY_DECL (class/lazy/literal init):     ${tally.LAZY_DECL}`);
  console.log(`  ANCHOR    (side-effect risk, keep pinned):${tally.ANCHOR}`);
  console.log(`  TOTAL reorder churn:                      ${total}`);
  console.log(
    `  provably-safe (HOISTED+LAZY_DECL): ${tally.HOISTED + tally.LAZY_DECL} (${((100 * (tally.HOISTED + tally.LAZY_DECL)) / total).toFixed(1)}%)`
  );
}

main();

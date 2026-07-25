/**
 * Reorder churn (exp037): of the statements that are BYTE-IDENTICAL in both
 * versions of a file, how many are emitted in a different POSITION and therefore
 * churn under git's line diff even though nothing about them changed?
 *
 * For each common file: take the subsequence of exact (hash,text) statements that
 * appear in both, compute the LCS of their prior-order vs fresh-order. Statements
 * NOT on the LCS are displaced — git renders them delete+add. Their line total is
 * the reorder churn an order-stable emitter would eliminate for free.
 *
 * Usage: npx tsx reorder-churn.ts <priorSrcDir> <freshSrcDir>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { statementHash } from "../../src/split/statement-hash.js";

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

interface Stmt {
  key: string;
  lines: number;
}
function statementsOfFile(code: string): Stmt[] {
  let ast;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  return ast.program.body.map((stmt) => {
    const text =
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : "";
    return {
      key: `${statementHash(stmt)}\x00${text}`,
      lines: text ? text.split("\n").length : 0
    };
  });
}

/** LCS over two arrays of keys; returns the set of fresh indices ON the LCS. */
function lcsFreshIndices(prior: string[], fresh: string[]): Set<number> {
  const n = prior.length;
  const m = fresh.length;
  // DP table (n+1)x(m+1). Files are small; guard huge ones.
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array(m + 1).fill(0)
  );
  for (let i = 1; i <= n; i++)
    for (let j = 1; j <= m; j++)
      dp[i][j] =
        prior[i - 1] === fresh[j - 1]
          ? dp[i - 1][j - 1] + 1
          : Math.max(dp[i - 1][j], dp[i][j - 1]);
  const onLcs = new Set<number>();
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    if (prior[i - 1] === fresh[j - 1]) {
      onLcs.add(j - 1);
      i--;
      j--;
    } else if (dp[i - 1][j] >= dp[i][j - 1]) i--;
    else j--;
  }
  return onLcs;
}

function main() {
  const [priorDir, freshDir] = process.argv.slice(2);
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));
  const common = [...freshFiles].filter((f) => priorFiles.has(f));

  let reorderLn = 0;
  let reorderStmts = 0;
  let stableStmts = 0;
  let skippedHuge = 0;
  const worst: Array<[string, number]> = [];

  for (const f of common) {
    const prior = statementsOfFile(
      fs.readFileSync(path.join(priorDir, f), "utf8")
    );
    const fresh = statementsOfFile(
      fs.readFileSync(path.join(freshDir, f), "utf8")
    );
    // restrict to statements exactly present in BOTH (the stable set), preserving order
    const priorCount = new Map<string, number>();
    for (const s of prior)
      priorCount.set(s.key, (priorCount.get(s.key) ?? 0) + 1);
    const freshCount = new Map<string, number>();
    for (const s of fresh)
      freshCount.set(s.key, (freshCount.get(s.key) ?? 0) + 1);
    const stableKey = (k: string) =>
      Math.min(priorCount.get(k) ?? 0, freshCount.get(k) ?? 0) > 0;
    const priorSeq = prior.filter((s) => stableKey(s.key));
    const freshSeq = fresh.filter((s) => stableKey(s.key));
    stableStmts += freshSeq.length;
    if (priorSeq.length * freshSeq.length > 4_000_000) {
      skippedHuge++;
      continue; // guard: giant file, skip LCS
    }
    const onLcs = lcsFreshIndices(
      priorSeq.map((s) => s.key),
      freshSeq.map((s) => s.key)
    );
    let fileReorder = 0;
    freshSeq.forEach((s, idx) => {
      if (!onLcs.has(idx)) {
        reorderLn += s.lines;
        fileReorder += s.lines;
        reorderStmts++;
      }
    });
    if (fileReorder > 0) worst.push([f, fileReorder]);
  }

  worst.sort((a, b) => b[1] - a[1]);
  console.log(
    "=== REORDER churn (byte-identical statements emitted out of order) ==="
  );
  console.log(`  stable statements (identical both sides): ${stableStmts}`);
  console.log(`  displaced (off-LCS) statements:           ${reorderStmts}`);
  console.log(`  REORDER churn lines (git would delete+add): ${reorderLn}`);
  console.log(`  huge files skipped: ${skippedHuge}`);
  console.log(`\n  top 15 files by reorder churn:`);
  for (const [f, n] of worst.slice(0, 15))
    console.log(`    ${String(n).padStart(5)}ln  ${f}`);
}

main();

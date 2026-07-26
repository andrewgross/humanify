/**
 * Where is REORDER churn, now that relocation is solved?
 *
 * exp038 built the load-time dependency model that made emission order follow
 * the prior release, taking reorder from 33%/27%/14%/3% of the diff down to
 * 4.4%/6.2%/3.2%/0.7%. Nothing has examined it since, and after exp041-043 took
 * relocation from 15,699 to 1,390 git lines it is now the SECOND largest noise
 * bucket at 6,078 — larger than the entire tractable naming residue (~2,600).
 *
 * `diff-composition.ts` charges reorder when an EXACT-matched statement (same
 * hash AND same text) is emitted outside the longest common subsequence of its
 * file's prior order. This asks the questions a brief would need answered:
 *
 *   - how concentrated is it? A handful of files or a broad smear decides
 *     whether there is a mechanism to find at all.
 *   - how far do statements move? One slot is an alignment tie-break; hundreds
 *     of slots is a different phenomenon.
 *
 * Usage: npx tsx probe.ts <priorSrcDir> <freshSrcDir> <label>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  /** A top-level statement that can observably DO something while the module
   * loads. exp038's load-order model forbids moving anything across one (the
   * boot-crash rule), so a reordered statement that is itself a barrier, or
   * whose path is blocked by one, is the constraint working rather than a
   * defect — and it caps what any lever here can recover. */
  isBarrier: boolean;
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
  return ast.program.body.map((s) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text ? text.split("\n").length : 0,
      isBarrier: s.type === "ExpressionStatement"
    };
  });
}

/** Indices of `fresh` that lie ON the LCS — i.e. did NOT move. */
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
  const perFile: Array<{ file: string; n: number; ln: number }> = [];
  let totalStmts = 0;
  let totalLn = 0;
  let barrierStmts = 0;
  let barrierLn = 0;
  let blockedStmts = 0;
  let blockedLn = 0;
  const distances: number[] = [];

  for (const rel of walk(freshDir)) {
    const pp = path.join(priorDir, rel);
    if (!fs.existsSync(pp)) continue;
    const prior = statementsOf(fs.readFileSync(pp, "utf8"));
    const fresh = statementsOf(
      fs.readFileSync(path.join(freshDir, rel), "utf8")
    );
    const key = (s: Stmt) => `${s.hash} ${s.text}`;

    // Exact pairing, FIFO by multiset — the population diff-composition scores.
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

    let n = 0;
    let ln = 0;
    fMatched.forEach((s, i) => {
      if (keep.has(i)) return;
      n++;
      ln += s.lines * 2;
      const was = priorPos.get(key(s));
      if (was !== undefined) distances.push(Math.abs(was - i));
      if (s.isBarrier) {
        barrierStmts++;
        barrierLn += s.lines * 2;
      } else if (was !== undefined) {
        // Is a barrier sitting between where it was and where it landed? Then
        // restoring the prior position would have to cross it, which the model
        // forbids.
        const lo = Math.min(was, i);
        const hi = Math.max(was, i);
        let crosses = false;
        for (let k = lo; k <= hi && !crosses; k++) {
          if (fMatched[k]?.isBarrier) crosses = true;
        }
        if (crosses) {
          blockedStmts++;
          blockedLn += s.lines * 2;
        }
      }
    });
    if (n > 0) perFile.push({ file: rel, n, ln });
    totalStmts += n;
    totalLn += ln;
  }

  perFile.sort((a, b) => b.ln - a.ln);
  const top10 = perFile.slice(0, 10).reduce((a, f) => a + f.ln, 0);
  distances.sort((a, b) => a - b);
  const q = (p: number) =>
    distances[Math.floor(p * (distances.length - 1))] ?? 0;

  console.log(`=== REORDER PROBE — ${label ?? ""} ===`);
  console.log(`  reordered statements: ${totalStmts} / ${totalLn} git lines`);
  console.log(
    `  files affected: ${perFile.length}; top 10 carry ${top10} ln (${((100 * top10) / Math.max(totalLn, 1)).toFixed(1)}%)`
  );
  console.log(
    `  move distance in slots — p50 ${q(0.5)}, p90 ${q(0.9)}, max ${distances[distances.length - 1] ?? 0}`
  );
  console.log(
    `  CONSTRAINED (cannot legally move back): barrier statements ${barrierStmts} / ${barrierLn} ln,` +
      ` blocked by a barrier ${blockedStmts} / ${blockedLn} ln` +
      ` — together ${((100 * (barrierLn + blockedLn)) / Math.max(totalLn, 1) || 0).toFixed(1)}% of the churn`
  );
  console.log("  worst files:");
  for (const f of perFile.slice(0, 6)) {
    console.log(`    ${String(f.ln).padStart(5)} ln  ${f.n} stmts  ${f.file}`);
  }
}

main();

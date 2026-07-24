/**
 * Why is a statement STILL displaced after Lever B v2?
 *
 * The alignment runs in BUNDLE-hash space (the ledger's per-file `hashes`
 * sequence); the diff a human sees is in EMITTED-text space. The two are 1:1 in
 * order — the k-th ledger entry for a file is the k-th body statement of that
 * file on disk — so we can join them and ask, for every displaced statement,
 * which gate stopped it:
 *
 *   BAIL        — the file had < 2 movable+unambiguous statements, so
 *                 alignFileStatements returned bundle order untouched.
 *   AMBIG       — its bundle hash occurs more than once on a side, so the
 *                 precision gate refused to let it claim a prior position.
 *   NOT_MOVABLE — unambiguous, but not a FunctionDeclaration: pinned by the
 *                 blanket load-order safety rule (the thing exp038 replaces).
 *   PLACED      — movable and unambiguous, so the aligner DID place it at its
 *                 prior position; it still reads as displaced because the
 *                 statements around it could not follow.
 *
 * Usage: npx tsx align-trace.ts <priorOutDir> <freshOutDir> [--file F]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { statementHash } from "../../src/split/statement-hash.js";

interface Ledger {
  order: string[];
  hashes: string[];
}

interface Body {
  text: string;
  lines: number;
  node: t.Statement;
}

/** Emitter-generated header lines (accessors, requires, directives) precede the
 * body and are not ledger statements. */
function isHeaderText(text: string): boolean {
  return (
    /^Object\.defineProperty\(module\.exports,/.test(text) ||
    /^(?:const|var|let) [$\w]+ = require\(/.test(text) ||
    /^["'][^"']*["'];?$/.test(text)
  );
}

function bodyOf(code: string): Body[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  const out: Body[] = [];
  let inHeader = true;
  for (const s of ast.program.body) {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    if (inHeader && isHeaderText(text)) continue;
    inHeader = false;
    out.push({ text, lines: text ? text.split("\n").length : 0, node: s });
  }
  return out;
}

function seqByFile(l: Ledger): Map<string, string[]> {
  const m = new Map<string, string[]>();
  l.order.forEach((f, i) => {
    const list = m.get(f) ?? [];
    list.push(l.hashes[i]);
    m.set(f, list);
  });
  return m;
}

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

type Gate = "BAIL" | "AMBIG" | "NOT_MOVABLE" | "PLACED";

function main() {
  const args = process.argv.slice(2);
  const [priorDir, freshDir] = args;
  const onlyFile = args.includes("--file")
    ? args[args.indexOf("--file") + 1]
    : undefined;

  const priorLedger = JSON.parse(
    fs.readFileSync(path.join(priorDir, ".humanify/split-ledger.json"), "utf8")
  ) as Ledger;
  const freshLedger = JSON.parse(
    fs.readFileSync(path.join(freshDir, ".humanify/split-ledger.json"), "utf8")
  ) as Ledger;
  const priorSeq = seqByFile(priorLedger);
  const freshSeq = seqByFile(freshLedger);

  const tally: Record<Gate, { lines: number; n: number }> = {
    BAIL: { lines: 0, n: 0 },
    AMBIG: { lines: 0, n: 0 },
    NOT_MOVABLE: { lines: 0, n: 0 },
    PLACED: { lines: 0, n: 0 }
  };
  const byGateShape = new Map<string, { lines: number; n: number }>();
  let skewed = 0;

  for (const [file, fSeq] of freshSeq) {
    if (onlyFile && file !== onlyFile) continue;
    const pSeq = priorSeq.get(file);
    if (!pSeq) continue;
    const fPath = path.join(freshDir, file);
    const pPath = path.join(priorDir, file);
    if (!fs.existsSync(fPath) || !fs.existsSync(pPath)) continue;
    const fBody = bodyOf(fs.readFileSync(fPath, "utf8"));
    const pBody = bodyOf(fs.readFileSync(pPath, "utf8"));
    if (fBody.length !== fSeq.length || pBody.length !== pSeq.length) {
      skewed++;
      continue; // header detection disagreed; don't guess
    }

    // Exact (text) pairing, FIFO — same rule as diff-composition.
    const pCount = new Map<string, number>();
    for (const b of pBody) pCount.set(b.text, (pCount.get(b.text) ?? 0) + 1);
    const fMatched: number[] = [];
    for (let i = 0; i < fBody.length; i++) {
      const n = pCount.get(fBody[i].text) ?? 0;
      if (n > 0) {
        pCount.set(fBody[i].text, n - 1);
        fMatched.push(i);
      }
    }
    const avail = new Map(pCount);
    const pMatched: number[] = [];
    for (let i = 0; i < pBody.length; i++) {
      const n = avail.get(pBody[i].text) ?? 0;
      if (n > 0) avail.set(pBody[i].text, n - 1);
      else pMatched.push(i);
    }
    const keep = onLcs(
      pMatched.map((i) => pBody[i].text),
      fMatched.map((i) => fBody[i].text)
    );

    // Replay the two gates the aligner applies, in bundle-hash space.
    const fHashCount = new Map<string, number>();
    for (const h of fSeq) fHashCount.set(h, (fHashCount.get(h) ?? 0) + 1);
    const pHashCount = new Map<string, number>();
    for (const h of pSeq) pHashCount.set(h, (pHashCount.get(h) ?? 0) + 1);
    const unambiguous = (k: number) =>
      fHashCount.get(fSeq[k]) === 1 && pHashCount.get(fSeq[k]) === 1;
    const movable = fBody.map((b) => t.isFunctionDeclaration(b.node));
    const bailed =
      fBody.filter((_, k) => movable[k] && unambiguous(k)).length < 2;

    fMatched.forEach((k, idx) => {
      if (keep.has(idx)) return;
      const gate: Gate = bailed
        ? "BAIL"
        : !unambiguous(k)
          ? "AMBIG"
          : !movable[k]
            ? "NOT_MOVABLE"
            : "PLACED";
      const ln = fBody[k].lines * 2;
      tally[gate].lines += ln;
      tally[gate].n += 1;
      const shape = fBody[k].node.type;
      const key = `${gate}/${shape}`;
      const cur = byGateShape.get(key) ?? { lines: 0, n: 0 };
      cur.lines += ln;
      cur.n += 1;
      byGateShape.set(key, cur);
      if (onlyFile) {
        console.log(
          `  displaced pos=${k} gate=${gate} ln=${fBody[k].lines} ${fBody[k].text.split("\n")[0].slice(0, 90)}`
        );
      }
    });
  }

  const total = Object.values(tally).reduce((a, b) => a + b.lines, 0);
  console.log(
    `\nTOTAL displaced (git lines): ${total}   [skewed files: ${skewed}]`
  );
  for (const g of ["BAIL", "AMBIG", "NOT_MOVABLE", "PLACED"] as Gate[]) {
    const v = tally[g];
    console.log(
      `  ${g.padEnd(12)} ${String(v.lines).padStart(7)} ln  ${String(v.n).padStart(5)} stmts  ${total ? ((100 * v.lines) / total).toFixed(1) : "0"}%`
    );
  }
  console.log("\n=== gate x shape ===");
  for (const [k, v] of [...byGateShape.entries()].sort(
    (a, b) => b[1].lines - a[1].lines
  )) {
    console.log(
      `${String(v.lines).padStart(7)} ln ${String(v.n).padStart(5)}x  ${k}`
    );
  }
}

main();

/**
 * Noise-source decomposition (exp037 Step 1).
 *
 * Answers: of the residual noiseLn, how much is recoverable by *name alignment*
 * (a small set of identifier substitutions reconciles a fresh noise statement to
 * an existing prior twin of the same structural hash) vs genuine DRIFT (no nearby
 * prior twin — the naming diverged too far).
 *
 * The noiseLn metric matches statements by identifier-blind statementHash and
 * counts a fresh statement as noise iff its hash exists in the prior but its
 * exact text matches no prior twin. Because same hash => identical node-type
 * traversal, we walk a fresh noise statement and each candidate prior twin in
 * lockstep, collecting Identifier names by index, and read off EXACTLY which
 * names would need to change to make the texts identical.
 *
 * Usage: npx tsx decompose-noise.ts <freshHumanified.js> <priorHumanified.js>
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  idents: string[]; // Identifier names in hash-traversal order
}

/** Collect Identifier names in the SAME iterative order statementHash traverses,
 * so two same-hash statements yield index-aligned sequences. */
function identSeq(stmt: t.Statement): string[] {
  const out: string[] = [];
  const stack: (t.Node | "close")[] = [stmt];
  while (stack.length > 0) {
    const item = stack.pop();
    if (item === "close" || item == null) continue;
    if (item.type === "Identifier" || item.type === "JSXIdentifier") {
      out.push(item.name);
    }
    const keys = t.VISITOR_KEYS[item.type] ?? [];
    for (let k = keys.length - 1; k >= 0; k--) {
      const child = (item as unknown as Record<string, unknown>)[keys[k]];
      pushChildren(stack, child);
    }
  }
  return out;
}

function pushChildren(stack: (t.Node | "close")[], child: unknown): void {
  if (Array.isArray(child)) {
    for (let i = child.length - 1; i >= 0; i--) {
      if (child[i] != null) pushChildren(stack, child[i]);
    }
    return;
  }
  if (
    typeof child === "object" &&
    child !== null &&
    typeof (child as { type?: unknown }).type === "string"
  ) {
    stack.push(child as t.Node);
  }
}

function statementsOf(code: string): Stmt[] {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  const body =
    wrapper && t.isBlockStatement(wrapper.functionPath.node.body)
      ? wrapper.functionPath.node.body.body
      : ast.program.body;
  return body.map((stmt) => {
    const text =
      stmt.start != null && stmt.end != null
        ? code.slice(stmt.start, stmt.end)
        : "";
    return {
      hash: statementHash(stmt),
      text,
      lines: text.length ? text.split("\n").length : 0,
      idents: identSeq(stmt)
    };
  });
}

/** Compare a fresh noise statement against a prior twin (same hash => same-length
 * ident seq). Returns the set of index positions that differ and the distinct
 * fresh->prior rename pairs. */
function diffAgainst(fresh: Stmt, prior: Stmt) {
  const a = fresh.idents;
  const b = prior.idents;
  const n = Math.min(a.length, b.length);
  let diffPositions = 0;
  const pairs = new Map<string, number>(); // "fresh\x00prior" -> count
  const freshToPrior = new Map<string, Set<string>>();
  const priorToFresh = new Map<string, Set<string>>();
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) {
      diffPositions++;
      const key = `${a[i]}\x00${b[i]}`;
      pairs.set(key, (pairs.get(key) ?? 0) + 1);
      if (!freshToPrior.has(a[i])) freshToPrior.set(a[i], new Set());
      freshToPrior.get(a[i])!.add(b[i]);
      if (!priorToFresh.has(b[i])) priorToFresh.set(b[i], new Set());
      priorToFresh.get(b[i])!.add(a[i]);
    }
  }
  // A rename set is "consistent" (a clean bijective rename) iff every fresh name
  // maps to exactly one prior name and vice versa.
  let consistent = true;
  for (const s of freshToPrior.values()) if (s.size > 1) consistent = false;
  for (const s of priorToFresh.values()) if (s.size > 1) consistent = false;
  return {
    diffPositions,
    distinctPairs: pairs.size,
    lenMismatch: a.length !== b.length,
    consistent
  };
}

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8"));
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8"));

  const priorByHash = new Map<string, Stmt[]>();
  const priorTextByHash = new Map<string, Set<string>>();
  for (const s of prior) {
    if (!priorByHash.has(s.hash)) priorByHash.set(s.hash, []);
    priorByHash.get(s.hash)!.push(s);
    if (!priorTextByHash.has(s.hash)) priorTextByHash.set(s.hash, new Set());
    priorTextByHash.get(s.hash)!.add(s.text);
  }

  let cleanN = 0;
  let novelN = 0;
  let noiseN = 0;
  let noiseLn = 0;

  // Buckets for the decomposition, keyed by (minDistinctPairs against best twin).
  const buckets = {
    single: { st: 0, ln: 0 }, // exactly 1 distinct rename pair reconciles it
    two: { st: 0, ln: 0 },
    fewSmall: { st: 0, ln: 0 }, // 3-5 pairs
    many: { st: 0, ln: 0 }, // 6+ pairs = drift
    lenMismatch: { st: 0, ln: 0 } // shouldn't happen if hash is sound
  };
  // Of single-pair statements, is the differing identifier consistent with a
  // clean rename? And how big is the hash-class (singleton vs bucket)?
  let singleInSingletonClass = { st: 0, ln: 0 };
  let singleInBucketClass = { st: 0, ln: 0 };
  // Distribution of best-twin distinctPairs.
  const pairHisto = new Map<number, { st: number; ln: number }>();
  // Collect the single-pair rename examples (the highly recoverable slice).
  const singleRenameExamples = new Map<string, number>();

  for (const s of fresh) {
    const priorTexts = priorTextByHash.get(s.hash);
    if (!priorTexts) {
      novelN++;
      continue;
    }
    if (priorTexts.has(s.text)) {
      cleanN++;
      continue;
    }
    noiseN++;
    noiseLn += s.lines;
    // Find best prior twin: minimize distinctPairs, tiebreak diffPositions.
    const twins = priorByHash.get(s.hash)!;
    let best: ReturnType<typeof diffAgainst> | null = null;
    let bestTwin: Stmt | null = null;
    for (const twin of twins) {
      const d = diffAgainst(s, twin);
      if (
        best === null ||
        d.distinctPairs < best.distinctPairs ||
        (d.distinctPairs === best.distinctPairs &&
          d.diffPositions < best.diffPositions)
      ) {
        best = d;
        bestTwin = twin;
      }
    }
    if (!best) continue;
    const dp = best.distinctPairs;
    const h = pairHisto.get(dp) ?? { st: 0, ln: 0 };
    h.st++;
    h.ln += s.lines;
    pairHisto.set(dp, h);

    if (best.lenMismatch) {
      buckets.lenMismatch.st++;
      buckets.lenMismatch.ln += s.lines;
    } else if (dp === 1) {
      buckets.single.st++;
      buckets.single.ln += s.lines;
      if (twins.length === 1) {
        singleInSingletonClass.st++;
        singleInSingletonClass.ln += s.lines;
      } else {
        singleInBucketClass.st++;
        singleInBucketClass.ln += s.lines;
      }
      // record the rename pair
      const b2 = bestTwin!;
      for (let i = 0; i < s.idents.length; i++) {
        if (s.idents[i] !== b2.idents[i]) {
          const key = `${s.idents[i]} -> ${b2.idents[i]}`;
          singleRenameExamples.set(
            key,
            (singleRenameExamples.get(key) ?? 0) + 1
          );
          break;
        }
      }
    } else if (dp === 2) {
      buckets.two.st++;
      buckets.two.ln += s.lines;
    } else if (dp <= 5) {
      buckets.fewSmall.st++;
      buckets.fewSmall.ln += s.lines;
    } else {
      buckets.many.st++;
      buckets.many.ln += s.lines;
    }
  }

  const pct = (n: number) => ((100 * n) / noiseLn).toFixed(1);
  console.log("=== TOTALS ===");
  console.log(
    `fresh statements: ${fresh.length}  clean: ${cleanN}  novel: ${novelN}  NOISE: ${noiseN}  noiseLn: ${noiseLn}`
  );
  console.log(
    "\n=== NOISE by min distinct rename-pairs against best prior twin ==="
  );
  console.log(
    `(how many DISTINCT identifier substitutions reconcile the statement to an existing prior twin)`
  );
  const row = (label: string, b: { st: number; ln: number }) =>
    console.log(
      `  ${label.padEnd(28)} st=${String(b.st).padStart(5)}  ln=${String(b.ln).padStart(6)}  (${pct(b.ln)}% of noiseLn)`
    );
  row("1 pair (single rename)", buckets.single);
  row("  ...in singleton class", singleInSingletonClass);
  row("  ...in bucket class", singleInBucketClass);
  row("2 pairs", buckets.two);
  row("3-5 pairs", buckets.fewSmall);
  row("6+ pairs (DRIFT)", buckets.many);
  row("len mismatch (hash bug?)", buckets.lenMismatch);

  console.log("\n=== distinctPairs histogram (st / ln) ===");
  for (const dp of [...pairHisto.keys()].sort((a, b) => a - b).slice(0, 20)) {
    const h = pairHisto.get(dp)!;
    console.log(
      `  ${String(dp).padStart(3)} pairs: st=${String(h.st).padStart(5)}  ln=${String(h.ln).padStart(6)}`
    );
  }

  console.log("\n=== top single-rename pairs (the most recoverable slice) ===");
  const ex = [...singleRenameExamples.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 40);
  for (const [k, n] of ex) console.log(`  ${String(n).padStart(4)}x  ${k}`);
}

main();

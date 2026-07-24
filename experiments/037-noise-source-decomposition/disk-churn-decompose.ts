/**
 * On-disk churn decomposition (exp037): the git line-diff of the split src tree
 * is what the user actually commits and reviews. It mixes four sources the
 * order-blind noiseLn metric cannot separate:
 *
 *   1. REAL change     — statements whose structure (hash) is novel in a file.
 *   2. NAMING drift    — statements whose hash matches a prior twin IN THE SAME
 *                        FILE but whose text differs (a name changed).
 *   3. REORDER churn    — statements byte-identical in both versions of a file but
 *                        emitted in a different POSITION (Myers counts delete+add;
 *                        an order-stable emitter would zero this).
 *   4. RELOCATION churn — statements whose home file changed (present identically
 *                        somewhere in the other tree, just not in this file).
 *
 * Method: parse every src file in both trees into top-level statements
 * (hash+text+lines). Per file, an "order-insensitive" symmetric difference on
 * (hash,text) isolates real+naming from reorder. Cross-file identity isolates
 * relocation. The residual gap vs the raw git Myers churn is pure reorder.
 *
 * Usage: npx tsx disk-churn-decompose.ts <priorSrcDir> <freshSrcDir>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { statementHash } from "../../src/split/statement-hash.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
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
      hash: statementHash(stmt),
      text,
      lines: text.length ? text.split("\n").length : 0
    };
  });
}

/** exact (hash,text) multiset key */
const exactKey = (s: Stmt) => `${s.hash}\x00${s.text}`;

function main() {
  const [priorDir, freshDir] = process.argv.slice(2);
  const priorFiles = new Set(walk(priorDir));
  const freshFiles = new Set(walk(freshDir));

  // Global exact-statement index (for relocation detection): key -> {prior,fresh} counts
  const globalPrior = new Map<string, number>();
  const globalFresh = new Map<string, number>();

  interface FileParse {
    stmts: Stmt[];
  }
  const priorParsed = new Map<string, FileParse>();
  const freshParsed = new Map<string, FileParse>();

  for (const f of priorFiles) {
    const s = statementsOfFile(fs.readFileSync(path.join(priorDir, f), "utf8"));
    priorParsed.set(f, { stmts: s });
    for (const st of s)
      globalPrior.set(exactKey(st), (globalPrior.get(exactKey(st)) ?? 0) + 1);
  }
  for (const f of freshFiles) {
    const s = statementsOfFile(fs.readFileSync(path.join(freshDir, f), "utf8"));
    freshParsed.set(f, { stmts: s });
    for (const st of s)
      globalFresh.set(exactKey(st), (globalFresh.get(exactKey(st)) ?? 0) + 1);
  }

  let realLn = 0; // novel hash in-file
  let namingLn = 0; // hash matches in-file, text differs
  let relocatedLn = 0; // exact statement exists elsewhere in the other tree
  let addedFileLn = 0; // whole new files
  let removedFileLn = 0; // whole removed files
  let orderInsensitiveResidual = 0; // matched exact in-file (should be 0 churn)

  const common = [...freshFiles].filter((f) => priorFiles.has(f));

  for (const f of common) {
    const fresh = freshParsed.get(f)!.stmts;
    const prior = priorParsed.get(f)!.stmts;
    // in-file hash -> set of texts (prior)
    const priorTextByHash = new Map<string, Set<string>>();
    const priorExact = new Map<string, number>();
    for (const s of prior) {
      (
        priorTextByHash.get(s.hash) ??
        priorTextByHash.set(s.hash, new Set()).get(s.hash)!
      ).add(s.text);
      priorExact.set(exactKey(s), (priorExact.get(exactKey(s)) ?? 0) + 1);
    }
    const freshExact = new Map<string, number>();
    for (const s of fresh)
      freshExact.set(exactKey(s), (freshExact.get(exactKey(s)) ?? 0) + 1);

    // Fresh side: classify each fresh statement not exactly matched in-file.
    for (const s of fresh) {
      const k = exactKey(s);
      const inFilePrior = priorExact.get(k) ?? 0;
      if (inFilePrior > 0) {
        // exact match in-file (order-insensitive clean) -> reorder-only if any
        orderInsensitiveResidual += 0;
        continue;
      }
      const texts = priorTextByHash.get(s.hash);
      if (texts && texts.size > 0) {
        namingLn += s.lines; // same shape in-file, text drifted
      } else if ((globalPrior.get(k) ?? 0) > 0) {
        relocatedLn += s.lines; // identical statement lived in another file before
      } else {
        realLn += s.lines; // genuinely new structure
      }
    }
  }

  for (const f of freshFiles)
    if (!priorFiles.has(f)) {
      for (const s of freshParsed.get(f)!.stmts) {
        if ((globalPrior.get(exactKey(s)) ?? 0) > 0) relocatedLn += s.lines;
        else addedFileLn += s.lines;
      }
    }
  for (const f of priorFiles)
    if (!freshFiles.has(f)) {
      for (const s of priorParsed.get(f)!.stmts) {
        if ((globalFresh.get(exactKey(s)) ?? 0) > 0) relocatedLn += s.lines;
        else removedFileLn += s.lines;
      }
    }

  console.log(
    "=== ON-DISK churn decomposition (fresh-side line attribution) ==="
  );
  console.log(`  REAL change (novel structure in-file):   ${realLn}`);
  console.log(`  NAMING drift (same shape, name changed): ${namingLn}`);
  console.log(`  RELOCATION (identical stmt, moved file):  ${relocatedLn}`);
  console.log(`  ADDED files (whole new):                  ${addedFileLn}`);
  console.log(`  REMOVED files (whole gone):               ${removedFileLn}`);
  console.log(`  (matched exact in-file => only REORDER can churn these)`);
  console.log(
    `\n  Order-insensitive total (real+naming+reloc+add+rem): ${realLn + namingLn + relocatedLn + addedFileLn + removedFileLn}`
  );
  console.log(
    `  git Myers churn was ~68,768 (add+del). Gap ≈ reordering churn.`
  );
  console.log(
    `  files: prior=${priorFiles.size} fresh=${freshFiles.size} common=${common.length}`
  );
}

main();

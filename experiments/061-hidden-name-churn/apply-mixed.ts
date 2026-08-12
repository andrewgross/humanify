/**
 * 061 — materialize the mixed-hunk tier's renames into a patched copy of a
 * tree, so the 055 ledgers can price the lever with their own instruments.
 *
 *   npx tsx experiments/061-hidden-name-churn/apply-mixed.ts \
 *     <priorSrc> <freshSrc> <outDir>
 *
 * Copies every file of <freshSrc> into <outDir>, replacing the ones the
 * widened pass (exp054 runFile, --mixed --skip-import-decls options)
 * rewrites. Deterministic, no LLM — the pass is downstream of every prompt,
 * which is what licenses this offline application (CLAUDE.md, post-split
 * reconcile note).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  applySubstitutions,
  runFile,
  walk
} from "../054-post-split-reconcile/pass.js";

const [PRIOR, FRESH, OUT] = process.argv.slice(2);
if (!PRIOR || !FRESH || !OUT) {
  console.error("usage: apply-mixed.ts <priorSrc> <freshSrc> <outDir>");
  process.exit(1);
}

const priorFiles = new Set(walk(PRIOR));
let rewritten = 0;
let renames = 0;
for (const f of walk(FRESH)) {
  const dest = path.join(OUT, f);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  const freshText = fs.readFileSync(path.join(FRESH, f), "utf8");
  if (!priorFiles.has(f)) {
    fs.writeFileSync(dest, freshText);
    continue;
  }
  const res = runFile(PRIOR, FRESH, f, true, {
    mixedHunkTier: true,
    skipImportDeclarations: true
  });
  if (res.status === "ok" && res.subs && res.freshLines) {
    fs.writeFileSync(dest, applySubstitutions(res.freshLines, res.subs));
    rewritten++;
    renames += res.renames.length;
  } else {
    fs.writeFileSync(dest, freshText);
  }
}
console.log(`patched tree at ${OUT}: ${rewritten} files, ${renames} renames`);

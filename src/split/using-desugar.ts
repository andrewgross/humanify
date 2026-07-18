/**
 * `using`/`await using` desugaring for the runnable split tree.
 *
 * Why: Bun cannot `require()` a CommonJS-marked module containing `using`
 * (oven-sh/bun#11100 — its transpiler injects ESM `bun:wrap` imports into
 * the CJS wrapper), and Node < 24 cannot parse the syntax at all. Since a
 * Bun application's real workloads NEED the Bun runtime (`Bun.*` APIs),
 * the runnable tree compiles explicit-resource-management away entirely:
 * Babel's official transform rewrites each declaration into the equivalent
 * try/finally with real `Symbol.dispose`/`Symbol.asyncDispose` calls —
 * faithful disposal order, error suppression, and awaiting, unlike the
 * lossy `HUMANIFY_STRIP_USING` regex strip.
 *
 * Scope discipline: only files that genuinely DECLARE with `using` are
 * regenerated (a cheap token prefilter, then a parse-level check — the
 * word "using" in a comment or string must not churn a file's formatting).
 * Everything else stays byte-identical. `retainLines` keeps statements on
 * their original lines so stack traces and cross-version diffs stay
 * aligned. The transform is deterministic. The review tree (--split-pure)
 * and `.humanify/humanified.js` (the prior-version matching target) are
 * never touched — the tree pass walks the same file set as the external-
 * dep scan, which excludes node_modules and the metadata dir.
 */

import { readFile, writeFile } from "node:fs/promises";
import * as path from "node:path";
import { transformSync } from "@babel/core";
import * as t from "@babel/types";
import { parseFileAst } from "../babel-utils.js";
import { jsFilesUnder } from "./runnable-scaffold.js";

/** Cheap reject: a file with no `using` token cannot declare with it. */
const USING_TOKEN = /\busing\b/;

/** True when the AST really declares with `using` / `await using`. */
function declaresUsing(ast: t.File): boolean {
  let found = false;
  t.traverseFast(ast, (node) => {
    if (
      t.isVariableDeclaration(node) &&
      (node.kind === "using" || node.kind === "await using")
    ) {
      found = true;
    }
  });
  return found;
}

/**
 * Desugar every `using`/`await using` declaration in `code`, or return
 * null when there is nothing to transform (callers keep the original
 * bytes). Throws when the transform fails — a runnable tree with live
 * `using` would be unloadable under Bun and Node < 24, so silently
 * keeping the file would ship the exact failure this pass exists to fix.
 */
export function desugarUsing(code: string): string | null {
  if (!USING_TOKEN.test(code)) return null;
  const ast = parseFileAst(code);
  if (!ast || !declaresUsing(ast)) return null;
  const result = transformSync(code, {
    plugins: ["@babel/plugin-transform-explicit-resource-management"],
    configFile: false,
    babelrc: false,
    sourceType: "unambiguous",
    retainLines: true,
    compact: false
  });
  if (!result?.code) {
    throw new Error("using desugar: transform produced no output");
  }
  return result.code;
}

/**
 * Desugar `using` across a written runnable tree (split files AND relinked
 * vendor factories — both are require()d and both may carry the syntax).
 * Returns the number of files rewritten.
 */
export async function desugarUsingInTree(outputDir: string): Promise<number> {
  let transformed = 0;
  for (const file of await jsFilesUnder(outputDir)) {
    const code = await readFile(file, "utf-8");
    const out = desugarUsing(code);
    if (out === null) continue;
    await writeFile(file, out);
    transformed++;
  }
  return transformed;
}

/** Human-readable label for progress messages. */
export function desugarSummary(outputDir: string, count: number): string {
  return count > 0
    ? `Desugared \`using\` in ${count} file(s) under ${path.basename(outputDir)} (Bun cannot require CJS+using, bun#11100; Node < 24 cannot parse it)`
    : "No `using` declarations to desugar";
}

import { existsSync, readdirSync } from "node:fs";
import * as path from "node:path";
import { err } from "./cli-error.js";

export function ensureFileExists(filename: string) {
  if (!existsSync(filename)) {
    err(`File ${filename} not found`);
  }
}

/**
 * Recursively list JS files under `dir`, returning paths relative to
 * `rootDir` (defaults to `dir`). Skips `node_modules`. Matches `.js` by
 * default; pass `exts` to broaden (e.g. [".js", ".cjs", ".mjs"]).
 */
export function listJsFilesRecursive(
  dir: string,
  rootDir: string = dir,
  exts: readonly string[] = [".js"]
): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules") continue;
      // `.humanify/` is pipeline METADATA — the split ledger, stats, and
      // `humanified.js`, which is the whole tree's code carried in one file for
      // the next release. Walking it double-counts every emitted source file.
      // The other two tree walkers (`jsFilesUnder`, `treeFiles`) already skip
      // it; this one diverging was ranked #3 in docs/responsibility.md's
      // still-duplicated list, and `env-reads` on a split tree was the live
      // consequence.
      if (entry.name === ".humanify") continue;
      results.push(...listJsFilesRecursive(fullPath, rootDir, exts));
    } else if (exts.some((e) => entry.name.endsWith(e))) {
      results.push(path.relative(rootDir, fullPath));
    }
  }
  return results;
}

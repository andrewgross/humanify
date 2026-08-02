/**
 * ONE changed-line counter for every experiment.
 *
 * ## Why
 *
 * There were at least twelve independent implementations across
 * `experiments/`, and they did not agree:
 *
 *  - most counted `<`/`>` (normal diff); `058/ceiling-ab.ts` counted `+`/`-`
 *    (unified diff, skipping `---`/`+++`/`@@` headers)
 *  - some passed `-r` (trees), most did not (files)
 *  - some passed `-N` (treat absent as empty), most did not — which silently
 *    drops added and deleted FILES from a tree comparison
 *  - several returned **0 when `diff` produced no stdout**, so a missing `diff`
 *    binary reported "no change" instead of failing
 *
 * Mixing numbers from any two of those is apples to oranges, and the last one
 * is the same silent-skip failure as a boot gate that passes when `bun` is
 * absent.
 *
 * ## What this counts
 *
 * **Changed lines = the number of `<` and `>` lines in a normal `diff`.** A
 * modified line counts TWICE (once removed, once added), which is what `git
 * diff --numstat` reports and what every published figure in this repo means
 * by "git lines". Header and separator lines are never counted.
 *
 * Both entry points fail loudly if `diff` cannot run.
 */
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { computeNormalDiff } from "../../src/rename/diff-reconcile.js";
import { listJsFilesRecursive } from "../../src/file-utils.js";

/** `diff` output can be very large on a whole tree. */
const MAX_BUFFER = 1 << 30;

/**
 * Changed lines between two TEXTS.
 *
 * Delegates to the production `computeNormalDiff`, which already normalises
 * CRLF and throws when `diff` is unavailable rather than reporting zero.
 */
export function changedLines(priorText: string, freshText: string): number {
  return countMarkers(computeNormalDiff(priorText, freshText));
}

/** `<` / `>` lines in normal-diff output. */
function countMarkers(diffText: string): number {
  let n = 0;
  for (const line of diffText.split("\n")) {
    if (line.startsWith("<") || line.startsWith(">")) n++;
  }
  return n;
}

export interface TreeDiff {
  /** Total changed lines across the tree. */
  total: number;
  /** Per file, relative to `priorDir`. Only files that differ appear. */
  byFile: Map<string, number>;
}

/**
 * Changed lines between two TREES, per file and in total.
 *
 * Always `-r -N`: without `-N` a file that exists on only one side is reported
 * as "Only in …" and contributes ZERO, which quietly excludes every added and
 * deleted file from the number — the largest single category of churn in a
 * release diff.
 */
export function changedLinesInTree(
  priorDir: string,
  freshDir: string
): TreeDiff {
  const r = spawnSync("diff", ["-rN", priorDir, freshDir], {
    encoding: "utf-8",
    maxBuffer: MAX_BUFFER
  });
  // 0 = identical, 1 = differences. Anything else — including a spawn failure
  // (status null when `diff` is not on PATH) — must not read as "no change".
  if (r.status !== 0 && r.status !== 1) {
    const detail = r.error?.message || r.stderr || "unknown error";
    throw new Error(`diff -rN failed: ${detail}`);
  }
  const byFile = new Map<string, number>();
  let current = "";
  for (const line of (r.stdout ?? "").split("\n")) {
    // `diff -rN a/x b/x` announces each file pair before its hunks.
    if (line.startsWith("diff -rN ")) {
      const parts = line.split(" ");
      const left = parts[parts.length - 2];
      current = left.startsWith(priorDir)
        ? path.relative(priorDir, left)
        : left;
      continue;
    }
    if (line.startsWith("<") || line.startsWith(">")) {
      byFile.set(current, (byFile.get(current) ?? 0) + 1);
    }
  }
  let total = 0;
  for (const n of byFile.values()) total += n;
  return { total, byFile };
}

/**
 * Files present in one tree and not the other.
 *
 * Reported separately because a ceiling must never net created lines against
 * removed ones, and a whole added file is the extreme case of that.
 */
export function treeFileDelta(
  priorDir: string,
  freshDir: string
): { added: string[]; removed: string[] } {
  const prior = new Set(listJsFilesRecursive(priorDir, priorDir));
  const fresh = new Set(listJsFilesRecursive(freshDir, freshDir));
  return {
    added: [...fresh].filter((f) => !prior.has(f)).sort(),
    removed: [...prior].filter((f) => !fresh.has(f)).sort()
  };
}

/** Read a file, or "" when absent — for comparing against a side that lacks it. */
export function readOrEmpty(p: string): string {
  return fs.existsSync(p) ? fs.readFileSync(p, "utf8") : "";
}

/**
 * Reading an emitted tree, a ledger, and a bundle's statements — once.
 *
 * ## The bug this prevents
 *
 * Experiment scripts derive "the top-level statements" two different ways, and
 * BOTH are correct — for different inputs:
 *
 *  - a **bundle** (`.humanify/humanified.js`) wraps everything in an IIFE, so
 *    the statements live in `wrapper.functionPath.node.body.body`. Reading
 *    `ast.program.body` there returns ONE statement: the IIFE itself.
 *  - a **split file** (`src/**\/*.js`) is a plain module, so `ast.program.body`
 *    is exactly right and there is no wrapper to find.
 *
 * A sweep of `experiments/` flagged ~7 scripts as "missing the wrapper". They
 * all take split-tree directories, so all of them were right — the sweep
 * compared them without checking what each is fed. But nothing DECLARES which
 * shape a helper expects, so pointing one at the wrong input returns a
 * plausible near-zero count instead of an error.
 *
 * These two functions each state their input shape and throw when given the
 * other, which is the actual fix.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { listJsFilesRecursive } from "../../src/file-utils.js";
import { METADATA_DIR } from "../../src/split/layout.js";

/** Emitted `.js` files under a tree, relative to it, `.humanify/` excluded. */
export function treeFiles(dir: string): string[] {
  return listJsFilesRecursive(dir, dir).filter(
    (rel) => !rel.split(path.sep).includes(METADATA_DIR)
  );
}

/**
 * Top-level statements of a BUNDLE — the wrapper IIFE's body.
 *
 * Throws when the input has no wrapper, rather than silently returning the one
 * statement `ast.program.body` would give. Use `fileStatements` for split files.
 */
export function bundleStatements(
  code: string,
  label = "bundle"
): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`${label}: could not parse`);
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) {
    throw new Error(
      `${label}: no wrapper IIFE found. This function is for BUNDLES ` +
        "(.humanify/humanified.js). For an emitted split file use fileStatements()."
    );
  }
  const body = wrapper.functionPath.node.body;
  if (body.type !== "BlockStatement") {
    throw new Error(`${label}: wrapper body is not a block`);
  }
  return body.body;
}

/**
 * Top-level statements of an emitted SPLIT FILE — a plain module body.
 *
 * Warns loudly if handed something wrapper-shaped, which would mean the caller
 * meant `bundleStatements`.
 */
export function fileStatements(code: string, label = "file"): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`${label}: could not parse`);
  const body = ast.program.body;
  if (body.length === 1 && findWrapperFunction(ast)) {
    throw new Error(
      `${label}: this looks like a BUNDLE (a single wrapper IIFE), not an ` +
        "emitted split file. Use bundleStatements()."
    );
  }
  return body;
}

/**
 * The ledger type is the PRODUCTION one, re-exported — not a copy.
 *
 * Experiments each declared their own `{hashes, order, nameToFiles}` shape and
 * every copy was a chance to drift from what the splitter actually writes.
 */
export type { StableSplitLedger as SplitLedger } from "../../src/split/stable-split.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

/** Read a tree's split ledger, failing with the path rather than a JSON error. */
export function readLedger(treeDir: string): StableSplitLedger {
  const p = path.join(treeDir, METADATA_DIR, "split-ledger.json");
  if (!fs.existsSync(p)) throw new Error(`no split ledger at ${p}`);
  return JSON.parse(fs.readFileSync(p, "utf8")) as StableSplitLedger;
}

/** A tree's bundle — what `--prior-version` points the next release at. */
export function readBundle(treeDir: string): string {
  const p = path.join(treeDir, METADATA_DIR, "humanified.js");
  if (!fs.existsSync(p)) throw new Error(`no bundle at ${p}`);
  return fs.readFileSync(p, "utf8");
}

/**
 * The binding-identity map the split's `preempt` / `fill` tiers consume.
 *
 * Absent on some runs, and **absence is not emptiness**: building a carry with
 * an empty map silently switches those two tiers off, which is how exp058's
 * first reconstruction "proved" that identity never dissents. Callers get
 * `null` so they can tell the difference and say so.
 */
export function readMatchMap(treeDir: string): Map<string, string> | null {
  const p = path.join(treeDir, METADATA_DIR, "prior-match-map.json");
  if (!fs.existsSync(p)) return null;
  return new Map(
    Object.entries(
      JSON.parse(fs.readFileSync(p, "utf8")) as Record<string, string>
    )
  );
}

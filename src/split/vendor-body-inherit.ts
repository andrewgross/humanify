/**
 * Cross-release reuse of vendored library bodies.
 *
 * humanify deliberately does NOT name the ~1,650 vendored library files (the
 * "Skipping N library files" line in any run log) — at that count the LLM
 * pass costs more than it is worth. The consequence is that Bun's minifier
 * reroll of every local passes straight through to the emitted tree: on each
 * gate hop essentially EVERY common vendor file changes text, and no library
 * changed with it.
 *
 * Measured across the four gate hops (experiments/046-vendor-noise):
 *
 *   bucket                                    files/hop   lines (4 hops)
 *   local-name reroll only                      ~1,540        13,980
 *   free minified token reroll                       4            44
 *   genuinely edited library bodies                 <8            82
 *
 * So 99% of body churn is a rename of code nobody reads. This pass writes the
 * PRIOR release's bytes whenever the two files are the same program, which
 * removes those files from the diff entirely.
 *
 * ## The key is the whole safety argument
 *
 * `computeStructuralSignature` serializes the file with every BINDING
 * replaced by a slot ordinal and every literal, property key, operator, free
 * identifier and regex kept VERBATIM. Two files sharing it differ only in the
 * spelling of their local bindings, so substituting one for the other cannot
 * change behaviour.
 *
 * It is emphatically NOT `structuralHash`, the manifest's cross-version join
 * key, which serializes with `preserveLiterals: false` — that keeps only a
 * string's LENGTH and a number's order-of-magnitude bucket, so two modules
 * differing in an endpoint URL, a timeout, or any same-length constant share
 * it. Keying reuse on it would silently ship a vendored library carrying the
 * previous release's constants. Precision over recall, and a vendored library
 * is the last place to guess.
 *
 * Require paths are deliberately IN the key rather than normalized away. A
 * body matched while ignoring its imports could require a path this tree does
 * not have; keeping them means a match is drop-in bytes. It costs almost
 * nothing — 13,900 recoverable lines instead of 13,980.
 *
 * Kill switch: `HUMANIFY_NO_VENDOR_INHERIT=1`.
 */
import fs from "node:fs";
import path from "node:path";
import { parseSourceAst, traverse } from "../babel-utils.js";
import { computeStructuralSignature } from "../analysis/structural-hash.js";
import { debug } from "../debug.js";
import type { NodePath } from "@babel/traverse";

/** Set to disable body inheritance and emit freshly-rendered bodies. */
export const VENDOR_INHERIT_OFF_ENV = "HUMANIFY_NO_VENDOR_INHERIT";

export interface VendorInheritStats {
  /** Files that had a prior counterpart and were compared. */
  considered: number;
  /** Files written with the prior release's bytes. */
  inherited: number;
}

export interface VendorBodyInheritor {
  /**
   * The bytes to write for `relPath` (relative to the OUTPUT ROOT, e.g.
   * `vendor/axios.js`), given what this run rendered. Returns `fresh`
   * unchanged unless the prior release holds the same program at the same
   * path.
   */
  bytesFor(relPath: string, fresh: string): string;
  stats(): VendorInheritStats;
}

/**
 * Rename-invariant, literal-PRESERVING signature of a whole file, or null
 * when it does not parse. A file that cannot be parsed is never inherited —
 * an unverifiable match is not a match.
 */
function fileSignature(code: string): string | null {
  try {
    const ast = parseSourceAst(code);
    if (!ast) return null;
    let sig: string | null = null;
    traverse(ast, {
      Program(p: NodePath) {
        sig = computeStructuralSignature(p);
        p.stop();
      }
    });
    return sig;
  } catch {
    return null;
  }
}

/**
 * Cheap necessary condition, so the ~1% of files that really changed are
 * rejected without two parses. Renaming a binding rewrites only
 * identifier-shaped runs, so the text with every such run removed is
 * rename-invariant: if those skeletons differ, no rename can reconcile the
 * files. Identifier-like text inside string literals is stripped too, which
 * only lets more candidates through to the exact check — never fewer.
 */
const IDENTIFIER_RUN = /[A-Za-z_$][A-Za-z0-9_$]*/g;
const skeleton = (code: string): string => code.replace(IDENTIFIER_RUN, "");

/**
 * `priorRoot` is the prior release's TREE ROOT (the directory holding
 * `vendor/`), not the vendor directory itself, so `relPath` resolves against
 * it unchanged. Returns undefined when there is no prior tree or the kill
 * switch is set — callers then emit freshly-rendered bodies.
 */
export function createVendorBodyInheritor(
  priorRoot: string | undefined
): VendorBodyInheritor | undefined {
  if (!priorRoot) return undefined;
  if (process.env[VENDOR_INHERIT_OFF_ENV]) {
    debug.log("bun-relink", "vendor body inheritance disabled by kill switch");
    return undefined;
  }

  const stats: VendorInheritStats = { considered: 0, inherited: 0 };

  return {
    bytesFor(relPath: string, fresh: string): string {
      let prior: string;
      try {
        prior = fs.readFileSync(path.join(priorRoot, relPath), "utf-8");
      } catch {
        return fresh;
      }
      stats.considered++;
      // Already identical — the self-hop case, and nothing to gain otherwise.
      if (prior === fresh) return fresh;
      if (skeleton(prior) !== skeleton(fresh)) return fresh;
      const priorSig = fileSignature(prior);
      if (priorSig === null) return fresh;
      if (priorSig !== fileSignature(fresh)) return fresh;
      stats.inherited++;
      return prior;
    },
    stats: () => ({ ...stats })
  };
}

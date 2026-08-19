/**
 * THE owner of "what statement encloses this function, is it usable as
 * identity evidence, and what does it hash to".
 *
 * Extracted from `fingerprint-index.ts` because propagation needs the same
 * answers and importing them from there would be a cycle — `fingerprint-index`
 * already imports `propagate`. Two copies of the 50-line cap, one per caller,
 * is precisely the duplication `docs/responsibility.md` exists to prevent: not
 * two functions that look alike, but two that answer the same question
 * DIFFERENTLY with nothing declaring the difference.
 *
 * The enclosing statement is the only address an anonymous function has. It is
 * the matcher's #2 resolver at ~13,000 matches per hop, and its properties are
 * measured in `experiments/079-ambiguity/RESULTS.md`.
 */
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { analysisCacheForPath } from "./analysis-cache.js";
import { hashPathWithMapping } from "./structural-hash.js";
import type { FunctionNode } from "./types.js";

/** Enclosing statements above this loc span carry too much unrelated code
 *  (and cost too much to hash) to serve as identity evidence.
 *
 *  UNMEASURED WHEN CHOSEN (exp079, arrived with the feature in e311b3b). What
 *  IS measured: it excludes 1,340 of the 18,844 functions that reach the rung
 *  on 2.1.215->216 — 7.1%. A bundle-wide census made it look ~6x larger; most
 *  functions in big statements never reach the rung at all. */
export const MAX_ENCLOSING_STMT_LINES = 50;

/**
 * Is a statement usable as identity evidence, and how big is it? THE owner of
 * the cap — the hash path and the abstain diagnostics both ask this, so a
 * counter reporting "excluded by the cap" cannot drift from the rule that
 * actually excluded it.
 *
 * `span` is `end.line - start.line + 1` from the parser's source positions,
 * which measures the GENERATOR'S line breaking rather than how much code the
 * statement holds. Comparable across versions only because both sides run
 * through babel-generator non-compact.
 */
export function statementUsability(node: t.Node | null | undefined): {
  usable: boolean;
  span: number | null;
  reason: "ok" | "noNode" | "noLoc" | "tooLong";
} {
  if (!node) return { usable: false, span: null, reason: "noNode" };
  const loc = node.loc;
  if (!loc) return { usable: false, span: null, reason: "noLoc" };
  const span = loc.end.line - loc.start.line + 1;
  if (span > MAX_ENCLOSING_STMT_LINES) {
    return { usable: false, span, reason: "tooLong" };
  }
  return { usable: true, span, reason: "ok" };
}

/** Hash one statement path with the shared cap and per-AST node cache. */
export function hashStatementPath(
  stmt: NodePath | null,
  stmtHashByNode: Map<t.Node, string>
): string | null {
  const node = stmt?.node;
  if (!stmt || !node) return null;
  if (!statementUsability(node).usable) return null;
  const known = stmtHashByNode.get(node);
  if (known !== undefined) return known;
  try {
    const { hash } = hashPathWithMapping(stmt);
    stmtHashByNode.set(node, hash);
    return hash;
  } catch {
    return null;
  }
}

/**
 * Rename-invariant hash of a FUNCTION's enclosing statement. null when there
 * is no usable statement: the function IS the statement (a declaration — zero
 * added context), the statement exceeds the cap, or hashing fails.
 *
 * Memoized per statement NODE in the owning AST's cache, because several
 * functions routinely share one enclosing statement (multiple arrows in one
 * options object) and that sharing is the whole point of the signal.
 */
export function enclosingStatementHash(fn: FunctionNode): string | null {
  const stmt = fn.path.getStatementParent();
  if (!stmt || stmt.node === fn.path.node) return null;
  return hashStatementPath(stmt, analysisCacheForPath(fn.path).stmtHashByNode);
}

/** Span buckets for reporting, ordered. The cap falls between the 3rd and 4th. */
export function spanBucket(span: number | null): string {
  if (span === null) return "unknown";
  if (span < 10) return "1-9";
  if (span < 25) return "10-24";
  if (span < 50) return "25-49";
  if (span < 100) return "50-99";
  if (span < 200) return "100-199";
  if (span < 500) return "200-499";
  return "500+";
}

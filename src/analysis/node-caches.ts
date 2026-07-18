/**
 * Registry for the module-level AST-node-keyed WeakMap caches (identifier →
 * binding, statement → hash, function → shingles). The caches are pure
 * memoization, but their backing ephemeron tables outlive the ASTs they were
 * filled from: every parse-hash-drop cycle (prior-version matching, diff
 * reconcile, minted-name sweep) leaves millions of dead keys behind, and once
 * a table is mostly tombstones V8 re-hashes the whole thing on nearly every
 * insert — the split phase's fresh inserts then run effectively O(n²)
 * (observed as a multi-hour hang on Claude Code 2.1.182). Resetting at a
 * phase boundary replaces the husk with a small fresh table; recomputation
 * is deterministic, so dropped entries only cost a re-hash on demand.
 */

const resets: Array<() => void> = [];

/** Called once at module load by each cache owner. */
export function registerNodeCacheReset(reset: () => void): void {
  resets.push(reset);
}

/** Swap every registered cache for a fresh one. Call between pipeline phases
 *  whose ASTs do not overlap (e.g. after rename/reconcile, before the split
 *  re-parses the humanified source). */
export function resetAnalysisNodeCaches(): void {
  for (const reset of resets) {
    reset();
  }
}

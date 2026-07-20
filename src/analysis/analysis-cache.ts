import type { Binding, NodePath, Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import type { FunctionNode } from "./types.js";

/**
 * Per-AST memoization for the analysis/hashing layer.
 *
 * One instance exists per AST root; entries live exactly as long as the AST
 * does and are collected WITH it as a single unit. This is the structural fix
 * for the ephemeron tombstone thrash (docs/issue-ephemeron-cache-thrash.md):
 * the previous module-level WeakMaps outlived every parse-then-drop cycle, so
 * each dead 30MB AST left millions of tombstones in a shared table and V8
 * re-hashed it on nearly every insert of the next phase — O(n²), 100%-CPU
 * hangs. With per-AST scoping there is no shared table to densify: the prior
 * bundle's graph build fills the PRIOR AST's cache, the new bundle's fills its
 * own, and dropping an AST frees its entries wholesale — no boundary resets,
 * no reset-ordering choreography.
 *
 * Plain Maps, deliberately: keys are nodes (or graph nodes) of the owning
 * AST, which the AST keeps alive anyway, and the cache itself is reachable
 * only through the root node — so weakness buys nothing and costs the
 * ephemeron write barrier + GC fixpoint that WeakMaps pay per entry.
 */
export class AnalysisCache {
  /**
   * Identifier occurrence → resolved binding (null = free). Resolution is
   * position-based, so it is safe to memoize for the AST's whole life —
   * ancestors re-hash the same nested identifiers constantly. Values may be
   * Binding objects from an older crawl era; hashing keys slots by the
   * binding's DECLARATION identifier node (era-stable), so a stale Binding
   * still groups correctly (see serializeIdentifier in structural-hash.ts).
   */
  readonly bindingByIdentifier = new Map<t.Identifier, Binding | null>();

  /** Statement node → rename-invariant hash (enclosing-statement evidence). */
  readonly stmtHashByNode = new Map<t.Node, string>();

  /**
   * Function-graph node → shingle set. Keyed by the graph's FunctionNode
   * object (same keying as the old module-level cache), held here so the
   * memoization dies with the AST instead of the process.
   */
  readonly shingleSetByFunction = new Map<FunctionNode, Set<string>>();
}

/**
 * Registry of caches, keyed by AST root node. Module-level, but with one
 * entry per PARSED TREE (a handful per process), not per AST node (millions)
 * — tombstone accumulation is structurally bounded.
 */
const cacheByRoot = new WeakMap<t.Node, AnalysisCache>();

/** The cache owned by an AST root node, created on first use. */
export function analysisCacheForRoot(root: t.Node): AnalysisCache {
  let cache = cacheByRoot.get(root);
  if (!cache) {
    cache = new AnalysisCache();
    cacheByRoot.set(root, cache);
  }
  return cache;
}

/**
 * The cache owned by the tree a scope belongs to: the topmost scope's node
 * (the Program for parsed files, the root scopable for detached snippets).
 * Node identity, not Scope identity, keys the registry — scopes are
 * re-created on crawl, nodes are not.
 */
export function analysisCacheForScope(scope: Scope): AnalysisCache {
  let s = scope;
  while (s.parent) s = s.parent;
  return analysisCacheForRoot(s.path.node);
}

/**
 * The cache owned by the tree a path belongs to. One scope-chain walk per
 * PUBLIC hashing call (per function / per statement, not per identifier) —
 * internal recursion threads the resolved cache explicitly.
 */
export function analysisCacheForPath(path: NodePath): AnalysisCache {
  return analysisCacheForScope(path.scope);
}

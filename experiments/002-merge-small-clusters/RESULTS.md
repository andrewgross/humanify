# Experiment 002 Results: Merge Small Clusters + Absorb Shared

## Results Summary

| Metric | Baseline (001) | With merging (002) | Target |
|--------|---------------|--------------------|--------|
| Clusters | 44 | 29 | 5-20 |
| Shared ratio | 36.8% | 0.0% | < 15% |
| MQ score | 0.205 | 0.207 | > 0.3 |
| Avg cluster size | 1.3 | 3.2 | — |
| Orphans | 2 | 2 | — |

## Key Observations

### 1. Shared reabsorption works perfectly
The merge+reabsorb cycle reduced shared functions from 35 (36.8%) to **zero**. All previously-shared functions were absorbed into the 3 large clusters once their callers merged together. This validates the hypothesis that shared was inflated by over-fragmentation.

### 2. Three large clusters align with Preact's architecture
After merging, three meaningful clusters emerged:
- **Core diff/render (26 members)**: diff, render, createElement, commitRoot, diffChildren, unmount, etc.
- **DOM operations (24 members)**: mountVNode, patchDomNode, syncDomProps, detachNode, etc.
- **Hooks (14 members)**: useState, useEffect, useReducer, useCallback, useContext, etc.

This roughly matches Preact's actual module structure (core + hooks), though core is split into diff/render vs DOM ops.

### 3. 23 isolated singletons remain
23 functions have **zero** top-level callers AND **zero** top-level callees in the static call graph. They cannot be merged by any edge-counting heuristic. These include:

| Function | Why isolated |
|----------|-------------|
| `_catchError` | Called via `options._catchError` (property access, not direct call) |
| `BaseComponent`, `Component` | Constructor functions — instantiated with `new`, not called directly |
| `createRef`, `Fragment` | Exported API — called by user code, not internally |
| `createEventProxy`, `dispatchEvent` | Event handler factories — assigned to variables, not called directly |
| `doRender`, `createInstance` | Called via hooks/options patterns (`options.__r`) |
| `flushAfterPaintEffects`, `afterNextFrame` | Called via `requestAnimationFrame` / setTimeout callbacks |
| 5 anonymous functions | Likely IIFEs or callback assignments |

**Root cause**: Our static analysis only tracks direct `identifier()` call patterns. It misses:
- Property access calls: `options._hook()`, `obj.method()`
- Constructor calls: `new Component()`
- Callback assignments: `requestAnimationFrame(fn)`
- Dynamic dispatch through variables

### 4. minClusterSize plateau
Results were identical for minClusterSize values 3, 5, 8, 10, and 15. All mergeable clusters get merged at size 3 — the remaining singletons have no edges at all.

### 5. MQ score barely improved
MQ went from 0.205 → 0.207 despite dramatically better cluster structure. This is because:
- The 23 singletons each contribute MQ = 1.0 (perfect intra, no inter), which inflates the average
- The formula `(1/k) * sum(MF_i)` penalizes having many clusters regardless of quality

## Size Distribution

| Size | Count |
|------|-------|
| 1 | 23 |
| 2 | 3 |
| 14 | 1 |
| 24 | 1 |
| 26 | 1 |

## What Worked
- Merge-by-edge-count correctly collapses related small clusters
- Shared-function traversal (counting edges through shared fns) enables indirect connections
- Reabsorb loop correctly identifies when shared functions become single-owner after merging
- Algorithm converges quickly (1-2 rounds)

## What Didn't Work
- Can't merge nodes with zero edges — no amount of minClusterSize helps
- MQ metric doesn't reward consolidating singletons into larger clusters

## Next Steps: Experiment 003

**Approach: Source proximity fallback for isolated functions**

Since experiment 001 showed that modules are contiguous in scope-hoisted bundles (Rollup preserves module order), we can use source line proximity as a fallback for functions that have no call graph connections:

1. After call-graph clustering + merging, identify remaining singletons
2. For each singleton, find the nearest non-singleton cluster by line distance
3. Merge the singleton into that cluster

This should collapse the 23 singletons into the 3 large clusters based on where they appear in the source, which correlates with their original module membership.

**Alternative**: Instead of proximity, we could also try:
- Expanding call graph analysis to detect property access calls (`options._hook()`)
- Using variable reference analysis (if `_catchError` is referenced by `options._catchError = _catchError`, track that)
- But these are more complex and may not generalize beyond Preact's patterns

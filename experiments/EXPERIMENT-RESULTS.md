# Reference Clustering Experiment Results

## Problem Statement

When bundlers like esbuild hoist all functions to the top level, the call graph becomes empty (>70% sparse). We need an alternative signal to group functions back into their original source files. The key challenge: **minification renames identifiers**, destroying the primary signal.

## Fixtures

| Fixture | Functions | Lines | Original Files | Description |
|---------|----------|-------|---------------|-------------|
| zod | 422 | 4,417 | 8 | Zod validation library, esbuild ESM bundle |
| hono | 110 | 2,113 | 14 | Hono web framework, esbuild ESM bundle |
| zod-minified | 422 | 2 | 8 | zod run through terser (mangle + compress) |
| hono-minified | 110 | 2 | 14 | hono run through terser |

## Baseline (pre-clustering)

Old call-graph approach: every function gets its own file.

| Fixture | Split Files | ARI |
|---------|------------|-----|
| zod | 212 | 0.128 |
| hono | 99 | 0.029 |

## Phase B: Signal Analysis (Experiment 007)

**Hypothesis**: Functions from the same source file reference the same variable names more than functions from different files. Measured via Jaccard similarity of referenced name sets.

| Fixture | Separation Ratio | IDF Separation | Sparsity | `__name()` | `__export()` |
|---------|-----------------|----------------|----------|------------|-------------|
| zod | 1.84x (MODERATE) | 1.71x | 78.0% | 62 | 1 |
| hono | 4.75x (STRONG) | 6.51x | 91.8% | 19 | 0 |
| zod-minified | 0.89x (WEAK) | 0.73x | 78.2% | 62 | 1 |
| hono-minified | 1.15x (WEAK) | 1.87x | 92.7% | 18 | 0 |

**Findings**:
- Unminified signal exists (1.84x-4.75x) but medians are 0.0 — most pairs share nothing
- Minification destroys signal (0.89x for zod-minified — below random)
- Bundler `__name()` calls survive minification but are too few to build a graph
- Sparsity confirms call-graph clustering is useless (>70% everywhere)

## Current Baseline (with reference clustering)

IDF-weighted Jaccard similarity on bare identifier names, agglomerative community detection.

| Fixture | Split Files | ARI | Target ARI |
|---------|------------|-----|-----------|
| zod | 7 | **0.329** | 0.40 |
| hono | 2 | -0.006 | 0.30 |
| zod-minified | 3 | **0.324** | 0.25 |
| hono-minified | 1 | 0.000 | 0.20 |

---

## Round 1: Token-Based Improvements

### Idea A: Minification-Resistant Reference Sets

**Hypothesis**: Collecting tokens that survive minification (property names, string literals, object keys, numeric literals) instead of bare identifiers should maintain signal quality through minification.

**Approach**: New `collectMinificationResistantNames()` collecting `prop:headers`, `str:GET`, `key:method`, `num:200`, `tpl:Error:`, `global:Promise` instead of bare identifiers.

| Fixture | Baseline ARI | Idea A ARI | Delta |
|---------|-------------|-----------|-------|
| zod | 0.329 (7 files) | 0.000 (1 file) | -0.329 |
| zod-minified | 0.324 (3 files) | 0.000 (1 file) | -0.324 |
| hono | -0.006 (2 files) | 0.000 (1 file) | +0.006 |
| hono-minified | 0.000 (1 file) | 0.000 (1 file) | 0.000 |

**Result**: FAILED. Resistant tokens are too broadly shared — `prop:_def` in 132/422 functions, creating a hyper-connected graph that collapses into one cluster. IDF cannot rescue tokens this common.

### Idea B: Hybrid Weighted References

**Hypothesis**: Use identifier refs for unminified bundles, resistant tokens for minified bundles, auto-detecting via median function name length.

**Approach**: `collectHybridNames()` returns both sets; `detectMinification()` picks which to use.

| Fixture | Baseline ARI | Idea B ARI | Delta |
|---------|-------------|-----------|-------|
| zod | 0.329 (7 files) | 0.004 (2 files) | -0.325 |
| zod-minified | 0.324 (3 files) | -0.005 (2 files) | -0.329 |
| hono | -0.006 (2 files) | 0.000 (1 file) | +0.006 |
| hono-minified | 0.000 (1 file) | 0.000 (1 file) | 0.000 |

**Result**: FAILED. Even in union mode (identifiers + resistant), the resistant tokens dilute the discriminative identifier signal.

### Idea C: Property Chain Fingerprints

**Hypothesis**: Full member expression chains like `.headers.get` are more specific than individual `.headers` + `.get` — less likely to collide across unrelated files.

**Approach**: `collectPropertyChains()` generating `chain:.headers.get`, combined with identifier refs.

| Fixture | Baseline ARI | Idea C ARI | Delta |
|---------|-------------|-----------|-------|
| zod | 0.329 (7 files) | 0.312 (2 files) | -0.017 |
| zod-minified | 0.324 (3 files) | 0.010 (2 files) | -0.314 |
| hono | -0.006 (2 files) | -0.016 (4 files) | -0.010 |
| hono-minified | 0.000 (1 file) | -0.006 (2 files) | -0.006 |

**Result**: FAILED. Minified property chains like `chain:.a.b` are meaningless noise that dilutes discriminative signal.

### Idea D: Proximity Signal Bonus

**Hypothesis**: Bundlers process files sequentially, so adjacent functions likely came from the same file. Add position-based similarity edges as a supplement to reference similarity.

**Approach**: Add proximity-weighted edges for isolated functions (window=1 adjacency, weight 0.15). Proximity-based orphan assignment fallback.

| Fixture | Baseline ARI | Idea D ARI | Delta |
|---------|-------------|-----------|-------|
| zod | 0.329 (7 files) | 0.329 (7 files) | 0.000 |
| zod-minified | 0.324 (3 files) | 0.324 (3 files) | 0.000 |
| hono | -0.006 (2 files) | -0.006 (2 files) | 0.000 |
| hono-minified | 0.000 (1 file) | 0.000 (1 file) | 0.000 |

**Result**: NO EFFECT. Any proximity edges in the graph cause cascade merges in agglomerative clustering — even tiny weights accumulate across community boundaries. Only safe as orphan assignment fallback (assign to nearest cluster by position instead of largest). Useful infrastructure (column-aware sorting for minified bundles) but no ARI improvement.

### Idea E: AST Shape Fingerprints

**Hypothesis**: Structural tokens (`stmt:IfStatement`, `method:push`, `op:typeof`) survive minification and could distinguish modules with different coding patterns.

**Approach**: `collectStructuralTokens()` added to reference sets.

| Fixture | Baseline ARI | Idea E ARI | Delta |
|---------|-------------|-----------|-------|
| zod | 0.329 (7 files) | 0.312 (5 files) | -0.017 |
| zod-minified | 0.324 (3 files) | 0.324 (3 files) | 0.000 |
| hono | -0.006 (2 files) | -0.006 (2 files) | 0.000 |
| hono-minified | 0.000 (1 file) | 0.000 (1 file) | 0.000 |

**Result**: NO EFFECT. Structural tokens are too ubiquitous — nearly every function has `IfStatement` and `.push()`. IDF correctly gives them near-zero weight, meaning they contribute nothing.

### Round 1 Conclusion

**Every token-based idea made things worse or had no effect.** Adding more tokens to the IDF/Jaccard pipeline cannot solve the minification problem because:
- Tokens that survive minification tend to be broadly shared (low IDF)
- Tokens that are discriminative get destroyed by minification
- The IDF/Jaccard pipeline itself is the bottleneck, not the token vocabulary

---

## Round 2: Non-IDF Approaches

### Idea F: Positional Reference Vectors

**Hypothesis**: Instead of matching by identifier NAME, resolve identifiers to their declaration POSITION via Babel scope analysis. `ParseStatus` renamed to `P` still binds to the declaration at line 50. This should recover unminified-level signal on minified bundles.

**Approach**: `collectPositionalReferences()` traverses with Babel scope analysis enabled, resolves each identifier to its binding site, records `pos:LINE:COL` for top-level bindings.

| Fixture | Baseline ARI | Idea F ARI | Baseline Files | Idea F Files |
|---------|-------------|-----------|---------------|-------------|
| zod | 0.329 | 0.317 | 7 | 9 |
| zod-minified | 0.324 | **0.317** | 3 | **9** |
| hono | -0.006 | -0.005 | 2 | 5 |
| hono-minified | 0.000 | -0.005 | 1 | **5** |

**Result**: PROMISING. **Primary hypothesis confirmed: minification immunity achieved.** zod and zod-minified produce identical results (0.317 ARI, 9 files). hono and hono-minified produce identical results (5 files). Previously zod-minified collapsed to 3 files and hono-minified to 1 file.

Trade-off: slight ARI regression on unminified zod (0.329→0.317) because position strings carry less semantic meaning than names. File count improved for minified cases (zod-minified: 3→9, closer to 8 ground truth; hono-minified: 1→5, closer to 14).

**Key insight**: Positional references should be COMBINED with name-based references (use names when available for semantic signal, positions always for structural signal).

### Idea G: Bundle Gap Detection

**Hypothesis**: Bundlers leave detectable seams between modules — gaps in character positions, shifts in naming patterns. Finding the largest gaps and splitting there should work regardless of minification.

**Approach**: `detectBundleGaps()` computes byte-offset gaps between consecutive functions (70% position gap, 30% size discontinuity). `gapBasedClustering()` splits at the largest gaps. Used as primary method when similarity graph density > 0.5 (characteristic of minified code). Improved `estimateFileCount` for minified bundles using `sqrt(totalFunctions)`.

| Fixture | Baseline ARI (files) | Idea G ARI (files) | Delta |
|---------|---------------------|-------------------|-------|
| zod | 0.329 (7) | 0.329 (7) | 0.000 |
| zod-minified | 0.324 (3) | 0.223 (12) | -0.101 (over-splits) |
| hono | -0.006 (2) | -0.006 (2) | 0.000 |
| hono-minified | 0.000 (1) | **0.528 (8)** | **+0.528** |

**Result**: BREAKTHROUGH for hono-minified (0.000→0.528, 1→8 files, V-Measure 0.750, 88.2% completeness). Bundler gap signal works because esbuild processes files sequentially — byte gaps between functions mark file boundaries even in minified output. Non-minified bundles unchanged (density check routes them to reference clustering). zod-minified regresses because target count overshoots (12 vs GT 8), but homogeneity is 84.9%.

### Idea H: Normalized AST Subtree Hashing

**Hypothesis**: Functions from the same file share coding idioms. Hash individual statements with identifiers replaced by `_` — shared patterns create similarity signal independent of names.

**Approach**: `collectSubtreeHashes()` normalizes each statement (identifiers→`_`, literals→type markers), hashes it, only keeps depth>=5 subtrees. Combined with identifier refs.

| Fixture | Baseline ARI (files) | Idea H ARI (files) | Delta |
|---------|---------------------|-------------------|-------|
| zod | 0.329 (7) | 0.324 (**8 = GT**) | -0.005 ARI, file count matches GT |
| zod-minified | 0.324 (3) | 0.282 (**6**) | -0.042 ARI, +3 files toward GT(8) |
| hono | -0.006 (2) | -0.006 (2) | 0.000 |
| hono-minified | 0.000 (1) | 0.000 (1) | 0.000 |

**Result**: MODEST IMPROVEMENT on file counts. Zod file count now matches ground truth (8). Zod-minified improves from 3→6 files (GT=8). ARI drops slightly because clusters aren't perfectly aligned yet. No effect on hono (too structurally homogeneous). Subtree hashing provides structural differentiation where minified identifiers can't.

### Idea I: Control Flow Shape Vectors

**Hypothesis**: Represent each function as a 20-dimensional count vector (IfStatements, ForLoops, TryCatch, etc.) and use cosine similarity instead of Jaccard. Completely bypasses the name/IDF pipeline.

**Approach**: `computeShapeVector()` returns 20-element feature vector. `cosineSimFromVectors()` for similarity. Added to graph via `augmentGraphWithShapeSimilarity()` with window=20, threshold=0.85, only boosting existing edges for non-complex functions.

| Fixture | Baseline ARI (files) | Idea I ARI (files) | Delta |
|---------|---------------------|-------------------|-------|
| zod | 0.329 (7) | 0.329 (7) | 0.000 |
| zod-minified | 0.324 (3) | 0.324 (3) | 0.000 |
| hono | -0.006 (2) | -0.006 (2) | 0.000 |
| hono-minified | 0.000 (1) | 0.000 (1) | 0.000 |

**Result**: NEUTRAL. Conservative design avoids false edges but provides no new signal. Shape vectors alone can't distinguish same-module from different-module functions because structural feature distributions overlap heavily across modules.

### Idea J: Reference Count Vectors

**Hypothesis**: Count HOW MANY of each category (total refs, unique refs, strings, calls) instead of WHICH ones. Use cosine similarity on normalized count vectors.

| Fixture | Baseline ARI (files) | Idea J ARI (files) | Delta |
|---------|---------------------|-------------------|-------|
| zod | 0.329 (7) | 0.004 (8) | -0.325 |
| zod-minified | 0.324 (3) | 0.019 (2) | -0.305 |
| hono | -0.006 (2) | -0.012 (3) | -0.006 |
| hono-minified | 0.000 (1) | -0.005 (2) | -0.005 |

**Result**: FAILED. Aggregate statistics too coarse — many functions across different modules have similar profiles. Cosine similarity edges add noise that disrupts IDF clustering.

---

## Conclusions

### What works

1. **Name-based IDF/Jaccard** (baseline) — best ARI for unminified bundles where identifier names are meaningful
2. **Gap detection** (G) — breakthrough for minified bundles where reference signals fail; exploits bundler's sequential file processing
3. **Positional references** (F) — achieves minification immunity by resolving to declaration positions instead of names
4. **Subtree hashing** (H) — improves file count resolution through structural pattern matching

### What doesn't work

Adding more tokens to the IDF/Jaccard pipeline (A, B, C, E) universally fails because:
- Minification-resistant tokens are too broadly shared → hyper-connected graph → 1 cluster
- Structural tokens are too ubiquitous → IDF kills them → no effect
- Property chains are noise in minified code → dilutes signal

Aggregate count vectors (I, J) are too coarse to distinguish modules.

### Recommended combination

The optimal strategy should route based on bundle characteristics:
- **Unminified + esbuild comments**: Use esbuild-esm adapter (ARI 1.000)
- **Unminified + sparse call graph**: Use name-based IDF/Jaccard (ARI 0.329)
- **Minified + dense similarity graph**: Use gap-based clustering (ARI 0.528 on hono-minified)
- **Minified + sparse similarity graph**: Use positional references (ARI 0.317, immune to renaming)

Subtree hashing (H) and proximity orphan assignment (D) can supplement any strategy.

---

## Combined Strategy (F+G)

### Implementation

Positional references (F) as default + gap-based clustering fallback (G) when reference signal is too weak.

**Routing logic in `referenceCluster()`:**
1. Build reference sets using positional references (Babel scope analysis → `pos:LINE:COL`)
2. Compute IDF weights and similarity graph
3. Check: if >50% of functions have empty reference sets → gap-based clustering
4. Otherwise: agglomerative clustering on positional-reference similarity graph
5. Proximity-based orphan assignment (nearest cluster by position)

### Results

```
                    |  zod          |  zod-min      |  hono         |  hono-min
────────────────────┼───────────────┼───────────────┼───────────────┼──────────────
Baseline (names)    |  0.329 / 7    |  0.324 / 3    | -0.006 / 2    |  0.000 / 1
Idea F (positional) |  0.317 / 9    |  0.317 / 9    | -0.005 / 5    | -0.005 / 5
Idea G (gaps)       |  0.329 / 7    |  0.223 / 12   | -0.006 / 2    |  0.528 / 8
Combined F+G        |  0.217 / 9    |  0.216 / 9    |  0.578 / 5    |  0.528 / 8
```

### Analysis

- **hono: -0.006 → 0.578** — gap-based fallback triggers (53% empty refs), correctly finds file boundaries
- **hono-minified: 0.000 → 0.528** — same gap-based path, minification-immune
- **zod/zod-minified: identical** (0.217/0.216) — positional refs achieve minification immunity
- **zod ARI regression** (0.329→0.217) — positional refs less discriminative than names; file count (9) closer to GT (8) than baseline (7)
- **All checks pass**: 635 unit tests, 21 fingerprint tests, clean typecheck + lint

### Idea J: Reference Count Vectors

**Hypothesis**: Instead of tracking WHICH identifiers a function uses, count HOW MANY of each category (total refs, unique refs, string count, call count, etc.). Use cosine similarity on normalized count vectors within a proximity window.

**Status**: Running...

# Experiment Plan: Reference Clustering Validation

## Goal

Validate that co-reference clustering improves split quality on sparse call graphs.
The call-graph adapter currently produces terrible results on hoisted bundles:

| Fixture | Original Files | Output Files | ARI   |
| ------- | -------------- | ------------ | ----- |
| zod     | 8              | 212          | 0.128 |
| hono    | 14             | 99           | 0.029 |

Target: ARI >= 0.40 on both fixtures.

## Pre-requisites

All fixtures (zod, hono) already exist. No LLM endpoint needed for Phase A/B.

## Phase A: Create Minified Fixtures

Create terser-minified variants to test the realistic case (mangled names, no comments).

```bash
# Create minified variants
tsx experiments/prepare-minified.ts zod zod-minified
tsx experiments/prepare-minified.ts hono hono-minified
```

Verify source maps chain correctly:

```bash
tsx experiments/run.ts zod-minified --split-strategy esbuild-esm --save zod-minified-esm-check
# If source map works, this should report metrics (even if ARI is low).
# If it errors, source map chaining is broken.
```

## Phase B: Measure Reference Signal (Experiment 007)

Before tuning clustering, verify the signal exists.

```bash
tsx experiments/007-reference-analysis/analyze.ts zod
tsx experiments/007-reference-analysis/analyze.ts hono
tsx experiments/007-reference-analysis/analyze.ts zod-minified
tsx experiments/007-reference-analysis/analyze.ts hono-minified
```

**What to look for:**

- Separation ratio (same-file mean Jaccard / cross-file mean Jaccard)
  - \> 2.0 = strong signal, clustering should work well
  - 1.5-2.0 = moderate, may need tuning
  - < 1.5 = weak, approach may not work
- IDF weighting: does it improve separation?
- Call graph sparsity: should be >70% for hoisted bundles (confirms we'd trigger reference fallback)
- Bundler signals: how many `__name()` and `__export()` patterns survive minification?

**Decision point:** If separation ratio < 1.5 on unminified fixtures, the approach needs rethinking. Stop and reassess.

## Phase C: Baseline the New Clustering

Run the call-graph adapter (which now auto-falls back to reference clustering on sparse graphs) and compare to the old baselines.

```bash
# Unminified fixtures (original names)
tsx experiments/run.ts zod --split-strategy call-graph --save zod-refcluster
tsx experiments/run.ts hono --split-strategy call-graph --save hono-refcluster

# Compare to old baselines
tsx experiments/run.ts zod --split-strategy call-graph --save zod-refcluster-v2 --compare zod-callgraph
tsx experiments/run.ts hono --split-strategy call-graph --save hono-refcluster-v2 --compare hono-callgraph

# Minified fixtures (mangled names)
tsx experiments/run.ts zod-minified --split-strategy call-graph --save zod-minified-refcluster
tsx experiments/run.ts hono-minified --split-strategy call-graph --save hono-minified-refcluster
```

**What to look for:**

- ARI improvement over baseline (0.128 for zod, 0.029 for hono)
- File count: should be closer to original (8 for zod, 14 for hono) not 212/99
- Homogeneity vs completeness balance
- Minified vs unminified: does the signal degrade?

## Phase D: Cluster Count Sweep (Experiment 008)

The `estimateFileCount` heuristic is a guess (functions/40). Calibrate it.

```bash
tsx experiments/008-cluster-count-sweep/sweep.ts zod
tsx experiments/008-cluster-count-sweep/sweep.ts hono
tsx experiments/008-cluster-count-sweep/sweep.ts zod-minified
tsx experiments/008-cluster-count-sweep/sweep.ts hono-minified
```

**What to look for:**

- ARI peak: at what target count is ARI maximized?
- Does the peak correlate with the actual original file count?
- How sensitive is ARI to target count? (flat plateau = good, sharp peak = fragile)
- Calibration: what's the optimal functions-per-file ratio?

**Action:** If `estimateFileCount` is far off from the optimal, update the heuristic constants.

## Phase E: E2E Validation

Verify split output is syntactically valid JavaScript.

```bash
tsx experiments/validate-split.ts zod
tsx experiments/validate-split.ts hono
tsx experiments/validate-split.ts zod --split-strategy call-graph
tsx experiments/validate-split.ts hono --split-strategy call-graph
```

**What to look for:**

- All output files parse without syntax errors
- Barrel index.js is generated
- Import resolution works (if possible)

## Phase F: Tuning (if needed)

Based on Phase C/D results, adjust parameters:

1. **If ARI is low but signal exists (Phase B shows separation):**

   - Try different similarity thresholds
   - Adjust IDF filtering
   - Try different community detection approaches (e.g., threshold-based connected components instead of modularity)

2. **If file count is wrong:**

   - Update `estimateFileCount` constants based on sweep data
   - Or pass explicit `--target-count` matching ground truth to measure ceiling

3. **If minified is much worse than unminified:**
   - Bundler signals (`__export`, `__name`) may be critical
   - Check if they survive terser

## Success Criteria

| Fixture       | Baseline ARI | Target ARI | Stretch |
| ------------- | ------------ | ---------- | ------- |
| zod           | 0.128        | >= 0.40    | >= 0.60 |
| hono          | 0.029        | >= 0.30    | >= 0.50 |
| zod-minified  | N/A          | >= 0.25    | >= 0.40 |
| hono-minified | N/A          | >= 0.20    | >= 0.30 |

## Commands Summary (copy-paste ready)

```bash
# Phase A
tsx experiments/prepare-minified.ts zod zod-minified
tsx experiments/prepare-minified.ts hono hono-minified

# Phase B
tsx experiments/007-reference-analysis/analyze.ts zod
tsx experiments/007-reference-analysis/analyze.ts hono
tsx experiments/007-reference-analysis/analyze.ts zod-minified
tsx experiments/007-reference-analysis/analyze.ts hono-minified

# Phase C
tsx experiments/run.ts zod --split-strategy call-graph --save zod-refcluster --compare zod-callgraph
tsx experiments/run.ts hono --split-strategy call-graph --save hono-refcluster --compare hono-callgraph
tsx experiments/run.ts zod-minified --split-strategy call-graph --save zod-minified-refcluster
tsx experiments/run.ts hono-minified --split-strategy call-graph --save hono-minified-refcluster

# Phase D
tsx experiments/008-cluster-count-sweep/sweep.ts zod
tsx experiments/008-cluster-count-sweep/sweep.ts hono

# Phase E
tsx experiments/validate-split.ts zod --split-strategy call-graph
tsx experiments/validate-split.ts hono --split-strategy call-graph
```

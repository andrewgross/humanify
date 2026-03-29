# Experiment 009: App Bundle Tuning

## Date: 2026-03-29

## Context

Added `app-zod-hono` fixture — a Hono+Zod API server bundled with esbuild (6,621 lines, 557 top-level functions, 31 source files: 8 app + 23 library).

## Baselines

### A. esbuild-esm adapter (ceiling — uses comment markers)

| Metric | Value |
|--------|-------|
| ARI | **1.000** |
| V-Measure | 1.000 |
| Output files | 32 (vs 31 ground truth) |
| Tree Similarity | 0.980 |

Perfect split. The esbuild-esm adapter + normalizeModulePath is working correctly.

### B. Call-graph adapter (before fix)

| Metric | Value |
|--------|-------|
| ARI | 0.235 |
| V-Measure | 0.400 |
| Output files | 10 (vs 31 ground truth) |

Severely under-splitting. The reference clustering produced only 10 files regardless of target count.

## Bug Found: Disconnected Component Collapse

**Root cause**: `detectCommunities` uses agglomerative clustering that merges within connected components. When the similarity graph has many disconnected components (252 for this bundle), it can't merge across them. The loop stops early, leaving 252 communities. Then `buildClustersFromCommunities` treats all singleton communities as orphans, and `assignOrphans` dumps them into the few multi-member clusters. Result: everything collapses into ~10 files regardless of target.

**Fix**: Added `mergeCommunitiesByPosition()` — when agglomerative clustering leaves more communities than the target, merge the closest communities by position centroid until reaching the target. This bridges disconnected components using position proximity.

### C. Call-graph adapter (after fix)

| Metric | Before | After |
|--------|--------|-------|
| ARI | 0.235 | 0.218 |
| V-Measure | 0.400 | 0.433 |
| Output files | 10 | 18 |
| Homogeneity | 35.9% | 41.7% |
| Completeness | 45.2% | 45.1% |

ARI slightly decreased but V-Measure improved. File count is much more reasonable (18 vs 10). The lower ARI is expected — with more files, there are more opportunities for misassignment.

### D. Regression check (existing fixtures)

| Fixture | Metric | Before | After |
|---------|--------|--------|-------|
| zod | ARI | 0.128 | 0.128 (identical) |
| zod | V-Measure | 0.342 | 0.342 (identical) |
| hono | ARI | -0.006 | 0.029 (improved) |
| hono | V-Measure | 0.025 | 0.652 (improved) |

No regression. Hono significantly improved.

## Cluster Count Sweep (app-zod-hono, non-minified)

| Target | ARI | V-Measure | Homogeneity | Completeness | Actual Files |
|--------|-----|-----------|-------------|--------------|-------------|
| 2 | 0.179 | 0.252 | 14.9% | 82.7% | 2 |
| 4 | **0.275** | 0.360 | 27.3% | 52.8% | 4 |
| 10 | 0.220 | 0.412 | 37.6% | 45.6% | 10 |
| 15 | 0.219 | 0.433 | 41.1% | 45.7% | 15 |
| 20 | 0.222 | 0.455 | 44.4% | 46.7% | 20 |
| 30 | 0.164 | 0.446 | 46.9% | 42.5% | 29 |
| 50 | 0.156 | **0.456** | 50.8% | 41.4% | 46 |

Best ARI at target=4, best V-Measure at target=50. Reference signal is weak for this bundle.

## Cluster Count Sweep (app-zod-hono-minified)

| Target | ARI | V-Measure | Homogeneity | Completeness | Actual Files |
|--------|-----|-----------|-------------|--------------|-------------|
| 2 | **0.319** | 0.222 | 14.7% | 45.1% | 2 |
| 4 | 0.291 | 0.355 | 27.9% | 49.0% | 4 |
| 10 | 0.246 | 0.414 | 37.3% | 46.6% | 10 |
| 20 | 0.246 | 0.467 | 45.6% | 47.9% | 20 |
| 30 | 0.193 | 0.482 | 51.9% | 45.0% | 30 |
| 40 | 0.192 | **0.486** | 53.3% | 44.6% | 38 |

Minified results are surprisingly close to non-minified, confirming that positional references survive minification.

## estimateFileCount Calibration

| Fixture | Functions | Lines | Truth | Estimate | Error |
|---------|-----------|-------|-------|----------|-------|
| zod | 437 | 12,341 | 8 | 21 | +163% |
| hono | 195 | 4,236 | 14 | 8 | -43% |
| app-zod-hono | 557 | 6,621 | 31 | 18 | -42% |
| app-zod-hono (min) | 557 | 2 | 31 | 24 | -23% |

The heuristic is inconsistent because functions-per-file varies widely (14-55). No single formula can predict file count accurately. Changing the formula to improve one fixture would hurt another.

## Decision: KEEP the position-merge fix

The fix addresses a real bug where disconnected components caused the target count to be completely ignored. The improvement is clear:
- Target count is now respected across all values
- No regression on existing fixtures
- Significant improvement on hono (ARI -0.006 → 0.029)

## Next Steps

The reference signal (ARI ~0.2-0.3) is far from the esbuild-esm ceiling (ARI 1.0). Potential directions:
1. **Hybrid approach**: combine reference clustering with gap-based clustering signals
2. **Better similarity metric**: the current IDF-weighted Jaccard may not capture the structure well
3. **__export block signals**: the bundler boost signals exist but may not be weighted enough
4. **Larger graph radius**: consider 2-hop reference neighbors, not just direct references

# Experiment 009: App Bundle Tuning

## Date: 2026-03-29

## Context

Added `app-zod-hono` fixture — a Hono+Zod API server bundled with esbuild (6,621 lines, 557 top-level functions, 31 source files: 8 app + 23 library).

## Phase 1: Baselines

### A. esbuild-esm adapter (ceiling)

| Metric | Value |
|--------|-------|
| ARI | **1.000** |
| V-Measure | 1.000 |
| Output files | 32 |

Perfect split using esbuild comment markers.

### B. Call-graph adapter (before any changes)

| Metric | Value |
|--------|-------|
| ARI | 0.235 |
| V-Measure | 0.400 |
| Output files | 10 (vs 31 truth) |

## Phase 2: Bug Fix — Disconnected Component Collapse

**Root cause**: `detectCommunities` uses agglomerative clustering that can't merge across disconnected components. 252 singleton communities became orphans and collapsed into ~10 clusters.

**Fix**: Added `mergeCommunitiesByPosition()`.

**Result**: ARI 0.235 → 0.218, V-Measure 0.400 → 0.433, file count 10 → 18.

## Phase 3: Gap-First Architecture (major improvement)

### Discovery

Compared pure gap-based vs pure reference clustering across all fixtures:

| Fixture | Gap ARI | Ref ARI | Winner |
|---------|---------|---------|--------|
| zod | 0.378 | 0.196 | Gap (+93%) |
| hono | 0.494 | 0.501 | Ref (barely) |
| zod-minified | 0.699 | 0.222 | Gap (+215%) |
| hono-minified | 0.706 | 0.712 | Ref (barely) |
| app-zod-hono | 0.412 | 0.164 | Gap (+151%) |
| app-zod-hono-minified | 0.241 | 0.193 | Gap (+25%) |

**Gap-based clustering wins on 4/6 fixtures, often by large margins.** Bundlers preserve file order, making position gaps the strongest signal.

### Implementation

Rewrote `referenceCluster()`:
1. **Primary**: Gap-based splitting at position gaps
2. **Refinement**: Move boundary functions using reference similarity (2x threshold)
3. **Bundler boost**: Apply __export block signals

Removed ~200 lines of dead code (agglomerative clustering, community detection).

### Results — All Fixtures

| Fixture | Before ARI | After ARI | Delta |
|---------|-----------|----------|-------|
| app-zod-hono | 0.218 | **0.484** | +0.266 |
| app-zod-hono-minified | 0.197 | **0.276** | +0.078 |
| zod | 0.128 | **0.334** | +0.206 |
| hono | 0.029 | **0.578** | +0.549 |
| hono-minified | 0.000 | **0.526** | +0.526 |
| zod-minified | 0.324 | 0.212 | -0.112 |

5/6 fixtures improved significantly. zod-minified regressed because byte-offset gaps in minified code (2 lines, 422 functions) are unreliable — the previous reference-based approach had better signal there.

### Regression: zod-minified

The zod-minified regression is expected: with all code on 2 lines, byte-offset gaps are noise. The `estimateFileCount` heuristic gives 21 for this fixture (truth: 8), causing over-splitting. Future work: detect minified single-line bundles and fall back to reference-based clustering for those cases.

## Decision: KEEP

The gap-first approach provides massive improvements across most fixtures (+0.266 ARI on the primary target fixture). The zod-minified regression is a known limitation of gap-based splitting on heavily minified code, but the overall improvement justifies the change.

## Summary of Changes

1. `src/split/reference-cluster.ts`: Rewrote `referenceCluster()` to use gap-first + reference refinement
2. `src/split/reference-cluster.test.ts`: Updated test for new gap-based behavior
3. Removed ~200 lines of dead agglomerative clustering code

## Phase 4: estimateFileCount Tuning (reverted)

Tried changing minified heuristic from sqrt(N) to N^0.4:
- Fixed zod-minified: 0.212 → 0.569
- Broke hono-minified: 0.526 → 0.307

No single exponent works because function-per-file ratio varies 8-55x across bundles. Reverted to sqrt(N).

## Phase 5: Gap vs Truth Count Analysis

Tested gap-based at the exact ground truth file count:
- app-zod-hono at truth=31: ARI 0.412
- app-zod-hono at estimated=18: ARI **0.484** (better!)

Counter-intuitively, fewer files gives better ARI because gap-based naturally groups related library functions. The heuristic estimate of 18 produces better results than knowing the truth.

## Final State

| Fixture | Before (pre-experiment) | After | Change |
|---------|------------------------|-------|--------|
| app-zod-hono | 0.235 | **0.484** | +106% |
| zod | 0.128 | **0.334** | +161% |
| hono | 0.029 | **0.578** | +1893% |
| hono-minified | 0.000 | **0.526** | +∞ |
| app-zod-hono-minified | N/A | **0.276** | new fixture |
| zod-minified | 0.324 | 0.212 | -35% (known limitation) |

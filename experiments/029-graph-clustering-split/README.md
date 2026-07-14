# Experiment 029 — Graph-clustering split (order-respecting)

## Goal

Make the Bun-bundle split produce a folder/file tree shaped like a real
`src/` — many small files in a nested hierarchy — instead of the current
flat ~25 folders of ~2,800-line files, WITHOUT losing cross-version
stability. A Bun-specific split strategy (the plugin architecture allows a
different strategy per tool).

## Hypothesis

The current stable-split segments the wrapper-body statement sequence by
SIZE BUDGET (min 80 stmts / 4000 lines per file), so files are huge and
folders are arbitrary runs of 8. If instead we cut the sequence at the real
module SEAMS — valleys in the IDF-weighted cross-reference density — we get
files that match real-src sizes AND cohere, and the seam depths give a
folder hierarchy for free. Order stays a first-class prior (contiguous
files), so readability and stability survive.

## What "order-respecting" buys (vs order-blind graph partitioning)

Bun emits modules in-order, so statement position already approximates the
module structure. We use the graph to place cuts and group folders, but
never reshuffle the sequence — files stay contiguous. Order-blind
partitioning was rejected earlier (exp007/008: ARI ≤ 0.5, non-deterministic,
unstable to small graph changes). See the parent design discussion.

## Data

- Input (split): `claude-code-2.1.89/binary-decompiled/src/entrypoints/index.js` (12 MB, one Bun wrapper IIFE).
- Ground-truth shape: `claude-code-src-2.1.88/` (the real unbundled tree; adjacent version).
- Stability pair: 2.1.87 → 2.1.89 (consecutive decompiled inputs).

## Metrics (intrinsic — no source map exists for the decompiled bundle, so

ARI-vs-original is unavailable and, per exp023, the wrong target)

- **Size distribution** (lines/file) vs real src.
- **MQ** (Bunch Modularization Quality) — cohesion, granularity-dependent so
  always compared at matched file count.
- **Folder count / nesting depth / files-per-folder** vs real src.
- **Cyclic files** (files in a >1 SCC of the import graph) — runnable-emit risk.
- **Cross-version churn** — % statements that change file across versions (stability).

## Results so far (2.1.89, files = the split unit)

| variant                    | files | folders | depth | median lines | MQ        | cross-edges |
| -------------------------- | ----- | ------- | ----- | ------------ | --------- | ----------- |
| real src 2.1.88 (target)   | 1902  | 293     | 5     | 129          | —         | —           |
| baseline (shipped budgets) | 231   | 25      | 1     | 2875         | 0.334     | 70.8%       |
| baseline @ fine budgets    | 2325  | —       | 1     | 234          | 0.201     | 81.3%       |
| **seam-tiered (this exp)** | ~1940 | 291     | 2     | ~104         | **0.243** | **74.9%**   |

**The decisive test** (`mqsweep.ts`) — at matched cluster count K, cutting at
graph seams beats naive equal-spacing, and the advantage GROWS with
granularity: +11% at K=200, +22% at K=800, **+32% at K=1600**. Both beat
random-scatter at every K (the order prior). So the reference graph _does_
carry exploitable module structure at the fine granularity we want — the
earlier "clustering fails" verdict was about minified input + ARI, neither
of which applies here (we split humanified/beautified code, measure MQ).

## Design (implemented in `lib/`)

- `metrics.ts` — size stats, MQ, cross-edge ratio, cyclic-file count (Tarjan).
- `graph.ts` — `buildRefGraph`: reuses the splitter's `referenceIndices`,
  adds IDF (hub down-weight) + line spans.
- `cluster.ts` — `crossingCurve` (O(E) windowed IDF crossing density via
  difference array); `deepSeamCuts` (global-deepest-seam file segmentation +
  maxLines safety); `segmentFiles` (budget-greedy, the weaker baseline-style).
- `folderize.ts` — `tieredOrderFromCuts`: tier file-boundary seams by depth
  into a nested folder odometer (deepest seams = top folders). No root dump.
- `split.ts` — `seamTieredSplit` (production candidate) + variants.
- `hier.ts` — divisive recursive variant (kept for comparison; root-dumps).

## Runbook

```bash
# ground-truth shape
tsx experiments/029-graph-clustering-split/srcdist.ts 2.1.88
# baseline / clustered / seam-tiered (needs a big heap for the 12MB bundle)
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/measure.ts 2.1.89 baseline
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/measure.ts 2.1.89 seam
# the decisive seam-vs-naive MQ sweep
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/mqsweep.ts 2.1.89
# unit tests
tsx --test experiments/029-graph-clustering-split/lib/*.test.ts
```

Env overrides: `EXP029_TARGET` (file count), `EXP029_MAXLINES`, `EXP029_TIERS` ("40,250"), `EXP029_WINDOW`.

## Open / next

- **Stability (P5, the hard requirement)**: cluster ONCE on a baseline;
  inherit per-name via the ledger thereafter; new bindings placed by
  reference-affinity, not textual locality. Two-version churn test.
- Folder balance: tiering still yields one oversized top folder + many
  1-file folders (median 1 vs real 2).
- Productionize into `stable-split.ts` as `assignClustered` (P4): concat-
  equivalence is free; add a load-time cycle merge gate for `--split-runnable`.

# Exp029 — Order-respecting graph clustering makes the split look like real src/, and the existing stability mechanism already fits it

## Headline

Cutting the Bun wrapper-body statement sequence at graph **seams** (valleys
in the IDF-weighted cross-reference density) instead of on a **size grid**
transforms the split from 231 huge flat files into ~2,100 small nested files
that match the real `src/` distribution — and, because the cuts stay
order-respecting (contiguous), the **existing** name-inheritance +
textual-locality stability mechanism is already the correct one. The
planned reference-affinity placement upgrade was **measured and refuted**.

## The distribution win (2.1.89 input, 2.1.88 real src as target)

|                              | files    | folders | maxDepth | median lines | mean | files/folder (med) | MQ    | cross-edges |
| ---------------------------- | -------- | ------- | -------- | ------------ | ---- | ------------------ | ----- | ----------- |
| **real src 2.1.88** (target) | 1902     | 293     | 5        | **129**      | 270  | **2**              | —     | —           |
| baseline (shipped budgets)   | 231      | 25      | **1**    | **2875**     | 2774 | 8                  | 0.334 | 70.8%       |
| baseline @ fine budgets      | 2325     | ~200    | 1        | 234          | 276  | 11                 | 0.201 | 81.3%       |
| **seam + balanced folders**  | **2095** | **329** | **2**    | **100**      | 306  | **6.4 (mean)**     | 0.234 | 75.4%       |

Files 231→2,095 (real 1,902). Folders 25→329 (real 293). Median file
2,875→100 lines (real 129). Files/folder mean 9.2→6.4 (real 6.5), size-capped
so no single folder dumps (877 under global-depth tiering → 26 with balanced
foldering). Depth 1→2. The shape now matches real src on every axis we can
measure. (The one 10,549-line file is a single megastatement — unsplittable,
present in every variant, including baseline.)

## Library-aware (app-only) — the truer picture

The measurements above cluster the RAW wrapper body, which still contains
~1,523 vendored npm packages inlined as Bun CJS factories (`var X = d((exports,
module) => …)`) — including React (10,549 lines, the "megastatement"). In
production those are extracted to `vendor/`/`libraries/` before the app split.
`libraryAwareBalancedSplit` sets them aside (one untouched file each) and
clusters only the app statements:

|                           | files    | folders | median | max       | MQ        |
| ------------------------- | -------- | ------- | ------ | --------- | --------- |
| real src 2.1.88           | 1902     | 293     | 129    | 5594      | —         |
| clustered incl. libraries | 2095     | 329     | 100    | **10549** | 0.234     |
| **clustered, app-only**   | **1893** | **297** | **82** | **3104**  | **0.260** |

Setting libraries aside removes the megastatement (max 10,549→3,104, a real
app function), lifts MQ (0.234→0.260 — the libraries were graph noise), and
lands **1,893 files / 297 folders vs real 1,902 / 293** — nearly identical.
Library detection is structural (`detectCjsHelper` — dominant callee of a
`var X = d((≥1-param)=>…)`; the shipped `identifyBunCjsFactory` keys on the
minified `{exports:{}}` literal and misses beautified code). Library files
keep their minified binding name (`libraries/wcq.js`) — proper library naming
is out of scope for now. (macOS's case-insensitive FS collapses some
case-only-distinct library names on disk; the in-memory count of 1,523 is
correct — real emission needs case-disambiguation.)

## The decisive test: seams beat naive, and the advantage grows with granularity

`mqsweep.ts` — same cluster count K, cut at graph seams vs naive
equal-spacing vs random-scatter:

```
     K    seam-MQ   equal-MQ  scatter-MQ   seam-advantage
    25   0.4012    0.4014    0.3793        -0.0%
   200   0.3719    0.3344    0.3064       +11.2%
   800   0.3274    0.2673    0.2284       +22.5%
  1600   0.2808    0.2127    0.1751       +32.0%
```

- At coarse K any cut works; at the fine K we want (~1,600–2,100 files),
  seam-cutting wins by **22–32% MQ**. The reference graph _does_ carry
  exploitable module structure at fine granularity.
- Both crush random-scatter at every K — the **order prior** alone is worth
  a lot (equal-spacing ≫ scatter).
- This is why the earlier "clustering fails" verdict (exp007/008, ARI≈0)
  doesn't apply: that was minified input scored by ARI-vs-original; we split
  humanified/beautified code and score intrinsic MQ.

Budget-greedy cutting could NOT capture this (baseline-fine 0.201 ≈ pure
greedy) because the budget grid pins cuts off the seams. Global-deepest-seam
selection (`deepSeamCuts`) is the unlock.

## Stability (P5): the existing mechanism is correct; affinity is refuted

Hold-out experiment (`stability.ts`) — drop a fraction of statements from
the ledger (pretend they're new code), place them back, score against their
true cluster:

```
  pct   placed   locality-file  affinity-file   locality-folder  affinity-folder
   10%    2457      89.0%          64.9%            98.2%            83.4%
   20%    4650      87.3%          59.4%            97.9%            79.4%
   40%    9013      84.8%          50.6%            97.6%            73.2%
```

**Textual locality wins decisively.** Because the clustering is
order-respecting (contiguous), a new statement's true file is the segment it
sits in — exactly its textual neighbor. Reference-affinity pulls it toward
scattered refs: marginally higher MQ (0.2339 vs 0.2307 after 20% churn) but
much worse _placement_. That MQ-vs-placement split is the order-blind /
order-respecting tension in miniature — and we chose order-respecting.

**Conclusion:** the clustered split inherits stable-split's proven
`assignWithPrior` (name votes + textual-locality residue) UNCHANGED. Cluster
once on a baseline; every release after inherits. New code lands in the
right folder ~98% of the time. The planned affinity upgrade is dropped.

## Honest caveats

- **MQ is granularity-confounded** — always compare at matched file count.
  At ~2,100 files seam-tiered 0.234 vs baseline-fine 0.201 = +16%; the real
  src would likely score similarly low at its granularity.
- **Folder balance** (fixed): global-depth tiering left one 877-file folder;
  balanced foldering (`balancedTierOrder` — deepest seam within a size window,
  recursively) caps folders and lands 329 folders / mean 6.4 files (real 293 /
  6.5). Still slightly more uniform than real (real has a long tail up to 298;
  ours maxes ~26) — real src's skew (a few huge folders like `components/`)
  isn't reproduced, but the count and mean match.
- **Cyclic files 1,979** (files in a >1 SCC) — the import graph is highly
  cyclic at fine granularity; the `--split-runnable` CJS emit already defers
  most reads, but a load-time-cycle merge gate is required before wiring
  runnable emit (P4).
- **Minified-name floor**: self-stability shows 303/21,831 (1.4%) abstaining
  even with the full ledger, from Bun's redeclared minified names. This is
  the SAME ambiguity the shipped splitter has; it vanishes on humanified
  (unique) names in production. The experiment runs on beautified-but-not-
  renamed code (no LLM), which is why we tested placement by hold-out rather
  than a real cross-version name diff.

## What was built (`experiments/029-graph-clustering-split/`)

- `lib/metrics.ts` — size stats, MQ, cross-edge ratio, Tarjan cyclic-file count.
- `lib/graph.ts` — IDF-weighted reference graph over wrapper statements.
- `lib/cluster.ts` — `crossingCurve` (O(E)), `deepSeamCuts`, `segmentFiles`.
- `lib/folderize.ts` — `tieredOrderFromCuts` (seam-depth folder odometer).
- `lib/split.ts` — `seamTieredSplit` (the candidate) + variants.
- `lib/stability.ts` — ledger inherit + textual-locality / (refuted) affinity.
- `measure.ts`, `srcdist.ts`, `mqsweep.ts`, `stability.ts` — harnesses.
- 24 unit tests.

## Next

1. **Productionized** (commit 562cdaf) — `src/split/cluster-assign.ts` is the
   SOLE `--split` fresh-grouping path; the budget grid is deleted. Library
   extraction + case-safe names + LLM naming folded in. `assignWithPrior`
   untouched. STILL TODO: load-time cycle merge gate for `--split-runnable`.
2. Folder-balance tuning (cap oversized folders; lift median off 1).
3. Naming — now that folders are coherent contiguous modules, the LLM namer
   has a real theme to name (the original motivation).

## P4 productionization — runnable default + src/vendor/.humanify layout (2026-07-15)

Landed on this branch (docs/plan-2026-07-15-runnable-default-src-layout.md):

- `--split` now emits the RUNNABLE CommonJS tree by default (`--split-pure`
  keeps the byte-exact review tree; `--split-runnable` is gone). The runnable
  emit still falls back to the pure tree loudly on a decline.
- Layout re-baseline: app code under `src/<top>/<sub>/<file>.js`, vendored
  libraries under `vendor/`, generated metadata under `.humanify/`
  (`split-ledger.json`, `_bundle.js`, `__bun-runtime.js`) — folder names
  single-sourced in `src/split/layout.ts`. Root = `index.js`, `run.cjs`,
  `package.json`, `RUNNABLE.md` only. The Bun unpack adapter's extracted
  factories (minified npm bundles) also land in `vendor/`, manifest beside
  them; the unpack step's on-disk source copy (`runtime.js` / passthrough
  `index.js`) is deleted once the split supersedes it, so the pure root is
  exactly `src/` + `vendor/` + `.humanify/`.
- Measured on 2.1.89 (verify-prod/verify-runnable/layout): concat-equivalence
  PASSED; 1,893 app files under src/ + 1,523 libraries under vendor/, 0 stray,
  0 case collisions; runnable emit load-time ACYCLIC (3,418 files, ~1.9s);
  entry requires `./src/…`; `_bundle.js` at `.humanify/_bundle.js`.
- Bug found by the harness: `computeRelativeImportPath` skipped the `./`
  prefix for dot-folder targets (`.humanify/_bundle.js` became a bare package
  specifier). Fixed with a `../`-only relative check + regression test.
- Full E2E on 2.1.89 (local gpt-oss-20b, ~35 min): `--split` produced 1,926
  LLM-named src/ files + 1,523 vendor/ + clean root; `npm install && node
run.cjs --version` printed `2.1.89 (Claude Code)` (exit 0) and a
  require-all check loaded 1,926/1,926 src modules. The unpack-extraction
  path (minified input → factories under vendor/ + manifest + re-link) was
  validated with a synthetic minified mini-bundle end to end, both modes —
  the relinked factory graph computes correct values through require
  boundaries.
- The load-time cycle merge gate stays deferred (2.1.89 is acyclic; cyclic
  inputs fall back to the pure tree gracefully).

## Reproduce

```bash
tsx experiments/029-graph-clustering-split/srcdist.ts 2.1.88
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/measure.ts 2.1.89 seam
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/mqsweep.ts 2.1.89
NODE_OPTIONS=--max-old-space-size=8192 tsx experiments/029-graph-clustering-split/stability.ts 2.1.89
tsx --test experiments/029-graph-clustering-split/lib/*.test.ts
```

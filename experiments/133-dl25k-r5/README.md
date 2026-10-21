# exp133: the download top-25k rebuilt with the current builder (k2-all-r5-origin-dl25k)

## STATUS (2026-10-21)

DONE on branch `exp133-dl25k-r5` (off main `f28cf5c3`; main `4b1d1a49`, factory_bodies on by default, merged in).
Data, scripts and labels only. No crates change is committed. **Bar NOT met: the new index is not recommended as
the default.** `k2-all-r5-origin` stays.

- **Build.** `/work/exp133-data/full/ix/k2-all-r5-origin-dl25k` (13 GB, index id `267ee4a5…`). It is exp118's
  package set (k2-all's pins plus the 13,785 new download-top-25k roots at `year-last3+major1`) built with main's
  builder at `f28cf5c3`: `unit-rows/5+origin` (`--sponge-regions origin`, exp124/exp130), all five slices, and
  no sourcesContent (exp131).
  - It was built as an increment. Every k2-all shard was reused from exp130's store, and exp118's literals shards
    were reused for the new versions.
  - 33,859 versions were built: the 28,196 new ones plus k2-all's known failures, retried.
  - 195 chunks, all exit 0. Shards took 10.9 h on 30 cores, and the merge took 32 min at a 62.8 GB peak.
- **Score** (main's plain defaults, scorer 13, structural, both scopes; base = `*exp133-base*` = k2-all-r5-origin,
  byte-identical to exp132's `*exp132-ftfw*`):
  - F0.5 is up on held-out (+0.42), pop (+0.02 / +0.23), all four test sets (bun +0.44, esbuild +0.30, rollup
    +0.73, webpack +1.00), test2-rollup (+0.11), test2-webpack (+0.88), the apps (+0.32 / +0.79 / +0.46) and
    wrangler.
  - F0.5 is DOWN on test2-bun (-0.10), test2-esbuild (-0.21), remin (-0.75 / -3.32), the dev esbuild build of
    task-master (`esbuild-app`, -1.27) and dev-rollup/webpack (-0.07 / -0.11).
  - **Precision is below k2-all-r5-origin's on 15 of 21 sets**: held-out 99.41 -> 98.99, test2-bun 99.15 -> 98.53,
    test2-esbuild 99.63 -> 99.03, pop-esbuild 99.30 -> 98.99, esbuild-app 99.07 -> 97.49, remin-terser
    99.90 -> 99.66.
  - App claimed is 0 everywhere.
- **Against the old dl25k** (exp118's index with today's defaults, `*exp133-old*`), the rebuild helps the
  population sets: pop +1.12 / +0.85, test +0.56 / +0.44 / +0.96 / +0.15, test2 rollup/webpack +0.48 / +0.46. It
  hurts test2-bun/esbuild (-0.08 / -0.34) and esbuild-app (-1.55). The builder fixes the sponge regions it can
  see. The remaining loss is a different mechanism.
- **Why precision falls (dev diagnosis, section 3).** The new packages are mostly CARRIERS: their dist inlines an
  already-indexed package's code (@mastra/mcp carrying ajv, @mermaid-js/layout-elk carrying mermaid, unpdf
  carrying pdfjs-dist, @architect/utils carrying chalk). They take units today's index names right, in three
  ways:
  - the carrier holds MORE of the unit's functions than the origin's own rows ("outscored": held-out 295 KB,
    esbuild-app 293 KB, the pop sets' @mastra/mcp lazy bodies);
  - exact ties, where the scoped carrier wins on name order (`@` sorts first; apps 290 KB);
  - factory modules whose truth family the index does not hold for that unit at all (held-out 483 KB).
- **Fix tried, not shipped:** `[library.lookup] tie_by_context` (lab only, `tie-by-context.patch`, test included).
  A structural tie with no previous claim goes to the tied package holding the prints in the smallest index
  module.
  - Sorted by context: apps P +0.12 / +0.38 / +0.12, but cc recall -4.4, dev-webpack P -0.45 and remin-terser
    P -0.21.
  - With the specificity bound (4x + 4096) it is inert except apps-rollup +0.15.
  - It reaches only the tie class, so it is not worth a crates change.
- **Next lever:** index-time origin ownership for carriers. A carrier's rows for code an indexed origin also holds
  in a smaller module of its own should be the origin's (exp110's idea, now with a population that is mostly
  carriers). Or a per-package carrier verdict at build time, from the package's dist against its declared
  dependencies' rows.

## 1. Build

`scripts/assemble-store.sh` builds the store and registry, hard-linked: exp130's store first, then exp118's for
what it lacks (`cp -n`). exp118's module shards carry `unit-rows/4` keys, so they are rebuilt; its literals
shards match (their key has no unit-rows version).

`scripts/build-dl25k-r5.sh` runs exp108's `build_chunks.py` with exp118's pins, `--sponge-regions origin`,
`flock /work/index-build.lock` and cpus 34-63. The dry run planned 33,761 versions to build and 397,861 shards to
reuse. exp118's work dirs had been pruned, so every new version is installed and built in this one work dir:
exp124's lesson holds by construction.

**Disk.** The builds are already slim: 170 of 170 sampled maps carry no sourcesContent. A new version's ~44 MB is
nearly all `node_modules` (builds are ~200 KB). The work dir grew about 150 GB/h. `scripts/prune_work_guard.py`
handles this:

- it deletes each finished chunk's work dirs, like exp118's `prune_work.py`;
- it pauses the build with SIGSTOP below 300 GB free, and resumes it above 350 GB.

It never paused. /work stayed at 1.1-1.5 TB free. A consequence: these installs are gone, and a later
re-fingerprint of the dl25k versions must reinstall them.

## 2. Results

Labels: `*exp133-base*` (k2-all-r5-origin), `*exp133-old*` (exp118's k2-all-dl25k) and `*exp133-dl*` (this
index). All use main `4b1d1a49`'s plain defaults, or `f28cf5c3` with the four factory keys passed explicitly for
`*exp133-old*` (identical crates behaviour). Scorer 13, structural rule, both scopes. test / test2 are summaries
only, and test2 was never diagnosed. Values are P / R / F0.5 in %. Bar = F0.5 up AND P at or above base, app 0.

| set | k2-all-r5-origin | k2-all-r5-origin-dl25k | dP | dF0.5 | wrong KB | bar |
|---|--:|--:|--:|--:|--:|---|
| cc | 100.00 / 81.90 / 95.77 | 100.00 / 82.46 / 95.92 | +0.00 | +0.15 | 0.0 -> 0.0 | PASS |
| esbuild-app | 99.07 / 74.74 / 93.01 | 97.49 / 74.24 / 91.75 | -1.57 | -1.27 | 131.1 -> 354.7 | fail |
| heldout | 99.41 / 67.71 / 90.90 | 98.99 / 69.70 / 91.32 | -0.42 | +0.42 | 938.2 -> 1664.4 | fail |
| wrangler | 99.07 / 33.14 / 70.87 | 99.88 / 33.26 / 71.31 | +0.81 | +0.44 | 5.8 -> 0.7 | PASS |
| apps-esbuild | 99.89 / 77.65 / 94.48 | 99.76 / 79.06 / 94.80 | -0.12 | +0.32 | 135.0 -> 285.9 | fail |
| apps-rollup | 99.09 / 79.31 / 94.38 | 98.97 / 82.51 / 95.17 | -0.12 | +0.79 | 710.6 -> 838.2 | fail |
| apps-webpack | 99.58 / 73.50 / 92.98 | 99.60 / 74.91 / 93.44 | +0.02 | +0.46 | 230.8 -> 222.5 | PASS |
| dev-rollup | 99.87 / 77.53 / 94.43 | 99.87 / 77.29 / 94.36 | -0.00 | -0.07 | 2.1 -> 2.1 | fail |
| dev-webpack | 99.64 / 62.89 / 89.22 | 99.64 / 62.63 / 89.11 | -0.00 | -0.11 | 4.1 -> 4.1 | fail |
| pop-bun | 99.44 / 76.61 / 93.85 | 99.31 / 76.99 / 93.87 | -0.13 | +0.02 | 1400.8 -> 1750.0 | fail |
| pop-esbuild | 99.30 / 78.71 / 94.37 | 98.99 / 80.33 / 94.60 | -0.31 | +0.23 | 1799.4 -> 2662.2 | fail |
| remin-esbuild | 99.69 / 51.76 / 84.11 | 99.70 / 50.36 / 83.37 | +0.01 | -0.75 | 160.0 -> 148.5 | fail |
| remin-terser | 99.90 / 20.33 / 56.04 | 99.66 / 18.28 / 52.72 | -0.25 | -3.32 | 18.6 -> 59.0 | fail |
| test-bun | 99.51 / 68.99 / 91.42 | 99.58 / 70.11 / 91.86 | +0.07 | +0.44 | 1029.7 -> 888.1 | PASS |
| test-esbuild | 99.26 / 73.37 / 92.72 | 99.24 / 74.38 / 93.02 | -0.02 | +0.30 | 1740.0 -> 1821.1 | fail |
| test-rollup | 98.96 / 72.43 / 92.21 | 98.89 / 74.89 / 92.93 | -0.07 | +0.73 | 829.8 -> 919.1 | fail |
| test-webpack | 99.48 / 72.19 / 92.49 | 99.40 / 75.50 / 93.48 | -0.07 | +1.00 | 568.4 -> 677.6 | fail |
| test2-bun | 99.15 / 68.12 / 90.87 | 98.53 / 69.02 / 90.77 | -0.61 | -0.10 | 1445.6 -> 2530.3 | fail |
| test2-esbuild | 99.63 / 71.04 / 92.21 | 99.03 / 71.63 / 92.00 | -0.59 | -0.21 | 723.1 -> 1894.5 | fail |
| test2-rollup | 96.35 / 75.65 / 91.35 | 96.29 / 76.18 / 91.46 | -0.06 | +0.11 | 1693.7 -> 1733.5 | fail |
| test2-webpack | 99.27 / 70.09 / 91.64 | 99.59 / 72.06 / 92.52 | +0.31 | +0.88 | 874.1 -> 509.9 | PASS |

**The ship bar, read honestly.**

- F0.5 is up on held-out, pop and test, but DOWN on test2-bun and test2-esbuild.
- Precision is at or above base on only 6 of 21 sets.
- Held-out recall rises by only 2.0 points (67.71 -> 69.70). Most of the not-indexed residual is still missed even
  with its packages indexed: indexing a package by name is not matching its bytes (exp118's finding, unchanged
  with factory_bodies on).

## 3. Why precision falls (dev material, by mechanism)

Tools: `scripts/judge13.py`, `scripts/wrongdiff.py` and `scripts/tieclass.py`. Outputs stay in
`/work/exp133-data/judge/` because they name packages.

- `judge13.py` gives per-claim verdicts exactly as scorer 13 judges them, over both scopes. exp125's `judge.py`
  is scorer 9, statement units only. It reproduces every card's `wrongFamilyBytes` and `appClaimedBytes` to the
  byte on dev.
- `wrongdiff.py` breaks the new wrong bytes down by scope, rule, base's verdict and whether the claimer is a newly
  indexed package.
- `tieclass.py` reads mmlib's structural emulation to say whether the truth's family was tied, outscored or not
  held at all.

New wrong bytes are bytes dl25k claims wrong and base does not. On held-out, esbuild-app and the apps sets, ≥ 95%
are claimed by packages base never claimed (newly indexed). On pop-esbuild it is ~60%: at least 436 KB are already-indexed
packages whose seeds and factory propagation newly reach modules base left unclaimed. Almost all are index seeds,
and the rest is propagation off them:

| set | new wrong | dominant class |
|---|--:|---|
| held-out | 913 KB | factory-scope seeds on modules base left unclaimed, truth family not held for the unit (483 KB); statement seeds outscoring the truth's origin, which base named (295 KB) |
| esbuild-app | 324 KB | one carrier outscoring ajv on two 146 KB lazy-init bodies (293 KB): the carrier's rows hold an 865-node function the origin's rows do not |
| apps-rollup / -esbuild / -webpack | 393 / 195 / 58 KB | exact ties won by the scoped carrier on name order (290 KB), and propagation off them |
| pop-esbuild / pop-bun | 1,127 / 624 KB | the same carriers (pdfjs-dist, mermaid and ajv carriers) by outscoring, plus 326 KB of factory propagation |

Named dev claimers, held-out excepted: @mastra/mcp (ajv, and the MCP SDK's copy of ajv), @mermaid-js/layout-elk
(mermaid), unpdf / pdf-parse (pdfjs-dist), @newrelic/rrweb (@rrweb/replay), @googleapis/sqladmin (googleapis),
@architect/utils (chalk, color-convert, signal-exit) and @sendbird/chat (whatwg-fetch). Every one is a package
whose own dist inlines its dependencies. The download rank adds application-level SDKs, which are exactly that.

Existing guards and why they miss:

- **copy_presence (exp121)** is per PACKAGE. It acts only when EVERY seed of a package is an inlined copy, and
  these carriers also have seeds of their own.
- **shadow_ties (exp114)** needs every seed to be a tie.
- **context_presence** needs the bundle to show the code in a smaller module than the carrier's. A lazy body or
  cut factory module of the origin is often as big as the carrier's module.

## 4. Lab: `tie_by_context` (not committed to crates; `tie-by-context.patch`)

The key is `[library.lookup] tie_by_context`, default off, with a lookup test. A structural tie that no previous
claim settles goes to the tied package holding the unit's prints in the smallest index module. Name order is
used only among equal contexts. The origin is the smaller context. On the dl25k index (lab labels
`*lab133-tc*`, `*lab133-tc4*`, dev corpora only), deltas against `*exp133-dl*` are P / F0.5:

| set | sort by context | context only when decisive (> 4x + 4096) |
|---|--:|--:|
| cc | +0.00 / -1.24 (R 82.46 -> 78.06) | 0 / 0 |
| apps-esbuild | +0.12 / +0.14 | -0.01 / -0.01 |
| apps-rollup | +0.38 / +0.41 | +0.15 / +0.15 |
| apps-webpack | +0.12 / +0.17 | -0.03 / -0.02 |
| dev-webpack | -0.45 / -0.40 | 0 / 0 |
| remin-terser | -0.21 / +0.04 | +0.02 / +0.01 |
| held-out, pop, esbuild-app | <= ±0.10 | <= ±0.02 |

Sorting by context fixes the apps' carrier ties but costs Claude Code recall. Switching the package per unit
breaks the previous-claim runs that support reads. It also costs dev-webpack and remin-terser precision. The
decisive variant is inert. Neither reaches the outscored or not-held classes, which hold most of the new wrong
bytes. So no crates change, and no neutrality run.

## Reproduce

```bash
S=experiments/133-dl25k-r5/scripts; D=/work/exp133-data/full
$S/assemble-store.sh $D
$S/build-dl25k-r5.sh $D /work/exp133-data/bin/humanify-index     # humanify-index at f28cf5c3 (main's builder)
python3 -I $S/prune_work_guard.py $D k2-all-r5-origin-dl25k $D/build.out <build_chunks pid> 300
$S/score-all.sh exp133-base /work/exp130-data/full/ix/k2-all-r5-origin
$S/score-all.sh exp133-dl $D/ix/k2-all-r5-origin-dl25k
python3 -I $S/table.py exp133-base exp133-dl
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I $S/judge13.py dev /work/exp133-data/judge/dev-base-dl.jsonl exp133-base exp133-dl   # apps / pop / remin likewise
python3 -I $S/wrongdiff.py /work/exp133-data/judge/dev-base-dl.jsonl
$PY -I $S/tieclass.py dev /work/exp133-data/judge/dev-base-dl.jsonl exp133-dl
# lab (patch applied in a scratch worktree, binary built there): $S/lab-all.sh <label> <binary> <index> --pipeline-set library.lookup.tie_by_context=true
```

# exp122: why class-d library units miss ("hash differs"), and would a normaliser pay? (lab-first census)

## STATUS (2026-10-16)

DONE on branch `exp122-normaliser` (off main `7daf1f0b`). Lab scripts, a census table and this README only; **no
pipeline code, no index built, nothing to merge into the pipeline.** The lab needed a hook in
`libindex::units::unit_prints`. It is kept as `proto/lab-hook.patch` and is NOT applied on the branch.

- **No candidate normalisation reaches the bar.** The bar was +1 point of held-out library bytes, or +2 points on
  pop or apps. Ten generic normalisations were simulated, alone and all together, exactly on both sides. The
  combined ceiling is:

  - held-out +0.29;
  - pop-bun +0.40, pop-esbuild +0.11;
  - apps-rollup +0.24, apps-webpack +0.07, apps-esbuild +0.02;
  - task-master +0.12.

  These are ceilings: they count a recovered unit whether or not the rule's guards would then claim it. So step 3
  (a new `LIBRARY_UNITS` version and a k2-all re-fingerprint) was **not run**, and neutrality is not needed.

- **The one set where a normaliser pays is wrangler**, a published bundle that was re-minified. There `const`/`let`
  -> `var` alone recovers 15.1 of wrangler's 32.3 d-points. This is exp095's finding, re-confirmed on k2-all.
  Wrangler is 0.73 MB of library code. Every corpus built with the index's own recipes (held-out, pop, apps, test,
  test2) gets 0.00 from it, so it is no lever for the headline sets. If real-world re-minified bundles become a
  scored population, `K` is the first form to try.
- **Why d-units differ** (points of library bytes; table below):
  - **The code is not in the index's own build of that package@version** (`not-emitted`). This is the bulk on
    every set built with the recipes: pop-bun 2.68 of 4.09, pop-esbuild 2.42 of 2.88, apps-esbuild 2.10 of 2.39,
    apps-rollup 2.14 of 3.93, task-master 1.75 of 3.97. The app reached a file, export condition or CJS/ESM
    variant that the index's entry build never emitted. This is a BUILD-coverage problem (more entries/conditions
    per version), not a hashing one.
  - **Near-identical code that differs in how cross-module bindings are spelled** (`near:*`, ratio >= 0.8, not
    fixable by any normalisation tried):
    - **lazy-init hoisting.** `a = x, b = y` inside a lazy ESM init vs `var a = x, b = y` in the eager build:
      held-out 1.57, pop-bun 0.25. Held-out's figure is 19 units in 10 packages, 88% of it in ONE package, and
      one 372 KB data unit is 1.2 points by itself. The `L` rewrite makes the statement shape equal, but those
      units still differ elsewhere.
    - **interop member access.** `import_x.foo` / webpack's mangled `n.A` vs a direct binding, and `(0, f)()`:
      apps-webpack 1.42, apps-rollup 0.54, held-out 0.24. Most of these units also differ in other ways, so even
      dropping every member access on an outside binding (`O`, a big specificity loss) recovers only 0.06 to 0.21.
  - **No build on disk** (`no-build`; a measurement gap of the lab): task-master 1.80, pop 0.24 to 0.43, held-out
    0.39.

## 1. Method (aggregates only; held-out is dev; test and test2 not touched)

Labels: `exp119-k2all-st`, `-apps` and `pop-exp119-k2all-st`. These are main's shipped defaults on k2-all (scorer
9). They are the labels whose `missShares` this census decomposes. CC is out: it has sample truth and no file, so
its misses are d-nofile.

1. **`scripts/capture.py`** re-runs exp096's scorer unchanged, with `decompose.classify` wrapped. Every library unit
   keeps its class, truth package, version, file and span, in a PRIVATE file outside the repo
   (`/work/exp122-data`). The d-shares it prints equal the labels' summaries: held-out 3.43, task-master 3.97,
   wrangler 32.27, pop-bun 4.09, pop-esbuild 2.88, apps-esbuild 2.39, apps-rollup 3.93, apps-webpack 2.10.
2. **`scripts/shard_owners.py`** reads k2-all's shards and records, per slice, which build roots hold each
   owner@version. **`scripts/plan.py`** then picks, for each d-unit, the package's own build first plus up to 3
   other roots holding that package@version in the bundle's slice. There are 1,639 build dirs, found under the
   recipe/4 work dirs of exp113, exp109, exp108, exp101, exp105, exp111 and exp118. They hold the builds of 93% of
   the d-bytes.
3. **`proto/`** (`exp122-lab`, a detached crate with `lab-hook.patch` applied) prints units through the REAL unit
   model on both sides:

   - the query side runs `humanify discover --unit-prints` with no index;
   - the index side runs the builder's own `unit_rows`.

   Every unit's ESTree JSON is rewritten by a candidate normalisation first. With no rewrite (variant `0`) it
   reproduces the shipped prints exactly:

   - 1,478 / 1,478 query whole and function prints on a held-out bundle;
   - 5,342 / 5,342 shard rows of an index build.

4. **Simulation** (`scripts/analyze.py`). A d-unit is RECOVERED under a variant when the right family's rows in
   those builds cover at least half its nodes. This is the structural rule's cover test, exp095's `mmlib`
   emulation. Variant `0` recovers 0.00 everywhere, which is the sanity check: these are d-units.
5. **Diff** (`scripts/diff.py`). Each d-unit's counterpart is chosen from the units of that package@version in the
   index build: same file first, then the best content-token Jaccard. The two canonical streams are token-diffed
   with difflib, capped at 6,000 tokens. **`scripts/census.py`** gives every d-byte one cause and writes
   `census.json`.

## 2. The candidates, simulated exactly (recovered points of library bytes)

| variant (rewrite of the unit's ESTree)                                              | held-out | task-master |  wrangler |  pop-bun | pop-esbuild | apps-esbuild | apps-rollup | apps-webpack |
| ----------------------------------------------------------------------------------- | -------: | ----------: | --------: | -------: | ----------: | -----------: | ----------: | -----------: |
| d (the ceiling)                                                                     |     3.43 |        3.97 |     32.27 |     4.09 |        2.88 |         2.39 |        3.93 |         2.10 |
| K const / let / var -> var                                                          |     0.00 |        0.00 |     15.06 |     0.00 |        0.00 |         0.00 |        0.03 |         0.00 |
| T template text raw -> cooked                                                       |     0.00 |        0.00 |      0.00 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| C template == concatenation                                                         |     0.00 |        0.00 |      0.00 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| P `a["b"]` -> `a.b`, quoted keys                                                    |     0.00 |        0.00 |      0.00 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| M `!0`/`!1`/`void 0`/`===`                                                          |     0.00 |        0.00 |      0.00 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| D declaration == expression (function, class)                                       |     0.00 |        0.00 |      0.00 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| A every arrow as a function (ES-target downlevel)                                   |     0.00 |        0.00 |      0.12 |     0.00 |        0.00 |         0.00 |        0.00 |         0.00 |
| L lazy-init hoisting `a=x,b=y` == `var a=x,b=y`                                     |     0.01 |        0.00 |     15.06 |     0.20 |        0.00 |         0.00 |        0.03 |         0.00 |
| I interop `(0, f)` -> f, `x.default` -> x                                           |     0.11 |        0.08 |      0.00 |     0.12 |        0.09 |         0.02 |        0.00 |         0.00 |
| O any static member of an outside binding -> the binding (broad; costs specificity) |     0.28 |        0.12 |      0.12 |     0.20 |        0.11 |         0.02 |        0.21 |         0.06 |
| **all ten together**                                                                | **0.29** |    **0.12** | **15.16** | **0.40** |    **0.11** |     **0.02** |    **0.24** |     **0.07** |

L includes K's keyword rule, hence wrangler's identical number. Collision signal: of the bytes recovered with all
ten, the recovered prints are ALSO held by another family in the same builds for 115 B on held-out and 7.9 KB on
apps-rollup, and 0 elsewhere. No full-index collision count was run, since nothing passed the recall bar first.

**Why exp095 measured more:** its +6.3 on Claude Code was a `--target bun` build drift (zod escapes), and its +15.4
was wrangler. On held-out it was ~0, as here. Every corpus scored since exp100 is built with the index's own
bundler versions and recipes. So the spelling families a normaliser blurs (keyword, template raw, quotes, minifier
literals, declaration form) never differ on them.

## 3. The census: one cause per class-d byte (points of the set's library bytes; `census.json`)

| set          |     d | not-emitted | near: lazy-init | near: interop | near: other / inlined | partial (0.5-0.8) | recovered by any variant (union) | no build on disk |
| ------------ | ----: | ----------: | --------------: | ------------: | --------------------: | ----------------: | -------------------------------: | ---------------: |
| held-out     |  3.43 |        0.72 |            1.57 |          0.24 |                  0.02 |              0.18 |                             0.30 |             0.39 |
| task-master  |  3.97 |        1.75 |            0.00 |          0.16 |                  0.11 |              0.02 |                             0.12 |             1.80 |
| wrangler     | 32.27 |        5.69 |            5.85 |          0.87 |                  1.42 |              2.49 |                            15.96 |                0 |
| pop-bun      |  4.09 |        2.68 |            0.25 |          0.13 |                  0.04 |              0.15 |                             0.40 |             0.43 |
| pop-esbuild  |  2.88 |        2.42 |            0.00 |          0.06 |                  0.02 |              0.02 |                             0.11 |             0.24 |
| apps-esbuild |  2.39 |        2.10 |            0.00 |          0.07 |                  0.02 |              0.05 |                             0.03 |             0.12 |
| apps-rollup  |  3.93 |        2.14 |            0.15 |          0.54 |                  0.54 |              0.25 |                             0.25 |             0.07 |
| apps-webpack |  2.10 |        0.17 |            0.04 |          1.42 |                  0.21 |              0.12 |                             0.06 |             0.09 |

The diff causes are spread across families:

- **Held-out lazy-init:** 19 units in 10 packages; one package is 88% of the bytes.
- **apps-webpack interop:** 1,681 units in 50 packages; the top package is 46%.
- **pop-bun lazy-init:** 468 units in 32 packages.

**Mapping to the brief's list:**

- **Another minifier's spelling** (terser, esbuild, bun; `M`, `P`, `T`, `C`, `A`): 0 on every recipe-built set.
  The corpora and the index use the same minifier.
- **const/let/var** (`K`): wrangler only.
- **Template vs concatenation** (`C`): 0.
- **Property-access spelling** (`P`): 0.
- **Helper inlining / interop** (`I`, `O`, near: interop): the second-largest diff cause on webpack and rollup, but
  it comes with other differences, so a normaliser recovers <= 0.21.
- **Build target** (`A`, `D`): 0.
- **A different version's code:** excluded by the class definition, because class d needs the version indexed.
  Its analogue is **not-emitted**: the right version, but a part of the package the index's entry build never
  bundled. This is the biggest cause.

## 4. What would move class d (not built here)

1. **Not-emitted (pop 2.4-2.7, apps-esbuild/rollup ~2.1, task-master 1.75).** Build more of each package version:

   - every `exports` subpath and condition (`import` and `require`, `development` and `production`);
   - or the package's files directly as entries.

   This is index-build coverage, the same kind of lever as exp118's coverage, and needs its own sizing (index size
   grows with entries).

2. **Lazy-init / interop.** Build the eager AND the lazy (`__esm`, required-ESM) shape of each module: an entry
   that `require`s an ESM package as well as one that `import`s it. On held-out it is concentrated (one package is
   88%), so it is not a generic +1.
3. **No build on disk** is a lab gap, not a matcher gap.

## Reproduce

```bash
D=/work/exp122-data; S=experiments/122-hash-differs/scripts; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I $S/capture.py exp119-k2all-st; $PY -I $S/capture.py exp119-k2all-st-apps --corpus apps
$PY -I $S/capture.py pop-exp119-k2all-st --corpus pop
python3 -I $S/shard_owners.py /work/exp113-data/store/shards $D/owners 14
python3 -I $S/plan.py exp119-k2all-st exp119-k2all-st-apps pop-exp119-k2all-st
git apply experiments/122-hash-differs/proto/lab-hook.patch      # lab only; revert after building
(cd experiments/122-hash-differs/proto && CARGO_TARGET_DIR=$D/target cargo build --release)
git checkout crates/
$S/run_lab.sh "0 K T C P M D A L I O LIO KTCPMDA KTCPMDALIO"      # 8 x 2 threads
$PY -I $S/analyze.py KTCPMDALIO L I O K T C P M D A
python3 -I $S/diff.py && python3 -I $S/census.py
```

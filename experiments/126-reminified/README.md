# exp126: re-minified bundles — a dev corpus (`remin`) and exp122's normalisations on it

## STATUS (2026-10-17)

DONE on branch `exp126-reminified` (off main `b9d82d26`). Harness, corpus and lab only; **no pipeline change is
merged and no index was built.** The `units/4` form (const / let / var walk as one keyword) was implemented
test-first and is kept as `units4-declaration-kinds.patch`, NOT applied: the lab says it does not clear the bar on
the new corpus.

- **New dev corpus `remin`** (`npm run eval -- library <label> ... --corpus remin`): 36 exp108 TRAINING apps (12
  CLIs, 12 Electron apps, 12 VS Code extensions, seed 126; never held-out, test, test2, `apps` or `pop`), 72 bundles
  in two sets:

  - `remin-esbuild`: stage 1 re-minified by esbuild 0.28.2 `--minify` (wrangler-min's shape);
  - `remin-terser`: stage 1 re-minified by terser 5 (compress + mangle).

  Stage 1 is an esbuild 0.28.2 bundle, not minified, that uses each package's PUBLISHED minified entry where one
  exists (`unpkg` / `jsdelivr` / `browser` field, or a `.min` sibling): 84 entries swapped in 23 of the 36 apps,
  5.9 MB. Truth comes from the final, composed source map, exactly as for the other corpora, with every truth layer
  frozen (origins, copies, chained, origins7, chained9). Every entry runs with `--bundler esbuild`, the bundler that
  built it.

- **Main's shipped defaults on it** (k2-all-r5, structural, scorer 9; label `remin-exp126-main`), P / R / F0.5, app
  claimed 0 on both:

  | set           |     P |     R |  F0.5 | miss d (hash differs) |
  | ------------- | ----: | ----: | ----: | --------------------: |
  | remin-esbuild | 99.04 | 78.17 | 94.02 |              15.5 pts |
  | remin-terser  | 99.82 | 60.45 | 88.32 |              31.0 pts |
  | wrangler      | 95.99 | 18.53 | 52.28 |              32.3 pts |

- **exp122's normalisations, lab-first** (its lab hook and tool unchanged, k2-all-r5's builds). The figures are
  recovered points of library bytes, a recall CEILING:

  | variant                    | remin-esbuild | remin-terser | wrangler |
  | -------------------------- | ------------: | -----------: | -------: |
  | K const / let / var -> var |      **5.71** |         0.75 |    14.96 |
  | each of T C P M D A I O    |       <= 0.01 |      <= 0.01 |  <= 0.12 |
  | all ten together           |          5.86 |         0.84 |    15.11 |

  As an F0.5 ceiling (recovered bytes counted named, precision held), K is worth **+1.6 on remin-esbuild, +0.3 on
  remin-terser** and about +17 on wrangler. The bar was at least +2 on remin AND wrangler, so **step 3 (a new
  `LIBRARY_UNITS` version and a k2-all-r5 re-fingerprint) was not run.** Wrangler is the outlier of its own
  population: its ESM-heavy scored code carries the respelled keyword much more often than the population's.

- **The lever the corpus found instead: read a terser-minified bundle against terser rows.** remin-terser's class-d
  units are mostly terser's compress rewrites, which no light normaliser equalises. The SAME units read against the
  index's rollup slice (rollup + terser builds of the same packages) are recovered with no normalisation at all:
  15.91 pts of 30.95 (webpack slice: 13.92). The real matcher agrees. Lab index `rollup4esb` is k2-all-r5 with the
  esbuild slice's unit rows replaced by the rollup slice's (label `remin-exp126-lab-rollrows`):

  - remin-terser R 60.45 -> **78.12**, F0.5 88.32 -> **94.28 (+5.96)**, P 99.82 -> 99.42;
  - remin-esbuild F0.5 94.02 -> 90.53, as it should, since its code is esbuild-minified.

  The pipeline picks the slice by bundler; the minifier detector says `terser` on all 72 bundles, esbuild-minified
  ones included, so it cannot pick the rows today. Next lane: a minifier-aware slice choice, or a union of the
  bundler's slice and the terser slices, judged on precision on every corpus.

- **A scope note (rule 8).** The scorer counts statement and lazy-body units only; a CommonJS factory is one
  external unit with no prints on the query side. 92% of remin-esbuild's bundle bytes sit in CJS factories
  (apps-esbuild: 83%). That holds for every esbuild and Bun corpus; remin only makes it larger.

## 1. The corpus (`scripts/dev_remin.py`, `scripts/remin-build.mjs`)

- **Pool.** exp108's training apps of kind cli, electron or vscode with a good closure and a library closure of
  at most 15 MB. Excluded: exp100's test candidates, exp095's held-out apps, and every app of the `apps` and `pop`
  corpora. This leaves 1,376 apps; 12 per kind were drawn at random (seed 126). exp116's test2 excluded all of
  exp108 by construction.
- **Install.** exp108's lock file, `npm ci` (exp112 dev_pop's install).
- **Stage 1.** The esbuild JS API at 0.28.2, `bundle`, not minified, the closure's externals / loaders / format,
  `NODE_ENV=production`. The plugin `prefer-min` re-resolves each bare specifier:

  1. a package entry takes the first of `unpkg`, `jsdelivr`, `browser` (a string) naming an existing minified
     file;
  2. otherwise the resolved file's `.min` sibling.

  A file counts as minified if its name holds `.min.`, or if it is >= 2 KB with a mean line length >= 300. esbuild
  chains a swapped file's own source map when it ships one (scorer 5's `chained` layer relabels 189 sources per
  set).

- **Stage 2.** terser 5 (`compress`, `mangle`; `module` for an ESM stage 1) with the stage-1 map as input, or esbuild
  `--minify` on the stage-1 file, which composes its linked map. All three outputs sit at the same directory depth,
  so the map's relative sources resolve unchanged.
- **Freeze.** `corpus.build_truth` and `freeze_origins` (exp096), then copies (exp104, reference label
  `remin-exp126-main`), chained (scorer 5), origins7 (scorer 7; `verify`: rules off = the frozen layer, 72/72) and
  chained9 (scorer 9). Not done: the exp120 families universe was not re-derived with remin's truth labels. A
  truth package outside the universe is then in no derived family, which can only under-credit, never over-credit.

## 2. The lab (`lab/`, exp122's scripts with paths and set filters)

`capture.py` -> `plan.py --sets wrangler,remin-esbuild,remin-terser` (3,414 d-units, 3.64 MB, every one with a
build on disk, 451 builds) -> `run_lab.sh` -> `analyze.py`. The work dir `/work/exp124-data/full/work` (k2-all-r5's
own) is the first build source. Variant `0` recovers 0.00 on all three sets, the sanity check. `diff.py` and
`census.py` give each d-byte one cause (`census.json`, points of library bytes):

| set           |     d | norm: K | near: other | near: inlined | near: interop | near: lazy-init | partial | not-emitted |
| ------------- | ----: | ------: | ----------: | ------------: | ------------: | --------------: | ------: | ----------: |
| remin-esbuild | 15.53 |    5.71 |        4.23 |          0.14 |          0.38 |            0.36 |    0.54 |        4.00 |
| remin-terser  | 30.95 |    0.75 |        9.63 |          2.74 |          2.81 |            1.51 |    3.66 |        9.71 |
| wrangler      | 32.27 |   14.96 |        1.41 |          0.01 |          0.87 |            5.85 |    2.49 |        5.80 |

`plan.py --slice-as SET=SLICE` reads a set's d-units against another slice's builds. `lab-rollup` and
`lab-webpack` share the query prints and recompute only the index side:

| d-units read against      | remin-terser `0` |   `K` | all ten | remin-esbuild `0` |
| ------------------------- | ---------------: | ----: | ------: | ----------------: |
| esbuild slice (the run's) |             0.00 |  0.75 |    0.84 |              0.00 |
| rollup slice (terser)     |        **15.91** | 16.05 |   17.53 |              0.64 |
| webpack slice (terser)    |            13.92 | 13.93 |   13.86 |              0.59 |

(webpack: 3.88 pts of remin-terser's d have no webpack build on disk.)

## 3. What was not built, and what is kept

- `units4-declaration-kinds.patch` (apply with `git apply`) makes `CanonicalForm.declaration_kinds`
  (`Verbatim` | `Unified`) a field. Only `LIBRARY_UNITS` (units/4) unifies; every cross-version form keeps the
  keyword, and the forms pin test proves it: only the `units` section's pin moves. The patch also carries the red
  tests (serializer, forms registry, unit-model pins). Applying it needs a full k2-all-r5 re-fingerprint, because a
  units/3 index is refused by stamp.
- `scripts/seed_work.py` seeds a re-fingerprint work dir. Each version's node_modules comes from the source that
  provides most of its builds (exp124's lesson); 72,925 versions are seeded, 123 with builds from more than one
  source. It ran once and left `/work/exp126-data/full/work`, which holds hard links only and is deletable.
- Lab data: `/work/exp126-data/lab*` and `/work/exp126-data/ix-swap` (hard links) are deletable. Keep
  `/work/exp126-data/remin`, which holds the frozen corpus.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=experiments/126-reminified/scripts
python3 -I $S/dev_remin.py plan && python3 -I $S/dev_remin.py build --jobs 8
$PY -I $S/dev_remin.py freeze --jobs 12
npm run eval -- library remin-exp126-main --index /work/exp124-data/full/ix/k2-all-r5 --rule structural --corpus remin
$PY -I experiments/104-truth-audit/copies.py remin-exp126-main /work/exp112-data/registry \
  crates/humanify-core/src/libindex/families-derived.json --jobs 1 --corpus remin
$PY -I experiments/106-matcher-hygiene/scorer5/chained.py build --corpus remin
$PY -I experiments/112-test-precision/scorer7/origins7.py needs --corpus remin needs7.json   # + owners
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp112-data/registry needs7.json
python3 -I experiments/112-test-precision/scorer7/bundled.py owners7.json
$PY -I experiments/112-test-precision/scorer7/origins7.py verify|build --corpus remin
$PY -I experiments/120-truth-families/scorer9/chained9.py build --corpus remin
$PY -I experiments/096-library-matching-score/score.py remin-exp126-main --corpus remin
# lab (exp122's hook, applied only while building the lab tool)
L=experiments/126-reminified/lab
$PY -I $L/capture.py remin-exp126-main --corpus remin; $PY -I $L/capture.py exp124-r5
python3 -I $L/shard_owners.py /work/exp124-data/full/store/shards /work/exp126-data/lab/owners 14
python3 -I $L/plan.py exp124-r5 remin-exp126-main --sets wrangler,remin-esbuild,remin-terser
git apply $L/proto/lab-hook.patch && (cd $L/proto && CARGO_TARGET_DIR=/work/exp126-data/lab/target cargo build --release); git checkout crates/
$L/run_lab.sh "0 K T C P M D A L I O KTCPMDA KTCPMDALIO" && $PY -I $L/analyze.py K L KTCPMDALIO T C P M D A I O
python3 -I $L/diff.py && python3 -I $L/census.py
LAB_D=/work/exp126-data/lab-rollup python3 -I $L/plan.py remin-exp126-main --sets remin-terser,remin-esbuild \
  --slice-as remin-terser=rollup-node-cjs-min --slice-as remin-esbuild=rollup-node-cjs-min   # then run_lab/analyze
# the swapped-rows lab index: k2-all-r5 hard-linked, units.esbuild-* -> units.rollup-*, manifest table shas swapped
npm run eval -- library remin-exp126-lab-rollrows --index /work/exp126-data/ix-swap/rollup4esb --rule structural --corpus remin
```

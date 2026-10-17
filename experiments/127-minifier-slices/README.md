# exp127: minifier-aware slice choice — read the terser-built rows for terser-minified code

## STATUS (2026-10-17)

DONE on branch `exp127-minifier-slices` (off main `0e418cb8c`). One new config key with four modes, **default
`off`, NOT flipped**. The recommended mode is `auto` (`terser_share = 0.2`). It lowers precision on one set,
remin-terser, so by the lane's rule it stays a recommendation.

- **What it is.** `[library.lookup] terser_slices = "off" | "union" | "fallback" | "auto"`, plus `terser_share`
  (default 0.2, read by `auto` only). Every mode except `off` reads the index's terser-built slices (webpack and
  rollup: every minified slice whose bundler minifies with terser) beside the slice the bundle's recipe picked. Each
  owner is counted once, at its smallest module.
  - `union` reads them for every print.
  - `fallback` reads them only for a print the chosen slice does not hold.
  - `auto` decides once per bundle, before any lookup. Over the whole-unit prints the lookup reads, it measures the
    share of nodes that ONLY the terser slices hold. At `terser_share` or more, the bundle reads the union;
    otherwise it reads the chosen slice alone. The share and the verdict are written into the report's
    `sliceReason`.
- **Totals (k2-all-r5, main's shipped defaults, structural rule, scorer 9; app claimed 0 everywhere).** The `auto`
  mode at 0.2 gives byte-identical summaries to `off` on 18 of 21 sets: cc, esbuild-app, held-out, the three
  `apps` sets, both `bundlers` sets, both `pop` sets, and all four sets of test and of test2. The three sets that
  move:

  | set           | off P / F0.5    | auto 0.2 P / F0.5 | delta P / F0.5       |
  | ------------- | --------------- | ----------------- | -------------------- |
  | wrangler      | 95.992 / 52.281 | 98.013 / 65.854   | **+2.020 / +13.573** |
  | remin-esbuild | 99.044 / 94.024 | 99.047 / 94.107   | +0.003 / +0.083      |
  | remin-terser  | 99.821 / 88.315 | 99.496 / 95.679   | **-0.325** / +7.364  |

  On remin-terser, `auto` beats exp126's swapped-rows lab index (88.32 -> 94.28) on both precision and F0.5.

- **Why `union` and `fallback` are not recommended.** `union` raises F0.5 on 18 of 21 sets (wrangler +13.6,
  remin-terser +7.4, dev-webpack +3.5, test2 on all four). But it lowers precision on test-bun, test-esbuild,
  test-rollup and test-webpack (-0.06 to -0.13), and F0.5 on test-bun and test-esbuild. `fallback` is worse than
  `union` almost everywhere (remin-terser P 98.63).
- **Fixing the minifier verdict (option 1) was not needed.** The detector calls `terser` on all 72 remin bundles.
  The slice question no longer reads that verdict: the per-bundle share in `auto` is a minifier detector built
  from data, for slice choice only. Of the bundles whose prints hit anything, 21 of 22 remin-terser bundles have a
  share of at least 0.2 (19 of them 0.47 or more). Only 5 of 36 remin-esbuild bundles do (0.33 to 1.0; not diagnosed). The other 14 remin-terser bundles read 0, because none of their
  whole-unit prints hit any slice. The pipeline's own minifier verdict, which feeds rename eligibility
  and name profiles, is untouched, so Claude Code (Bun) is unaffected by construction.
- **Choosing per unit (option 3).** `fallback` is the per-print form of it, and it loses to `union` on precision.
  Choosing per bundle (`auto`) is what removes the precision cost on real apps. A finer per-unit choice could only
  sit between those two, and it was not built.
- **Gate:** ALL 13 STAGES PASSED, run bare at `0caaf10a1`, `64ae4c1dd` and `754878daf`.
  **Warm neutrality** of `754878daf` against main `0e418cb8c` (pair 2.1.215 -> 2.1.216, `--priors
/work/exp050-cold --cache /work/neutrality-cache`, under `flock /work/neutrality.lock`, run from a clean detached
  worktree):
  - default mode: **NEUTRAL, 0 files / 0 lines, cache +0 / +0, exit 0 / 0**;
  - report mode (`library.matching=report`, `library.index=k2-all-r5`): **NEUTRAL, 0 / 0, +0 / +0, exit 0 / 0**,
    and `.humanify/library-report.json` is byte-identical too.

## 1. The measurement (P / R / F0.5, and the deltas against `off`)

`off` is this branch with the key off (labels `*-exp127-off`). It equals main's `exp124-r5` labels on every set,
value for value. For test2 the `off` column is `test2-exp124-r5`, the same index and defaults on main; the branch's
own test2 `off` run was refused by run.py because the tree was dirty. Test and test2 are summaries only and were
never diagnosed.

| set           | off P / R / F0.5         | union                                      | fallback                                   |
| ------------- | ------------------------ | ------------------------------------------ | ------------------------------------------ |
| cc            | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 (+0.000 / +0.000) | 100.000 / 81.90 / 95.768 (+0.000 / +0.000) |
| esbuild-app   | 96.599 / 69.44 / 89.589  | 96.599 / 69.44 / 89.593 (+0.000 / +0.003)  | 96.599 / 69.45 / 89.594 (+0.001 / +0.005)  |
| heldout       | 99.310 / 67.60 / 90.791  | 99.309 / 67.66 / 90.812 (-0.000 / +0.021)  | 99.309 / 67.66 / 90.814 (-0.000 / +0.023)  |
| wrangler      | 95.992 / 18.53 / 52.281  | 98.013 / 28.48 / 65.854 (+2.020 / +13.573) | 97.217 / 27.02 / 63.980 (+1.225 / +11.699) |
| apps-esbuild  | 99.291 / 92.15 / 97.776  | 99.315 / 92.48 / 97.868 (+0.024 / +0.092)  | 99.303 / 92.47 / 97.856 (+0.012 / +0.081)  |
| apps-rollup   | 98.984 / 78.66 / 94.119  | 98.972 / 78.89 / 94.176 (-0.012 / +0.057)  | 98.971 / 78.77 / 94.141 (-0.013 / +0.022)  |
| apps-webpack  | 99.388 / 73.30 / 92.785  | 99.390 / 73.58 / 92.876 (+0.001 / +0.090)  | 99.388 / 73.59 / 92.876 (-0.001 / +0.091)  |
| dev-rollup    | 99.873 / 77.53 / 94.430  | 99.874 / 78.10 / 94.598 (+0.001 / +0.168)  | 99.873 / 77.53 / 94.430 (+0.000 / +0.000)  |
| dev-webpack   | 99.642 / 62.89 / 89.216  | 99.688 / 72.51 / 92.736 (+0.046 / +3.520)  | 99.687 / 72.35 / 92.683 (+0.045 / +3.467)  |
| pop-bun       | 95.509 / 79.34 / 91.768  | 95.466 / 79.90 / 91.885 (-0.043 / +0.117)  | 95.339 / 79.75 / 91.752 (-0.170 / -0.016)  |
| pop-esbuild   | 94.079 / 81.42 / 91.243  | 94.053 / 81.56 / 91.258 (-0.027 / +0.015)  | 94.047 / 81.53 / 91.245 (-0.032 / +0.002)  |
| remin-esbuild | 99.044 / 78.17 / 94.024  | 99.076 / 80.71 / 94.763 (+0.032 / +0.739)  | 99.076 / 80.71 / 94.763 (+0.032 / +0.739)  |
| remin-terser  | 99.821 / 60.45 / 88.315  | 99.496 / 82.95 / 95.679 (-0.325 / +7.364)  | 98.628 / 82.84 / 95.007 (-1.193 / +6.692)  |
| test-bun      | 97.852 / 67.56 / 89.798  | 97.784 / 67.66 / 89.789 (-0.068 / -0.010)  | 97.713 / 67.58 / 89.713 (-0.139 / -0.086)  |
| test-esbuild  | 97.005 / 76.80 / 92.156  | 96.874 / 76.87 / 92.081 (-0.131 / -0.074)  | 96.884 / 76.87 / 92.088 (-0.121 / -0.068)  |
| test-rollup   | 97.938 / 68.80 / 90.292  | 97.882 / 69.20 / 90.389 (-0.056 / +0.097)  | 97.900 / 69.01 / 90.336 (-0.037 / +0.044)  |
| test-webpack  | 99.242 / 70.62 / 91.802  | 99.183 / 71.12 / 91.929 (-0.059 / +0.127)  | 99.237 / 71.20 / 91.991 (-0.005 / +0.189)  |
| test2-bun     | 98.850 / 79.30 / 94.205  | 98.827 / 79.41 / 94.219 (-0.024 / +0.013)  | 98.850 / 79.39 / 94.230 (-0.000 / +0.025)  |
| test2-esbuild | 99.489 / 75.80 / 93.637  | 99.496 / 75.87 / 93.663 (+0.007 / +0.025)  | 99.491 / 75.88 / 93.662 (+0.003 / +0.024)  |
| test2-rollup  | 96.327 / 75.35 / 91.247  | 96.457 / 75.49 / 91.381 (+0.131 / +0.134)  | 96.386 / 75.41 / 91.306 (+0.059 / +0.060)  |
| test2-webpack | 99.275 / 70.08 / 91.639  | 99.276 / 70.27 / 91.705 (+0.001 / +0.066)  | 99.264 / 70.25 / 91.690 (-0.011 / +0.051)  |

`auto` at 0.2 (labels `*-exp127-auto20`) equals `off` exactly on every set except the three in the STATUS table.

**Choosing the share (dev data only).** The `probe` labels run `auto` at share 1.0, which records each bundle's
share and changes nothing measurable. `simulate.py` then rebuilds `auto(tau)` EXACTLY from per-bundle cards: a
bundle's verdict is made before any lookup, so its `auto` output is its `union` output or its `off` output. On
every dev set at tau 0.15 to 0.3, `auto` equals `off`, except wrangler (+13.57) and remin. At tau 0.05, held-out and
pop-bun move with P down. 0.2 keeps the whole remin-terser gain (tau 0.1 and 0.2 are equal there; at 0.3 F0.5 falls
to 94.60). The real runs reproduced the simulation to the last digit (remin 94.107 / 95.679). Test and test2 were
scored once, at the chosen 0.2.

## 2. What the precision cost is (dev sets only)

`wrongs.py` lists wrong-family bytes per (claimed package, truth package).

- **remin-terser, `auto`: +22,951 wrong bytes.**
  - 14,329 B: `onnxruntime-web` claimed where the truth says `@huggingface/transformers`. **Hand-checked as a
    vendored copy**: `transformers.min.js` in that app's node_modules inlines onnxruntime-web (10x
    `InferenceSession`, the `ort-wasm-*` loaders), so the claim names the code's origin. The frozen copies layer was
    derived from the reference label `remin-exp126-main`, which never claimed these units, so it gives no origin
    credit. Counted as right, P would be about 99.73.
  - 9,393 + 1,066 B: inlined tslib helpers (`__awaiter` / `__generator` …) claimed as `@exasol/exasol-driver-ts` or
    `react-to-print`. tslib owns no row for that statement, and the rows of the many packages that inline it sit
    above the specificity bound, except one. This class predates exp127: the same exasol-for-tslib claims already
    cost `off` on remin-esbuild (5 x 1,020 B). `helper_families` 8 or 12 removes the exasol seeds, but
    react-to-print then takes the code by module propagation (remin-terser P 99.48 / 99.42). Not fixed here.
  - The rest is under 1 KB per pair (`open` propagating into its dependencies `is-docker`, `powershell-utils` …).
- **pop under `union` (not under `auto`): the largest added wrongs are vendored copies too.** One example:
  `regexp-tree` and `@babel/types` claimed in `@putout/minify`, whose published `bundle/minify.min.js` inlines
  regexp-tree's parser (`_hasUFlag`, `parseRegExp`). `vite` -> `chokidar` on apps-rollup looks like the same case.
  These figures are a scorer-truth gap, not proof that the claims are wrong. The fix belongs in the copies layer,
  re-derived with hand checks (exp104's protocol), not in this lane.

## 3. Code (test-first)

- `libindex::recipe`: `RecipeBundler::minifier`, `TerserSlices`, `terser_slices` (the terser-built slices other
  than the chosen one, in manifest order).
- `libindex::lookup::LibraryIndex`: `load(dir, recipe, terser)` opens the extra unit tables. `unit_hits` merges
  them, each owner once at its smallest module. `decide_terser_slices` (OnceLock) holds `auto`'s per-bundle
  verdict. `slice_reason()` names the slices read and the verdict.
- `libdetect::discovery`: one call, `decide_terser_slices(&prints, &rules)`, right after the bundle's prints and
  before any lookup.
- Config: `library.lookup.terser_slices` (a choice), `library.lookup.terser_share` (a float, default 0.2),
  `humanify.example.toml` regenerated, a row in `docs/library-matching-config.md` and in `docs/responsibility.md`.
- Tests: `recipe_test` (which slices are terser-built), `lookup_test` (union, fallback and off read from an on-disk
  index of three slices; auto's share and verdict on both sides of the floor), `library_config_test` (default off,
  every spelling, a refused value). The union, fallback and off tests were run red (they did not compile against
  the old API) before the code was written. The auto test was written before its code, but its red was not run on
  its own.

## 4. Caveats

- The labels `test2-exp127-union` and `test2-exp127-fallback` ran from the worktree's `target/release`, and a gate
  rebuilt that binary at `64ae4c1dd` while they were running. The `union` and `fallback` code paths are identical in
  both commits (`64ae4c1dd` only added `auto`, and its verdict is never set outside `auto`), so the outputs are
  unaffected. Their pipeline.json records the first binary. Every later label was built with `--commit` in its own
  worktree.
- Cost: `union` and `auto` open two more unit tables (memory-mapped) and do up to three lookups per print.

## Reproduce

```bash
S=experiments/127-minifier-slices; IX=/work/exp124-data/full/ix/k2-all-r5
for c in dev apps bundlers pop remin test test2; do   # label prefix: <corpus>-, none for dev
  npm run eval -- library <c>-exp127-auto20 --index $IX --rule structural --corpus $c --commit 754878daf \
    --pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2
done   # likewise =off / =union / =fallback; probe = auto at terser_share=1.0 (dev corpora only)
python3 -I $S/simulate.py remin remin-exp127-off remin-exp127-union remin-exp127-probe 0.1 0.2 0.3
/work/exp095-library-fingerprints/scale/.venv/bin/python -I $S/wrongs.py remin-exp127-auto20 --corpus remin --set remin-terser
```

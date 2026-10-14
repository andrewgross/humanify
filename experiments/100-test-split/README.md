# exp100: a frozen TEST split for the library-matching score

## STATUS (2026-10-14)

- **FROZEN. Never tune on it.** The test split is 55 new apps (30 npm CLIs, 25 Electron apps), each built with
  Bun (the held-out recipe) and esbuild (the index's esbuild recipe), and where the build succeeds with webpack +
  terser (41 apps) and rollup + terser (40 apps): **191 bundles in four sets** (`test-bun`, `test-esbuild`,
  `test-webpack`, `test-rollup`). It exists so that the overnight F1 campaign's changes can be judged on apps
  that no tuning lane has seen.
- **Selection rule (mechanical, recorded in `selection.json`):**
  - **CLI:** exp095's own frozen npm-high-impact top-download list (`npm-top.json`, sha256 in `selection.json`),
    ranks **6,001 and beyond**. exp095 drew its CLIs from ranks 1-6,000, so none of these were candidates there.
    exp095's qualifying rule, unchanged: not `@types/`, the latest manifest has a `bin`, it is not deprecated,
    and it has ≥ 5 dependencies. Candidates are taken in rank order (ranks 6,009-7,863 hold 45). The test set
    keeps **the first 30 whose install and Bun build succeed**.
  - **Electron:** the electron/apps catalog exp095 used is exhausted (it read every one of its 614 entries). This
    split instead uses GitHub's `topic:electron fork:false archived:false` search, sorted by stars (the first
    1,000 results, fetched 2026-10-14, ties broken by name), **minus every catalog repository**. exp095's
    qualifying rule, unchanged: a root package.json, `electron` in its (dev)dependencies, and ≥ 3 runtime library
    dependencies after the tooling filter. The test set keeps **the first 25 in star order whose install and Bun
    build succeed**.
  - **Excluded by name or repository:** every app exp095's selection tried. That is its 360-app corpus (the 338
    bundled apps, which hold the 67 held-out apps, plus 22 that failed) and every app its log dropped. So no test
    app was in the 338-app corpus the index's package list was chosen from, and none is a held-out app.
  - Failed builds move the cut; they never change which apps were chosen. The cut skipped one CLI (`jest-expo`:
    its Bun build failed) and two Electron apps (their install failed).
- **Builds** (`scripts/build.py`): the install, entry and externals are exp095's `closure.one`, unchanged. Each
  app is built four ways:

  - **Bun 1.4.0** `build --target node --format cjs --minify --sourcemap=external`, the held-out recipe, with its
    top-level-await fallback.
  - **esbuild 0.28.2** `--bundle --minify --platform=node --format=cjs --sourcemap`, with the closure's externals.
  - **webpack 5.111.1** in production mode (its default terser minimizer), target node, one chunk
    (`scripts/webpack-build.cjs`). Sources are written relative to the output directory, and webpack's runtime
    modules are spelled `\0webpack/…`.
  - **rollup 4** + node-resolve + commonjs + json + terser, format cjs (exp095 lab `rollup-build.mjs`), retried
    as ES output when the code has top-level await.

  The truth is the same as the held-out truth: the source map, through exp096 `corpus.map_truth` and
  `build_origins`, unchanged. One generic rule was added to `sources_table`: a source spelled `\0…` is a
  bundler-generated module with no file (rollup's helpers, webpack's runtime), so it is labelled `<unmapped>`
  and never judged.

- **webpack and rollup are not indexed, and discover cannot read them today.** The index has Bun and esbuild
  slices only, and `discover` refuses a bundler that has no slice. So those two sets run with `--bundler
esbuild`: they are read through the esbuild slice, and the manifest's `builtWith` names the real bundler. On
  main, the Bun and esbuild adapters unpack no module from a webpack or rollup bundle (the one-file floor), so
  those sets score as no discovery (recall 0). That zero is the honest baseline for the generality question.
- **Baseline** (main `e9b8c0f0`, index `/work/exp097-data/ix/step4`, labels `test-step4f-structural` /
  `test-step4f-agreement`). Per-set aggregates only, P / R / F1:

| set (bundles, library MB)  | structural P / R / F1                 | agreement P / R / F1     | app claimed (str / agr) |
| -------------------------- | ------------------------------------- | ------------------------ | ----------------------- |
| `test-bun` (55, 66.0)      | 49.7 / 13.1 / 20.8                    | 55.9 / 12.2 / 20.0       | 0 / 0                   |
| `test-esbuild` (55, 68.2)  | 48.1 / 13.5 / 21.1                    | 55.6 / 11.2 / 18.7       | 13,806 B / 0            |
| `test-webpack` (41, 162.5) | 0 / 0 / 0 (no discovery)              | 0 / 0 / 0 (no discovery) | 0 / 0                   |
| `test-rollup` (40, 117.8)  | 10.3 / 0.1 / 0.1 (discovery ran on 6) | 18.6 / 0.0 / 0.1         | 76 B / 72 B             |

For comparison, held-out 67 is 96.1 / 23.9 / 38.3 (structural). **Read the test PRECISION with care.** Most
wrong-family bytes on `test-bun` and `test-esbuild` are pre-bundled copies: library code that a shipping
package inlined into its own `dist/` chunks, with no vendored directory or module marker. The origin layer
cannot see such a copy, so the truth credits it to the shipper while the matcher names the origin. The
largest contributors were checked by the shipped chunk's file name, which names the inlined package. This is exp096 §4's known truth limit, much more common in
this population. Recall and the hard bars (app claimed) are unaffected. Judge a change by its precision DELTA
on the test split, and fix the truth (an origin-layer improvement, re-frozen as a new split version) before
quoting absolute test precision. Not-indexed is the dominant miss: 59% of `test-bun` and 75% of
`test-esbuild` library bytes.

## Integrity rules

- Tuning lanes must not read anything under `/work/exp100-test-data/`. That directory holds the bundles, truth,
  raw reports and the per-bundle cards. They must not choose packages, rules or thresholds from the test apps'
  identities (`selection.json`), and must not run test labels while iterating. A test label is run once per
  candidate that is ready to merge.
- For a test label, `run.py --corpus test` writes the cards beside its reports under the test data dir, not
  into git. It commits only `summary.json` (per-set aggregates), `pipeline.json` and an aggregate `timing.json`,
  and its run log names no bundle.
- Re-freezing, adding apps, or re-cutting the set makes a NEW split (a new manifest sha). Every test label must
  then be re-run.

## Why a separate manifest, not a bump of corpus.json

`corpus-test.json` and `origins-test.json` sit beside the dev manifest. `lib096.use("test")` (`--corpus test`
on `run.sh`, `score.py`, `corpus.py verify|origins` and `npm run eval -- library`) points every path at them
and at `/work/exp100-test-data/frozen/`. Three reasons:

1. Every committed dev label keeps its `corpusSha256` and its cards byte for byte. No re-score is needed, and a
   dev run's `pipeline.json` and `summary.json` are spelled exactly as before (`corpus` appears only on test
   labels).
2. The split stays physically separate, so excluding it is the default. A tuning lane that runs `run.sh` without
   `--corpus test` cannot score the test set by accident, and the leaderboard refuses to put a dev label beside a
   test label (different corpus manifests).
3. Bumping the shared manifest would have put the test bundles' per-app cards into every label's committed
   `cards/`, where every tuning lane reads them.

## How to run

```bash
P=/work/exp095-library-fingerprints/scale/.venv/bin/python
npm run eval -- library test-<label> --index <dir> --rule structural|agreement --corpus test --cpus 0-15
$P -I experiments/096-library-matching-score/leaderboard.py test-step4f-structural test-<label>
$P -I experiments/096-library-matching-score/corpus.py verify --corpus test
```

Rebuilding from scratch (network; responses are cached under `/work/exp100-test-data/cache`):

```bash
python3 -I experiments/100-test-split/scripts/select_apps.py          # candidates.json
taskset -c 0-15 python3 -I experiments/100-test-split/scripts/build.py --jobs 8   # installs + 4 builds, the cut
$P -I experiments/100-test-split/scripts/freeze_test.py --jobs 12      # corpus-test.json + truth
$P -I experiments/096-library-matching-score/corpus.py origins --corpus test
```

## Caveats

- **Electron population differs from the held-out one.** The source is GitHub topic search, not the catalog. The
  set is newer and more monorepo-shaped (playwright, AFFiNE and bitwarden qualify through their root
  package.json), and Electron apps install their declared ranges as resolved today.
- **CLI population is lower-ranked** (ranks 6,009+ vs 1-6,000): less popular CLIs, so they are likely further
  from the index's package choice than the held-out CLIs were.
- **Versions resolve today**, as for the held-out apps: the as-of policy's best case.
- **Bun 1.4.0's maps misattribute some modules in large bundles** (exp095 selection §Status). The test truth
  inherits that, as the held-out truth does.
- webpack built 41 of the 55 apps and rollup built 40. A build failure drops the app from that slice only, so the
  four sets are not the same app list.
- The GitHub search result order is a fetch-time fact. The fetched pages are cached, and the chosen apps are
  recorded with their star counts.

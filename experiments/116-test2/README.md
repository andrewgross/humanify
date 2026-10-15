# exp116: test split v2, an untouched final judge for library matching

## STATUS (2026-10-15)

- **FROZEN. Never tune on it. Never diagnose it.** This split is a JUDGE, not a dev set. No lane may read its
  per-app results, its bundles, truth or reports (`/work/exp116-test2-data/`), or choose packages, rules or
  thresholds from its app identities (`selection.json`). Do not run aggregate decompositions of its wrong or missed
  bytes, by app or by mechanism. That is how exp100's split was spent. A `test2` label is run once per candidate
  that is ready to merge, and only its per-set summary is read.
- **Why it exists.** exp100's test split was partly spent: exp104, exp107 and exp112 ran aggregate diagnoses on
  its wrong and app-claimed bytes, and exp112 found its truth rules there before it validated them on dev. test2
  holds new apps that no lane has seen.
- **What it is.** 55 apps (30 npm CLIs, 25 Electron apps) built four ways with exp100's scripts, unchanged:
  **188 bundles**, as `test2-bun` 55, `test2-esbuild` 55, `test2-webpack` 44 and `test2-rollup` 34.
- **The truth uses the current scorer's layers: scorer 7** (`exp096-score/7`; exp112 merged to main at
  `bc59bd57`, and this branch merged it). The layers are:
  - the source-map truth and the scorer 4-6 origins (`corpus.py`);
  - scorer 7's `origins7` layer (lazybody + chunk; `origins7-test2.json`). `origins7.py verify` with every rule
    off: 188/188 entries equal the frozen origin layer;
  - chained (`chained-test2.json`);
  - copies (`copies-test2.json`). Its reference label is `test2-ref-wr-structural`: the step4-wr-v3 index, main's
    shipped default, all four sets. Rebuilt after the reference re-ran at `bc59bd57`, the copies layer was
    identical to the one built at `b4db2b76`.
- **Main's shipped default on test2** (main `bc59bd57`, structural, the recommended tunables that are now the
  `[library]` defaults; k2-l12m4 index for Bun/esbuild, step4-wr-v3 for webpack/rollup). Per-set aggregates only:

| set (bundles, library MB)   | P     | R     | F0.5  | F1    | app claimed | label                      |
| --------------------------- | ----- | ----- | ----- | ----- | ----------- | -------------------------- |
| `test2-bun` (55, 41.6)      | 98.52 | 77.65 | 93.50 | 86.85 | 0           | `test2-main-k2-structural` |
| `test2-esbuild` (55, 43.1)  | 99.41 | 74.17 | 93.07 | 84.95 | 0           | `test2-main-k2-structural` |
| `test2-webpack` (44, 170.6) | 98.29 | 31.76 | 69.27 | 48.01 | 0           | `test2-main-wr-structural` |
| `test2-rollup` (34, 59.1)   | 96.69 | 44.87 | 78.55 | 61.29 | 0           | `test2-main-wr-structural` |

No set reaches the 99.8% precision bar. App claimed is 0 everywhere. These numbers are a baseline for deltas.
Nothing here says why a set scores what it does, and nothing should be read into it. Asking why would spend the
split.

## Selection rule (mechanical; `scripts/select_apps.py`, recorded in `selection.json`)

exp100's method, with two sources that no earlier lane read:

- **CLI:** GitHub `topic:cli fork:false archived:false language:JavaScript language:TypeScript`, sorted by stars
  (first 1,000 results, ties broken by name). The repo's root package.json must name a published, non-private
  package that has a `bin`. That package must then pass exp095's CLI rule, unchanged: its npm `latest` manifest
  has a `bin`, it is not deprecated, and it has ≥ 5 dependencies. Its `repository` must also name the same GitHub
  repo, so a name collision cannot qualify. It is built as the published CLI, exactly as exp100's CLIs were. The
  npm download lists are spent: exp095 read ranks 1–6,000, exp100 read 6,001–7,863, and exp108 took every
  qualifier left in the union list.
- **Electron:** GitHub `topic:electron-app fork:false archived:false`, sorted by stars (first 1,000 results). It
  uses exp095's qualifying rule, unchanged: a root package.json, `electron` in its (dev)dependencies, and ≥ 3
  runtime library dependencies. `topic:electron` itself was read to 4,816 rows by exp108.
- **Excluded by identity** (npm name, GitHub repo or app slug), before any rule:

  - exp095's 360-app selection corpus and its drop log, which hold the 67 held-out apps;
  - the electron/apps catalog;
  - the dev corpus apps;
  - all 90 exp100 candidates;
  - exp108's 1,897-app training corpus and its drop log;
  - exp107's `apps` corpus and exp112's `pop` corpus (`corpus-pop.json`, now on main).

  In all, 6,623 names, 3,554 repos and 2,347 slugs.

- **The cut:** the first 30 CLIs and the first 25 Electron apps, in candidate order, whose install and Bun build
  succeed. A failed build moves the cut and never changes which apps were chosen. Four CLIs were skipped because
  their Bun build failed. A failed esbuild, webpack or rollup build drops the app from that slice only.

## Builds and truth

- `experiments/100-test-split/scripts/build.py`, run unchanged with `SEL=/work/exp116-test2-data`, produced:
  - Bun 1.4.0 (the held-out recipe);
  - esbuild 0.28.2;
  - webpack 5.111.1 + terser (exp100's `webpack-build.cjs`; tools from exp100's lockfile);
  - rollup 4 + terser.
- `freeze_test.py --split test2` (exp100's freezer, with the split as a parameter) wrote `corpus-test2.json`.
  Entries are `test2-<bundler>-<slug>` and sets are `test2-<bundler>`.
- `corpus.py origins --corpus test2` wrote the origin layer.
- `chained.py build --corpus test2` wrote scorer 5's chained layer.
- `copies.py test2-ref-wr-structural … --corpus test2` wrote exp104's copies layer. Its registry cache was
  fetched for the truth's packages (`scripts/universe.py` + exp104 `fetch_metadata.py`).

## Harness wiring

- `lib096.CORPORA["test2"]` holds the manifest, the layers and `/work/exp116-test2-data/frozen`.
- `lib096.PRIVATE = ("test", "test2")`: on a private corpus, score.py keeps the per-bundle cards beside the
  reports under the data dir, out of git. It commits only `summary.json`, `pipeline.json` and `timing.json`. The
  run log names no bundle, and `app_claims.py` refuses the corpus.
- `npm run eval -- library … --corpus test2` is accepted (`scripts/eval.ts`, with a unit test).
- The leaderboard knows the `test2-*` set names. It refuses to mix test2 labels with dev or test labels, because
  each has a different corpus manifest.

## How to run (a merge candidate, once)

```bash
P=/work/exp095-library-fingerprints/scale/.venv/bin/python
npm run eval -- library test2-<label>-k2 --index /work/exp108-data/idx/ix/k2-l12m4 --rule structural \
  --corpus test2 --sets test2-bun,test2-esbuild --cpus 0-11
npm run eval -- library test2-<label>-wr --index /work/exp103-data/ix/step4-wr-v3 --rule structural \
  --corpus test2 --sets test2-webpack,test2-rollup --cpus 0-11
$P -I experiments/096-library-matching-score/leaderboard.py test2-main-k2-structural test2-<label>-k2
$P -I experiments/096-library-matching-score/corpus.py verify --corpus test2
```

Rebuilding from scratch (network; responses cached under `/work/exp116-test2-data/cache`):

```bash
python3 -I experiments/116-test2/scripts/select_apps.py
SEL=/work/exp116-test2-data taskset -c 0-11 python3 -I experiments/100-test-split/scripts/build.py --jobs 4
python3 -I experiments/116-test2/scripts/selection.py
$P -I experiments/100-test-split/scripts/freeze_test.py --jobs 10 --split test2
$P -I experiments/096-library-matching-score/corpus.py origins --corpus test2
$P -I experiments/106-matcher-hygiene/scorer5/chained.py build --corpus test2
$P -I experiments/112-test-precision/scorer7/origins7.py needs --corpus test2 /work/exp116-test2-data/needs7.json
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp112-data/registry /work/exp116-test2-data/needs7.json
$P -I experiments/112-test-precision/scorer7/origins7.py owners --corpus test2 /work/exp116-test2-data/owners7.json
python3 -I experiments/112-test-precision/scorer7/bundled.py /work/exp116-test2-data/owners7.json --jobs 6
$P -I experiments/112-test-precision/scorer7/origins7.py verify --corpus test2 --jobs 10   # rules off = frozen layer
$P -I experiments/112-test-precision/scorer7/origins7.py build --corpus test2 --jobs 10
npm run eval -- library test2-ref-wr-structural --index /work/exp103-data/ix/step4-wr-v3 --rule structural \
  --corpus test2 --commit bc59bd57   # its first score fails: no copies layer yet
python3 -I experiments/116-test2/scripts/universe.py --corpus test2 /work/exp116-test2-data/universe.json
python3 -I experiments/104-truth-audit/fetch_metadata.py /work/exp116-test2-data/registry /work/exp116-test2-data/universe.json
$P -I experiments/104-truth-audit/copies.py test2-ref-wr-structural /work/exp116-test2-data/registry \
  crates/humanify-core/src/libindex/families-derived.json --jobs 1 --corpus test2
$P -I experiments/096-library-matching-score/score.py test2-ref-wr-structural --corpus test2
```

## Caveats

- **The population differs from held-out and from exp100's test split.** These are GitHub-topic CLIs and `topic:electron-app` apps
  (CLI stars 2,389–241,250; Electron stars 223–63,507). The library denominator is smaller per Bun/esbuild bundle than on
  exp100's test split (41.6 vs 66.0 MB for 55 Bun bundles). As on exp100, webpack's denominator is much larger
  than Bun's for the same apps. Compare test2 labels only with test2 labels.
- Versions resolve as of today (2026-10-15), and Bun 1.4.0's map misattribution is inherited, as for exp100.
- **Re-cutting, adding apps or changing a truth layer makes a NEW split version.** Every test2 label must then be
  re-run. When a scorer bump lands, re-score the committed test2 labels with `score.py --corpus test2`. That
  writes new summaries, and the cards stay private.

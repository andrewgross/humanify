# exp138: re-judging the `helper_prints = "seed"` + `terser_slices = "auto"` default flip

## STATUS (2026-10-22)

DONE on branch `default-helpers-terser` (off main `869931920`).

- **The pair is NOT flipped.** It fails the flip bar on one set, remin-terser (precision 99.90 → 99.32).
  `terser_slices` stays `off`.
- **`helper_prints = "seed"` is FLIPPED ALONE** (Andrew, 2026-10-22). It meets the bar on every set: precision is
  never lower, and F0.5 is at worst -0.000006.

  - The change is in `DiscoveryOptions::recommended()`. The pin test
    `the_exp138_helper_seed_defaults_on_and_turns_off` asserts that seed is the default, that it turns off from
    the file, and that `terser_slices` stays off.
  - `humanify.example.toml` was regenerated and `docs/library-matching-config.md` updated.
  - Gate and warm neutrality results are in the commit message of the flip.

- **What was re-measured.** exp129 recommended flipping both keys together. It measured them under scorer 12 on
  k2-all-r5-origin-h, before `factory_bodies` (with its tie fence, banner index and whole seed) became the default
  and before the production index. This lane re-scores them on today's setup:
  - main `869931920`'s plain defaults;
  - the recommended index `/work/exp135-data/full/ix/k2-all-r5-origin-prod` (it holds `helpers.jsonl`);
  - structural rule, scorer 13, both scopes;
  - every corpus.
- **The baseline** is exp135's `*exp135-prod*` labels. They were built at `6ab6da5c2`, and `crates/` is unchanged
  from there to `869931920`, so they are main's current defaults on this index.
- **The variants.** Each adds `--pipeline-set` overrides to `experiments/135-noprint-census/scripts/score-all.sh`:
  - `*exp138-helper*`: `library.discovery.helper_prints=seed`;
  - `*exp138-terser*`: `library.lookup.terser_slices=auto` and `terser_share=0.2`;
  - `*exp138-both*`: both keys.
- **The flip bar:**
  - precision at or above the base on every set;
  - F0.5 at or above the base on every set, apart from losses of 0.002 or less;
  - app claimed bytes 0 everywhere.
- **Results:**
  - helper alone: **MET.** Precision is never lower. F0.5 falls by at most 0.000006 (test-webpack). App claimed 0.
  - terser alone: **NOT MET.** It is inert on every set except the two remin sets. remin-terser P falls 0.60.
  - both: **NOT MET**, on remin-terser alone: P -0.58 (wrong bytes 18.6 → 339.8 KB), against F0.5 +28.66.
- **Why the result differs from exp129.** Before factory bodies, the terser slices barely reached remin-terser's
  big bundles. exp129's `auto+seed` labels claim nothing at all in the worst bundle below. Now that factory
  modules are cut into body statements, the terser-built rows match much more:

  - remin-terser recall goes 20.3 → 53.3 %, 30.8 MB more named;
  - the matches include copies whose truth is another package.

  wrangler's exp127/129 gain (+10..13 F0.5) is gone: factory bodies already took its base from about 41 to
  70.9 F0.5, and `auto` now moves it by 0.

## 1. Table (P / R / F0.5 in %, both scopes; deltas against base, P / F0.5)

test and test2 are summaries only. test2 is frozen and was never diagnosed.

| set           | base (exp135-prod)       | helper only              | terser only              | both                     | d helper        | d terser            | d both              |
| ------------- | ------------------------ | ------------------------ | ------------------------ | ------------------------ | --------------- | ------------------- | ------------------- |
| cc            | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | +0.000 / +0.000 | +0.000 / +0.000     | +0.000 / +0.000     |
| esbuild-app   | 99.066 / 74.74 / 93.012  | 99.078 / 74.74 / 93.020  | 99.066 / 74.74 / 93.012  | 99.078 / 74.74 / 93.020  | +0.012 / +0.009 | +0.000 / +0.000     | +0.012 / +0.009     |
| heldout       | 99.565 / 68.51 / 91.289  | 99.565 / 68.51 / 91.289  | 99.565 / 68.51 / 91.289  | 99.565 / 68.51 / 91.289  | +0.000 / -0.000 | +0.000 / +0.000     | +0.000 / -0.000     |
| wrangler      | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | +0.000 / +0.000 | +0.000 / +0.000     | +0.000 / +0.000     |
| apps-esbuild  | 99.886 / 77.65 / 94.476  | 99.905 / 77.65 / 94.488  | 99.886 / 77.65 / 94.476  | 99.905 / 77.65 / 94.488  | +0.018 / +0.012 | +0.000 / +0.000     | +0.018 / +0.012     |
| apps-rollup   | 99.093 / 79.31 / 94.384  | 99.112 / 79.31 / 94.398  | 99.093 / 79.31 / 94.384  | 99.112 / 79.31 / 94.398  | +0.020 / +0.014 | +0.000 / +0.000     | +0.020 / +0.014     |
| apps-webpack  | 99.580 / 73.50 / 92.981  | 99.599 / 73.50 / 92.994  | 99.580 / 73.50 / 92.981  | 99.599 / 73.50 / 92.994  | +0.018 / +0.013 | +0.000 / +0.000     | +0.018 / +0.013     |
| dev-rollup    | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | +0.000 / +0.000 | +0.000 / +0.000     | +0.000 / +0.000     |
| dev-webpack   | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | +0.000 / +0.000 | +0.000 / +0.000     | +0.000 / +0.000     |
| pop-bun       | 99.495 / 77.19 / 94.060  | 99.495 / 77.19 / 94.060  | 99.495 / 77.19 / 94.060  | 99.495 / 77.19 / 94.060  | +0.000 / +0.000 | +0.000 / +0.000     | +0.000 / +0.000     |
| pop-esbuild   | 99.304 / 78.71 / 94.365  | 99.313 / 78.71 / 94.372  | 99.304 / 78.71 / 94.365  | 99.313 / 78.71 / 94.372  | +0.009 / +0.006 | +0.000 / +0.000     | +0.009 / +0.006     |
| remin-esbuild | 99.691 / 51.76 / 84.113  | 99.719 / 51.76 / 84.129  | 99.691 / 51.84 / 84.157  | 99.720 / 51.84 / 84.173  | +0.029 / +0.016 | +0.001 / +0.044     | +0.029 / +0.060     |
| remin-terser  | 99.902 / 20.33 / 56.043  | 99.902 / 20.33 / 56.043  | 99.303 / 53.31 / 84.689  | 99.322 / 53.31 / 84.700  | +0.000 / +0.000 | **-0.599** / +28.65 | **-0.580** / +28.66 |
| test-bun      | 99.551 / 70.16 / 91.854  | 99.551 / 70.16 / 91.854  | 99.551 / 70.16 / 91.854  | 99.551 / 70.16 / 91.854  | +0.000 / -0.000 | +0.000 / +0.000     | +0.000 / -0.000     |
| test-esbuild  | 99.260 / 73.37 / 92.717  | 99.270 / 73.37 / 92.724  | 99.260 / 73.37 / 92.717  | 99.270 / 73.37 / 92.724  | +0.010 / +0.007 | +0.000 / +0.000     | +0.010 / +0.007     |
| test-rollup   | 98.961 / 72.43 / 92.206  | 98.966 / 72.43 / 92.210  | 98.961 / 72.43 / 92.206  | 98.966 / 72.43 / 92.210  | +0.005 / +0.003 | +0.000 / +0.000     | +0.005 / +0.003     |
| test-webpack  | 99.476 / 72.19 / 92.486  | 99.483 / 72.18 / 92.485  | 99.476 / 72.19 / 92.486  | 99.483 / 72.18 / 92.485  | +0.007 / -0.001 | +0.000 / +0.000     | +0.007 / -0.001     |
| test2-bun     | 99.262 / 69.37 / 91.385  | 99.262 / 69.37 / 91.385  | 99.262 / 69.37 / 91.385  | 99.262 / 69.37 / 91.385  | +0.000 / -0.000 | +0.000 / +0.000     | +0.000 / -0.000     |
| test2-esbuild | 99.626 / 71.04 / 92.206  | 99.647 / 71.04 / 92.221  | 99.626 / 71.04 / 92.206  | 99.647 / 71.04 / 92.221  | +0.021 / +0.015 | +0.000 / +0.000     | +0.021 / +0.015     |
| test2-rollup  | 96.352 / 75.65 / 91.353  | 96.356 / 75.65 / 91.356  | 96.352 / 75.65 / 91.353  | 96.356 / 75.65 / 91.356  | +0.004 / +0.003 | +0.000 / +0.000     | +0.004 / +0.003     |
| test2-webpack | 99.274 / 70.09 / 91.641  | 99.281 / 70.07 / 91.641  | 99.274 / 70.09 / 91.641  | 99.281 / 70.07 / 91.641  | +0.007 / -0.000 | +0.000 / +0.000     | +0.007 / -0.000     |

The helper key's F0.5 losses, exactly (F0.5 points, 0..1 scale):

| set           | F0.5 change | precision | right bytes lost | wrong bytes |
| ------------- | ----------: | --------- | ---------------: | ----------: |
| heldout       |   -0.000001 | unchanged |            927 B |   unchanged |
| test-bun      |   -0.000001 | unchanged |            618 B |   unchanged |
| test2-bun     |   -0.000001 | unchanged |            309 B |   unchanged |
| test-webpack  |   -0.000006 | +0.00007  |         24,376 B |    -7,626 B |
| test2-webpack |   -0.000004 | +0.00007  |         25,480 B |    -8,556 B |

The webpack rows are the right webpack seeds exp129 already reported, accepted at -0.001.

## 2. Why remin-terser loses precision (dev material; remin is diagnosable)

The table below sorts the 339 KB of new wrong claims on remin-terser (both against base) with `experiments/133-dl25k-r5/scripts/judge13.py`.
That is 321 KB net once the 18 KB the pair fixes is taken off.

| bundle                                                     | new wrong | what it is                                                                                                                                                                                                                                                                                                                      |
| ---------------------------------------------------------- | --------: | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| electron vite-electron-esbuild-starter (bundles vite)      |    275 KB | 199 KB claimed as `@vitejs/plugin-basic-ssl`: vite's inlined certificate chunk (node-forge), which that plugin later copied out of vite; truth credits the shipper vite. Also 43 KB rollup-in-vite and 19 KB `@vue/compiler-core`, 9 KB prettier. In base this bundle named 354 KB; it now names 1.27 MB (1.21 MB of it right). |
| postcat                                                    |     18 KB | 16 KB of `liquid-json` claimed as `jju`                                                                                                                                                                                                                                                                                         |
| bili, rxjs-report-usage, x-dispatch                        |     24 KB | 4.7 KB each of `@babel/parser` claimed as `prettier`, which carries a copy of it, plus `import-meta-resolve` forks for `@babel/core`                                                                                                                                                                                            |
| others (franz `core-js` → `@nextcloud/event-bus`, sqlite3) |    ~20 KB | carriers and forks                                                                                                                                                                                                                                                                                                              |

The whole loss is exp133/134's carrier class: a package that carries a copy of the code is named instead of the
package truth credits. It becomes reachable once factory bodies meet the terser slices. Even without the vite
bundle, remin-terser P would be 99.87, still below the base on the same bundles (99.91).

Two options that are not built:

- an owner rule for code that is held whole by both the bundle's shipper and a carrier (exp134 shows that
  per-unit ownership in general breaks more than it fixes);
- a stricter `terser_share`. It would not help here: the vite bundle is truly terser-minified.

## 3. Reproduce

```bash
S=experiments/135-noprint-census/scripts/score-all.sh; IX=/work/exp135-data/full/ix/k2-all-r5-origin-prod
$S exp138-both   $IX /work/flip-ht-data/both   --pipeline-set library.discovery.helper_prints=seed \
  --pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2
$S exp138-helper $IX /work/flip-ht-data/helper --pipeline-set library.discovery.helper_prints=seed
$S exp138-terser $IX /work/flip-ht-data/terser --pipeline-set library.lookup.terser_slices=auto \
  --pipeline-set library.lookup.terser_share=0.2
python3 -I experiments/133-dl25k-r5/scripts/table.py exp135-prod exp138-helper exp138-terser exp138-both
/work/exp095-library-fingerprints/scale/.venv/bin/python -I experiments/133-dl25k-r5/scripts/judge13.py \
  remin /tmp/judge-remin.jsonl remin-exp135-prod remin-exp138-both   # output names packages: keep it outside the repo
```

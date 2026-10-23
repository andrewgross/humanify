# exp141: an anchor for terser-slice claims

## STATUS (2026-10-23)

**JUDGED ON remin2 (once, summaries only, label `remin2-exp141-x`, commit `b3c9e0876`): FAILS.** remin2-terser P
main 99.712 -> candidate 99.562 (exp139 combo 99.518), F0.5 50.18 -> 78.28; statement scope P 98.148 -> 97.110
(40.5 -> 156.0 KB wrong), factory scope 99.937 -> 99.908; app claimed 0; remin2-esbuild byte-identical (P 99.499).
The anchor rule fixed remin3's statement-scope copies but not remin2's: two dev corpora did not predict the judge.
Do not flip; do not re-judge a variant of this rule on remin2.

DONE on branch `exp141-terser-statements` (off main `673c72cc0`, exp139's branch merged in). One new default-off key,
`[library.discovery] terser_anchor = off | statements | all`, and one new DEV corpus, `remin3`. **Not merged. No
default flipped. remin2 / test / test2 were not scored** (the coordinator decides on spending one remin2 look).

- **The dev corpus could not show the problem, so a second one was built.** On `remin` (36 apps), terser auto +
  exp139's keys add only 6.4 KB of wrong bytes in the STATEMENT scope (and 1.5 MB of right ones): there was nothing
  to diagnose. `remin3` is exp126's recipe at seed 141 with 24 apps per kind (72 apps, 144 bundles, none of any other
  corpus's apps, remin2's included), every scorer-14 layer built as exp140 built remin2's. It reproduces remin2's
  failure shape: on `remin3-terser`, terser auto + exp139's keys take precision from **99.855 to 99.670**
  (statement scope 99.535 -> 98.994, +104 KB wrong; factory 99.924 -> 99.773).
- **Census (section 1): the new wrong bytes are copies, not look-alikes.** Of 284 KB new wrong on remin3-terser: a
  package's code shipped inside the truth's package (monaco-editor's marked / dompurify / vscode-uri, @babel/core's
  import-meta-resolve, mermaid's js-yaml) 109 KB; a fork published under another name 66 KB; a carrier's pre-bundled
  dist claiming its origin's code (@bramus/specificity / css-tree, prettier / graphql) 60 KB; an app's own verbatim
  copy of a library (escope) 25 KB; other 25 KB. No error class is "small generic units terser made collide".
- **The separating feature (section 2): is the family ANCHORED?** A terser-made seed (the claimer scores higher over
  all slices read than over the chosen slice alone) is 99.97% right on remin3 when the bundle shows its family by a
  chosen-slice seed no other family scores (or a declaration), and 93% right in the statement scope / 98% in the
  factory scope when it does not. Ten rules simulated (both corpora, both halves); the anchor is the only one that
  lifts precision over main's on both corpora without a threshold.
- **The key**: `terser_anchor = "all"`: in a bundle that reads the terser slices, a terser-made index seed stands
  only when its family is anchored: a declared or banner seed, or an index seed the chosen slice makes on its own on a
  unit no other family scores. `statements` applies it to top-level and lazy-module statements only. No threshold.
- **Result (section 4; scorer 14, k2-all-r5-origin-prod, both scopes; candidate = terser auto + exp139's three keys +
  `terser_anchor = all`, built at `b3c9e0876`)**:
  - remin3-terser: **P 99.855 -> 99.904 (+0.050)**, F0.5 51.24 -> **80.81 (+29.57)** (terser + keys alone: 99.670 /
    83.32); wrong 48.4 -> 83.9 KB on 2.6x the named bytes. Statement scope 99.535 -> 99.549, factory 99.924 -> 99.956.
  - remin-terser: **P 99.902 -> 99.978 (+0.077)**, F0.5 56.04 -> **83.19 (+27.15)** (terser + keys: 84.97).
  - remin-esbuild / remin3-esbuild: P >= main, F0.5 +0.044 / +0.164.
  - every other dev set (dev, apps, bundlers, pop): **identical cards** to terser + keys and to main (285/285).
- **Bar: precision and F0.5 pass on every dev set. The app bar fails on remin3-terser**: app claimed 0.1 -> 25.5 KB,
  one Electron app's verbatim copy of `escope` in its own source tree. Main's default already claims the identical
  copy (25.1 KB) in that app's esbuild build (remin3-esbuild app 26.4 KB on main), so remin3 has no app-0 baseline on
  either set. Andrew's call whether that is a matcher error.
- **Halves (apps split by sha1 of the slug; the rule has no threshold, so this checks variance, not tuning)**: 3 of 4
  halves at or above main's precision. remin3 half 1 reads 99.887 vs main's 99.914: the same escope app copy (23.4 KB)
  is that half's whole excess; without it the half passes.
- **For the judge**: `terser_anchor = all` together with exp139's keys. Without exp139's keys the anchor fails
  remin-terser (99.876, vite's plugin carrier is anchored), so they stay. The exact command is in section 6.
- **Gate: 13/13 bare** (`npm run check`, tree at `b3c9e0876` + this README / data / scripts). **Warm neutrality** of
  `b3c9e0876` (keys off) against main `673c72cc0`, pair 2.1.215 -> 2.1.216, under `flock /work/neutrality.lock`, from a
  clean detached worktree: default **NEUTRAL 0 files / 0 lines, cache +0 / +0, exit 0 / 0**; report mode **NEUTRAL
  0 / 0, +0 / +0, exit 0 / 0**, `library-report.json` identical.

## 0. Why a second dev corpus

exp140's judge split remin2-terser's failure by scope: the statement scope went 98.148 -> 97.346 (+131 KB wrong) and
exp139's keys did nothing there. On remin, the only dev corpus whose bundles read the terser slices, the statement
scope has no such failure (judge13.py of `remin-exp138-helper` vs `remin-exp139-both`, remin-terser):

| statement scope, remin-terser |      KB |
| ----------------------------- | ------: |
| new claims, right             | 1,527.6 |
| new claims, `origin`          |    71.7 |
| new claims, wrong family      |     6.4 |
| right -> unclaimed            |    10.9 |

So `remin3` was built (`dev_remin.py --corpus remin3`, seed 141, 24 apps per kind, pool 1,304 after removing every
app of every other corpus manifest, remin2's included; `selection.json`). All 72 apps built (no replacement). Its
statement scope holds 21.4 MB of library code (remin: 7.4 MB). Every scorer-14 layer was built the way exp140 built
remin2's, the claim-blind ones (copies2, misattr13, mapdefect14) from `remin3-exp141-main`'s units. It is a DEV
corpus: diagnosed freely, cards committed.

| label (remin3-terser)                            |      P |     R |   F0.5 | wrong + app KB | statement P | factory P |
| ------------------------------------------------ | -----: | ----: | -----: | -------------: | ----------: | --------: |
| main (`remin3-exp141-main`)                      | 99.855 | 17.38 | 51.236 |           48.4 |      99.535 |    99.924 |
| terser auto (`remin3-exp141-terser`)             | 99.621 | 50.34 | 83.309 |          366.9 |      98.986 |    99.718 |
| terser auto + exp139 keys (`remin3-exp141-both`) | 99.670 | 50.30 | 83.317 |          319.0 |      98.994 |    99.773 |

## 1. Census: what the terser slices claim wrongly

`scripts/census141.py` over judge13.py rows (main vs variant), classes from `data/classes.json` (each (claimed,
truth) pair of 1 KB or more read by hand from the claims, the truth and the index rows). KB of NEW wrong bytes
(wrong family or app code claimed; `data/census-*.json`):

| class                 | what it is                                                                                                                                                                                                  |     remin3 terser+keys | remin3 + anchor | remin terser+keys | remin + anchor |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------: | --------------: | ----------------: | -------------: |
| vendored-in-truth     | the truth's package ships a copy of the claimer's code (monaco-editor's marked 47 KB, dompurify, vscode-uri; @babel/core's import-meta-resolve x3 bundles; mermaid's js-yaml claimed as i18next-fs-backend) | 108.6 (statement 79.4) |            10.6 |              13.8 |              0 |
| fork                  | the same code under another name (yargs for @curlconverter/yargs 58 KB; @apollographql/graphql-upload-8-fork; react-is for @jest/react-is-\*; jju for liquid-json)                                          |                   65.7 |             8.0 |              15.8 |              0 |
| carrier-claims-origin | the claimer's pre-bundled dist carries the truth's code (@bramus/specificity for css-tree 52 KB, prettier for graphql 17 KB, chalk's vendored ansi-styles)                                                  |                   59.8 |             1.1 |               0.7 |            0.7 |
| app-copy              | the app's own source holds a verbatim library copy (escope)                                                                                                                                                 |                   25.4 |            25.4 |                 0 |              0 |
| other                 | propagation inside a shown family, sub-KB residue                                                                                                                                                           |                   24.8 |            20.6 |               9.2 |            8.3 |
| **total**             |                                                                                                                                                                                                             |              **284.3** |        **65.6** |          **39.5** |        **9.0** |

Every class is a COPY: the claimed package's index really holds that code. What the terser slices add is reach: the
remin recipe bundles the packages' published minified entries, which are exactly what the terser-built slices print,
so a copy a package ships in its minified dist is now visible. Match strength does not separate right from wrong
(remin3-terser, terser-made seeds, both scopes; `scripts/buckets141.py`, `data/buckets-*.txt`):

| feature of a terser-made seed              | right KB | wrong KB | precision |
| ------------------------------------------ | -------: | -------: | --------: |
| whole-unit hit                             |   35,177 |     63.2 |     99.82 |
| function prints only                       |   29,250 |    129.3 |     99.56 |
| one package holds the prints (any context) |   39,584 |     66.9 |     99.83 |
| two or more hold them                      |   24,843 |    125.6 |     99.50 |
| another family scores the unit             |   14,595 |    103.3 |     99.30 |
| statement scope                            |    7,514 |     65.5 |     99.14 |
| factory scope                              |   56,913 |    127.0 |     99.78 |

By unit size (nodes) the wrong bytes sit in a few large units (the 2^12-node bucket: 96.5 KB, marked's 48 KB body and
the yargs fork's 33 KB file); sub-500-node units are 99.8-99.96% right. The wrong bytes are a handful of families,
and each is wrong in ALL of its claims in its bundle: a per-family rule, not a per-unit one.

## 2. What separates right from wrong: the anchor

Per bundle, a family is ANCHORED when it has a declared or banner seed, or an index seed the chosen slice makes on
its own on a unit no other family scores. Terser-made seed bytes by anchor (`data/anchor-seeds-*.json`):

| scope     | anchor                          | remin3 right KB |  remin3 wrong KB | remin right KB | remin wrong KB |
| --------- | ------------------------------- | --------------: | ---------------: | -------------: | -------------: |
| statement | anchored (exclusive)            |         6,592.3 |              0.3 |        3,788.6 |              0 |
| statement | only a shared chosen-slice seed |            29.7 |                0 |           50.3 |              0 |
| statement | none                            |           892.3 |             65.3 |          175.6 |              0 |
| factory   | anchored (exclusive)            |        50,915.6 | 16.9 (+25.0 app) |       26,285.2 |            3.7 |
| factory   | only a shared chosen-slice seed |           781.8 |             54.1 |          139.0 |            1.5 |
| factory   | none                            |         5,215.5 |             56.0 |        2,351.5 |           17.3 |

The mechanism: the terser slices are a second reading of code the bundle's own slice already places. A package the
app really bundles leaves at least one unit the chosen slice recognises as that package's alone (its entry, its own
statements). A copy's claimer has no such unit: everything it is credited with is the copied code, which is also why
a chosen-slice seed another family scores (bramus's 2.4 KB of css-tree, prettier's 0.1 KB) must not count as an
anchor.

## 3. Simulations (seed level, before any crates change)

`scripts/sim141.py` drops terser-made seeds by rule (a family that loses a share of its seed bytes in a bundle loses
that share of its propagated bytes) and re-computes the set from the label's cards, per half (apps split by sha1 of
the slug) and overall. Calibration: for `anchor` (shared seeds count) it predicted remin3 99.798 / 81.21 and remin
99.952 / 83.34; the lab binary measured 99.797 / 81.17 and 99.952 / 83.339. Bars: remin3 main 99.855 (halves 99.801 /
99.914), remin main 99.902 (99.955 / 99.828). Base = terser auto + exp139 keys. Precision / F0.5 overall
(`data/sim-rules*.txt`, `data/sim-few-units.txt`):

| rule (drops terser-made seeds ...)                                                    | thresholds | remin3 -R / -W KB |    remin3 P / F0.5 | remin -R / -W KB |     remin P / F0.5 | passes                                             |
| ------------------------------------------------------------------------------------- | ---------- | ----------------: | -----------------: | ---------------: | -----------------: | -------------------------------------------------- |
| **anchor_excl**: family not anchored by an exclusive chosen-slice seed or declaration | none       |       8,363 / 205 | **99.870 / 80.88** |       3,037 / 20 | **99.956 / 83.21** | both; halves 3/4                                   |
| anchor: as above, a shared chosen-slice seed counts                                   | none       |       7,205 / 138 |     99.798 / 81.21 |       2,816 / 19 |     99.952 / 83.34 | remin only                                         |
| anchor_stmt / anchor_excl_stmt: statement scope only                                  | none       |        1,182 / 79 |     99.749 / 83.02 |          216 / 1 |     99.918 / 84.85 | remin only (statement scope passes both)           |
| anchor_contested: unanchored and another package holds the prints                     | none       |       3,037 / 121 |     99.788 / 82.49 |        1,129 / 2 |     99.920 / 84.33 | remin only                                         |
| hidden_host: unanchored, an anchored family holds the prints beyond the context limit | none       |          545 / 67 |     99.737 / 83.20 |          258 / 0 |     99.916 / 84.82 | remin only                                         |
| stmt_off: every statement-scope one                                                   | none       |      10,831 / 105 |     99.750 / 80.00 |        5,256 / 4 |     99.916 / 81.79 | remin only                                         |
| fn_only: no whole-unit hit                                                            | none       |      34,138 / 160 |     99.745 / 70.54 |       17,060 / 8 |     99.898 / 72.39 | neither                                            |
| gained_share: family more than half terser-made                                       | 0.5        |      62,810 / 253 |     99.804 / 51.46 |      30,251 / 34 |     99.963 / 55.51 | remin only                                         |
| few_units: family under N index seeds in the bundle                                   | N = 16     |      13,313 / 201 |     99.858 / 79.21 |       6,247 / 31 |     99.978 / 81.16 | both, margin 0.003; half 1 fails at every N 12..64 |

- few_units ("a dependency brings its module set, a copy a fragment") is the only other rule that reaches main's
  precision on remin3, at N >= 16 with a 0.003-point margin, and it fails remin3's half 1 at every N tried (12, 16,
  20, 24, 32, 48, 64), costs 2-4x the anchor's recall, and cannot tell the yargs fork from yargs (same module set).
- The statement-scope-only anchor fixes the statement scope on both corpora (remin3 statement P 98.994 -> 99.560 in
  the lab; main 99.535) but leaves remin3's factory-scope copies, so remin3-terser stays at 99.748.

## 4. The key and the lab ladder

`[library.discovery] terser_anchor = "off" | "statements" | "all"` (default off), `DiscoveryOptions::terser_anchor`
(`TerserAnchor`). It runs inside exp139's `terser_presence`, so only in a bundle that reads the terser slices; it
uses exp139's per-seed slice scores (`gained`: the family scores higher over every slice read than over the chosen
slice). Verdicts after the carrier / majority / thin ones; a dropped seed is claimed by no one (main's presence fence
holds it). Tests, watched red (missing field, then failing asserts, then the exclusive case) before green:
`terser_anchor_drops_terser_made_statement_seeds_of_a_family_the_chosen_slice_never_sees`,
`terser_anchor_statements_leaves_factory_seeds_and_all_reaches_them`,
`the_exp141_terser_anchor_key_defaults_off_and_sets_from_the_file`.

Lab ladder (prebuilt binaries, lab labels not committed), terser auto + exp139 keys + :

| setting                                          |  remin3-terser P / R / F0.5 | statement P | factory P |   remin-terser P / R / F0.5 |
| ------------------------------------------------ | --------------------------: | ----------: | --------: | --------------------------: |
| nothing (`*-both`)                               |     99.670 / 50.30 / 83.317 |      98.994 |    99.773 |     99.917 / 53.16 / 84.968 |
| anchor statements, shared seeds count            |     99.748 / 49.59 / 82.966 |      99.560 |    99.773 |     99.918 / 52.91 / 84.842 |
| anchor all, shared seeds count                   |     99.797 / 46.48 / 81.174 |      99.560 |    99.831 |     99.952 / 50.06 / 83.339 |
| anchor statements, exclusive                     |     99.747 / 49.44 / 82.879 |      99.549 |    99.773 |     99.918 / 52.84 / 84.807 |
| **anchor all, exclusive (the key as committed)** | **99.904 / 45.79 / 80.805** |      99.549 |    99.956 | **99.978 / 49.76 / 83.189** |
| anchor all, exclusive, WITHOUT exp139's keys     |     99.903 / 45.83 / 80.829 |      99.550 |    99.954 |     99.876 / 50.16 / 83.351 |
| main (no terser)                                 |     99.855 / 17.38 / 51.236 |      99.535 |    99.924 |     99.902 / 20.33 / 56.043 |

## 5. Results (scorer 14, structural rule, both scopes, k2-all-r5-origin-prod, every DEV corpus)

Labels: main's defaults = `*exp138-helper*` (remin3: `remin3-exp141-main`, at `673c72cc0`); terser auto + exp139 keys
= `*exp139-both*` (remin3: `remin3-exp141-both`, at `56a6f474f`, exp139's crates); candidate = `*exp141-x*` (at
`b3c9e0876`: `terser_slices=auto`, `terser_share=0.2`, `terser_carriers=0.5`, `terser_factory_majority=1`,
`terser_payload_literal=true`, `terser_anchor=all`). Bar: P >= main, F0.5 >= main - 0.002, app claimed <= main's.
(`scripts/table141.py`, `data/table.json`.) test, test2 and remin2 were not scored.

| set            | main defaults P / R / F0.5 | terser auto + exp139 keys |    + terser_anchor = all | d terser+keys P / F0.5 | d candidate P / F0.5 | wrong KB main / keys / cand | bar                                                                |
| -------------- | -------------------------: | ------------------------: | -----------------------: | ---------------------: | -------------------: | --------------------------: | ------------------------------------------------------------------ |
| cc             |   100.000 / 81.90 / 95.768 |  100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 |        +0.000 / +0.000 |      +0.000 / +0.000 |             0.0 / 0.0 / 0.0 | PASS                                                               |
| esbuild-app    |    99.078 / 74.74 / 93.020 |   99.078 / 74.74 / 93.020 |  99.078 / 74.74 / 93.020 |        +0.000 / +0.000 |      +0.000 / +0.000 |       126.3 / 126.3 / 126.3 | PASS                                                               |
| heldout        |    99.636 / 68.56 / 91.354 |   99.636 / 68.56 / 91.354 |  99.636 / 68.56 / 91.354 |        +0.000 / +0.000 |      +0.000 / +0.000 |       575.7 / 575.7 / 575.7 | PASS                                                               |
| wrangler       |    99.069 / 33.14 / 70.870 |   99.069 / 33.14 / 70.870 |  99.069 / 33.14 / 70.870 |        +0.000 / +0.000 |      +0.000 / +0.000 |             5.6 / 5.6 / 5.6 | PASS                                                               |
| apps-esbuild   |    99.908 / 77.65 / 94.490 |   99.908 / 77.65 / 94.490 |  99.908 / 77.65 / 94.490 |        +0.000 / +0.000 |      +0.000 / +0.000 |       107.3 / 107.3 / 107.3 | PASS                                                               |
| apps-rollup    |    99.112 / 79.31 / 94.398 |   99.112 / 79.31 / 94.398 |  99.112 / 79.31 / 94.398 |        +0.000 / +0.000 |      +0.000 / +0.000 |       678.8 / 678.8 / 678.8 | PASS                                                               |
| apps-webpack   |    99.599 / 73.50 / 92.994 |   99.599 / 73.50 / 92.994 |  99.599 / 73.50 / 92.994 |        +0.000 / +0.000 |      +0.000 / +0.000 |       215.5 / 215.5 / 215.5 | PASS                                                               |
| dev-rollup     |    99.873 / 77.53 / 94.430 |   99.873 / 77.53 / 94.430 |  99.873 / 77.53 / 94.430 |        +0.000 / +0.000 |      +0.000 / +0.000 |             2.1 / 2.1 / 2.1 | PASS                                                               |
| dev-webpack    |    99.642 / 62.89 / 89.216 |   99.642 / 62.89 / 89.216 |  99.642 / 62.89 / 89.216 |        +0.000 / +0.000 |      +0.000 / +0.000 |             4.0 / 4.0 / 4.0 | PASS                                                               |
| pop-bun        |    99.518 / 77.21 / 94.082 |   99.518 / 77.21 / 94.082 |  99.518 / 77.21 / 94.082 |        +0.000 / +0.000 |      +0.000 / +0.000 |    1189.9 / 1189.9 / 1189.9 | PASS                                                               |
| pop-esbuild    |    99.315 / 78.71 / 94.373 |   99.315 / 78.71 / 94.373 |  99.315 / 78.71 / 94.373 |        +0.000 / +0.000 |      +0.000 / +0.000 |    1730.6 / 1730.6 / 1730.6 | PASS                                                               |
| remin-esbuild  |    99.719 / 51.76 / 84.129 |   99.720 / 51.84 / 84.173 |  99.720 / 51.84 / 84.173 |        +0.000 / +0.044 |      +0.000 / +0.044 |       141.8 / 141.8 / 141.8 | PASS                                                               |
| remin-terser   |    99.902 / 20.33 / 56.043 |   99.917 / 53.16 / 84.968 |  99.978 / 49.76 / 83.189 |       +0.015 / +28.925 |     +0.077 / +27.146 |           18.2 / 40.3 / 9.8 | PASS                                                               |
| remin3-esbuild |    99.679 / 49.48 / 82.863 |   99.680 / 49.78 / 83.034 |  99.680 / 49.77 / 83.028 |        +0.001 / +0.171 |      +0.001 / +0.164 |       322.1 / 323.2 / 323.2 | PASS                                                               |
| remin3-terser  |    99.855 / 17.38 / 51.236 |   99.670 / 50.30 / 83.317 |  99.904 / 45.79 / 80.805 |       -0.185 / +32.081 |     +0.050 / +29.569 |         48.4 / 319.0 / 83.9 | P, F0.5 pass; **app FAIL** (0.1 -> 25.5 KB, one app's escope copy) |

Per-bundle cards: the candidate's are identical to terser + keys on every card of dev (70), apps (97), bundlers (27)
and pop (91); remin 38/72 and remin3 68/144 identical (the bundles that read the terser slices change). Halves of the
terser sets (candidate vs main precision): remin3 99.919 vs 99.801 and 99.887 vs 99.914; remin 99.980 vs 99.955 and
99.977 vs 99.828.

The candidate's remaining new wrong bytes on remin3-terser (65.6 KB, section 1): the escope app copy 25.4 KB,
dompurify's propagation inside monaco-editor 10.3 KB (its 259-byte seed is anchored by a unit of its own), the
graphql-upload and react-is forks 8 KB, propagation inside mermaid's shown family 5.4 KB, sub-KB residue.

## 6. For the judge (not run here)

One remin2 look would answer whether the anchor holds on unseen apps:

```bash
npm run eval -- library remin2-exp141-x --index /work/exp135-data/full/ix/k2-all-r5-origin-prod --rule structural \
  --corpus remin2 --commit b3c9e0876 \
  --pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2 \
  --pipeline-set library.discovery.terser_carriers=0.5 --pipeline-set library.discovery.terser_factory_majority=1 \
  --pipeline-set library.discovery.terser_payload_literal=true --pipeline-set library.discovery.terser_anchor=all
```

Compare with `remin2-exp140-main` (P 99.712 / F0.5 50.18; statement 98.148, factory 99.937). What the dev sets
predict: precision above main's on both remin corpora by +0.05..0.08 points with ~92-94% of the terser F0.5 gain; the
risk is a judge app whose own copy of a library (the escope class) or a fork with an exclusive unit lands in it.

## 7. Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; S=experiments/126-reminified/scripts; X=experiments/141-terser-statements/scripts
python3 -I $S/dev_remin.py plan --corpus remin3 && python3 -I $S/dev_remin.py build --jobs 12 --corpus remin3
#  freeze + every truth layer: exp140's list with --corpus remin3 (/work/exp141-data/layers.sh, layers2.sh)
C="--index /work/exp135-data/full/ix/k2-all-r5-origin-prod --rule structural --corpus remin3"
T="--pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2"
K="--pipeline-set library.discovery.terser_carriers=0.5 --pipeline-set library.discovery.terser_factory_majority=1 --pipeline-set library.discovery.terser_payload_literal=true"
npm run eval -- library remin3-exp141-main $C --commit 673c72cc0
npm run eval -- library remin3-exp141-both $C --commit 56a6f474f $T $K
npm run eval -- library remin3-exp141-terser $C --commit 56a6f474f $T
J=/work/exp141-data/judge   # per-claim rows name packages: outside the repo
$PY -I experiments/133-dl25k-r5/scripts/judge13.py remin3 $J/remin3-main-both.jsonl remin3-exp141-main remin3-exp141-both
$PY -I experiments/139-carriers/scripts/carriers.py remin3 $J/remin3-main-both.jsonl c1 remin3-exp141-both /work/exp135-data/full/registry $J/remin3-both-seeds.jsonl
$PY -I $X/holders.py remin3 remin3-exp141-both $J/remin3-both-seeds.jsonl $J/remin3-holders.jsonl
$PY -I $X/sim141.py remin3 remin3-exp141-both $J/remin3-main-both.jsonl $J/remin3-holders.jsonl $J/remin3-both-seeds.jsonl [--scope statement|factory] [--rules ...] [--param units=N]
python3 -I $X/census141.py $J/remin3-main-both.jsonl --json experiments/141-terser-statements/data/census-remin3-main-both.json
#  (the same for remin: remin-exp138-helper / remin-exp139-both)
/work/exp141-data/score-dev.sh    # the candidate on dev, apps, bundlers, pop, remin, remin3 at b3c9e0876
python3 -I $X/table141.py --json experiments/141-terser-statements/data/table.json --identical
```

## Gate and neutrality

| check                                                                                           | result                                                                                                                                                                  |
| ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `npm run check` (bare, exit 0)                                                                  | ALL 13 STAGES PASSED (typecheck, lint, rust:fmt, rust:clippy, knip, knip:prod, census:clones, census:concurrency, unit, rust:unit, rust:build, rust:format-golden, e2e) |
| warm neutrality, default mode, `b3c9e0876` vs `673c72cc0`, 2.1.215 -> 2.1.216                   | NEUTRAL: 0 files / 0 lines, cache +0 / +0 (baseline 0), exit 0 / 0                                                                                                      |
| warm neutrality, report mode (`library.matching=report`, `library.index=k2-all-r5-origin-prod`) | NEUTRAL: 0 / 0, +0 / +0, exit 0 / 0; `library-report.json` byte-identical                                                                                               |
| dev sets without terser reads (dev, apps, bundlers, pop)                                        | 285/285 cards identical to terser + keys (and to main's defaults)                                                                                                       |
| remin2 / test / test2                                                                           | not scored (judges)                                                                                                                                                     |

# exp148: carrier coverage — why the index leaves out the packages whose copies main mis-names

## STATUS (2026-10-24)

DONE on branch `exp148-carrier-coverage` (off main `d84196a24`). Harness scripts, one delta index, scored dev
labels. **No crates change, no default changed, nothing merged.** Dev corpora only; test / test2 / remin2 were not
scored, diagnosed or opened.

- **Census (§1): coverage is 37.5% of main's wrong bytes, not most of it.** Of 6.91 MB wrong-family bytes over 16 dev
  sets: the truth package is not indexed at all **1.00 MB (14.5%)**; indexed but not at the bundled version
  **1.59 MB (23.0%)**; indexed at that version and matching still went wrong **3.81 MB (55.1%)**; the truth row
  names no version 0.51 MB (7.3%). Every a/b package but one is public on npm.
- **Why the selection rule left them out (§2)**: prereleases are never indexed 754 KB (all vitepress 2 alpha,
  which also fails to build: `.vue`), bundled by only one training app 706 KB (the >= 2 cut), older 0.x lines
  past the 4-line cap 595 KB (monaco-editor), selected but the build produced no rows 186 KB (tsx: esbuild syntax
  error on every version), older majors / the 12-a-year cap 240 KB, bundled by no training app 107 KB. Four
  carriers (vitepress, monaco-editor, @putout/minify, tsx) are 1.79 MB of the 2.60 MB.
- **Proposed generic rule (§3), built and measured (§4): REFUTED.** "From the npm download top-25k, add every
  package whose build holds a verbatim copied file of indexed code, unless that code belongs to one of its declared
  dependencies or a same-scope sibling": 269 packages, 595 versions (+0.9% of pins), +0.2 GB, 2.7 h to build
  (~0.8 h without one pathological package). It fixes the exp147 case exactly (agentmail: 76.8 KB wrong -> 0,
  +1.30 MB named) but **fails the bar on 15 of 17 dev sets with main's defaults and with exp144's `shown`**:
  the carriers make the ORIGINALS ambiguous wherever the original is bundled, and the refusals cost far more
  recall than the fixed claims (wrangler F0.5 -9.99, remin-terser P -0.168, esbuild-app P -0.096 / F0.5 -0.49,
  held-out F0.5 -0.08, pop F0.5 -0.11 / -0.10 with P +0.011 / +0.013).
- **Version-policy rules (§3) are not cheap enough**: five years of older lines = 14,336 versions (~9x exp142's
  delta) for <= 318 KB of reach; prereleases = 635 versions whose whole reach is one package that cannot be built.
- **What would have to come first**: a matching rule that resolves a copy tie toward the package whose OTHER files
  the bundle also holds (the shipper is present when its non-copied files are), instead of refusing; with that,
  indexing carriers stops costing the originals. exp133/134/139 tried per-unit versions of this and refuted them;
  a bundle-level "which co-holder is otherwise present" vote is the untried variant. Separately, two builder
  leads are cheap and generic: esbuild loaders for non-JS assets (`.vue`, `.node`, `.woff2`, `.png`, ...: 1,379
  pinned versions of 448 packages own no rows, 409 more fail on syntax) — vitepress and tsx are among them.
- **Judge commands (NOT run; not recommended — the dev bar fails on 15 of 17 sets):**
  `npm run eval -- library {test,test2,remin2}-exp148-car --index /work/exp148-data/full/ix/k2-all-r5-origin-prod-asof20261015-carriers --rule structural --corpus {test,test2,remin2} --commit 673c72cc0`
  and for `shown`: `npm run eval -- library {test,test2,remin2}-exp148-car-shown --index /work/exp148-data/full/ix/k2-all-r5-origin-prod-asof20261015-carriers --rule structural --corpus {test,test2,remin2} --commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown`
  (bases `*-exp142-asof` and `*-exp144-shown2`).

## 1. Census: main's wrong-family bytes on the recommended index, by cause

`scripts/census.py` reads every library unit scorer 14 judges `wrongFamily` (both scopes) under main's defaults on
the recommended index `k2-all-r5-origin-prod-asof20261015` (labels `*exp142-asof*`, crates = main's), with its truth
row (shipper package T, version V). Totals reproduce every card's `wrongFamilyBytes` exactly. Classes: **a** T is in
no version of the index; **b** T is indexed, V is not; **c** T@V is indexed and matching still went wrong; **n** the
truth row has no version. Held-out appears as aggregates only. Judges were not read.

| set            | wrong KB | a: not indexed | b: indexed, not this version | c: version indexed | n: truth has no version |
| -------------- | -------: | -------------: | ---------------------------: | -----------------: | ----------------------: |
| heldout        |    568.4 |    112.7 (20%) |                    29.4 (5%) |        426.3 (75%) |                0.0 (0%) |
| esbuild-app    |    122.5 |      11.0 (9%) |                     3.1 (2%) |         77.1 (63%) |              31.3 (26%) |
| wrangler       |      5.6 |       0.0 (0%) |                     0.0 (0%) |         5.6 (100%) |                0.0 (0%) |
| apps-esbuild   |    107.3 |     21.8 (20%) |                     0.1 (0%) |         85.1 (79%) |                0.4 (0%) |
| apps-rollup    |    678.8 |      61.3 (9%) |                    35.3 (5%) |        582.2 (86%) |                0.0 (0%) |
| apps-webpack   |    215.5 |      15.8 (7%) |                     4.3 (2%) |        155.0 (72%) |              40.5 (19%) |
| dev-rollup     |      2.1 |       0.0 (0%) |                     0.0 (0%) |         2.1 (100%) |                0.0 (0%) |
| dev-webpack    |      4.0 |       0.0 (0%) |                     0.0 (0%) |         4.0 (100%) |                0.0 (0%) |
| pop-bun        |   1214.7 |    252.2 (21%) |                  501.6 (41%) |        460.9 (38%) |                0.0 (0%) |
| pop-esbuild    |   1753.9 |    360.3 (21%) |                  377.4 (22%) |        708.7 (40%) |             307.5 (18%) |
| remin-esbuild  |    141.8 |       4.5 (3%) |                   15.1 (11%) |         88.8 (63%) |              33.5 (24%) |
| remin-terser   |     15.6 |       0.2 (1%) |                    2.1 (14%) |         13.3 (85%) |                0.0 (0%) |
| remin3-esbuild |    297.1 |      21.8 (7%) |                   74.0 (25%) |        148.6 (50%) |              52.7 (18%) |
| remin3-terser  |     51.0 |       0.6 (1%) |                   13.4 (26%) |         36.4 (71%) |                0.6 (1%) |
| remin4-esbuild |   1387.5 |    140.2 (10%) |                  489.5 (35%) |        734.9 (53%) |               23.0 (2%) |
| remin4-terser  |    347.6 |       1.9 (1%) |                   46.3 (13%) |        281.5 (81%) |               17.8 (5%) |
| **all dev**    |   6913.5 |   1004.2 (15%) |                 1591.5 (23%) |       3810.5 (55%) |              507.3 (7%) |

- **The index already holds the truth's version for 55% of main's wrong bytes (3.81 MB of 6.91 MB).** Coverage is
  NOT the main precision loss. 783 KB of class c is an exact tie the truth package lost (both T and the claimed
  package hold the unit's whole print), 1.22 MB an exact copy only the claimed package holds (T's rows for that code
  went to the origin, or T's indexed rows differ), 1.81 MB claims made by function prints, propagation or fill.
- **Coverage (a + b) is 2.60 MB = 37.5%**: a = 1.00 MB (14.5%), b = 1.59 MB (23.0%).
- **Every a/b truth package but one is a public npm package** (`@jest/react-is-19`, 5.4 KB, is a registry 404).
- **a/b is very concentrated.** Four packages are 1.79 MB of the 2.60 MB: `vitepress` 2.0.0-alpha (754 KB, pop
  only), `monaco-editor` 0.24-0.44 (590 KB, remin3/remin4), `@putout/minify` (300 KB, pop), `tsx` (144 KB, pop).
  All four are carriers: their published files hold copies of other packages' code (vitepress's node chunk,
  monaco's vendored marked / dompurify / vscode-languageserver-types, @putout/minify's babel / regexp-tree /
  prettier, tsx's es-module-lexer / get-tsconfig), and main names the copies after the other packages.

## 2. Why the selection rule left them out

`census.py`'s reason for each a/b byte, from the index manifest (pins, refusals), the shards' build notes, the
registry snapshot and the training-app closure counts (`closure_counts.py`: exp108 `select_roots.py`'s count — the
index's package rule is "bundled by >= 2 training apps", exp095's + exp108's, held-out excluded; versions by exp101
`tail_pins.py` `year-last12+major4`: every stable release of the last 12 months, at most 12, plus the newest of at
most 4 older release lines, a line being a major or a 0.minor; no prereleases).

| class: reason                         |    KB | % of all wrong |
| ------------------------------------- | ----: | -------------: |
| b: version-not-chosen:prerelease      | 754.0 |          10.9% |
| a: apps=1                             | 705.6 |          10.2% |
| b: version-not-chosen:older-0.x       | 595.3 |           8.6% |
| a: selected-no-rows:build-error       | 144.0 |           2.1% |
| b: version-not-chosen:older           | 142.5 |           2.1% |
| a: apps=0                             | 107.2 |           1.6% |
| b: version-not-chosen:last-12-months  |  97.0 |           1.4% |
| a: selected-no-rows:built-no-rows     |  42.0 |           0.6% |
| a: not-on-npm                         |   5.4 |           0.1% |
| b: version-pinned-no-rows:build-error |   2.3 |           0.0% |
| b: version-refused:no-shard           |   0.4 |           0.0% |

The reasons, in plain terms:

1. **Prereleases are never indexed (754 KB, 10.9%).** All of it is `vitepress` 2.0.0-alpha.17 / .20 (pop): the
   apps use the next major while it is still an alpha. **And vitepress fails to build at every version the index
   pins** (1.6.4: esbuild "No loader is configured for .vue files", an empty shard), so a prerelease rule alone
   would add nothing for it.
2. **The package is bundled by only ONE training app (706 KB, 10.2%)** — the >= 2 cut. `@putout/minify` 300,
   `temml` 81 (a katex fork), `agentmail` 80 (the generated SDK runtime exp147 found), `@rrweb/record` 32,
   `expr-eval-fork` 27, `@stoplight/yaml-ast-parser` 24. For pop / remin\* that one app is usually the dev app
   itself (they are drawn from the same training pool), so this count is not corpus-blind for those sets.
3. **Older 0.x release lines past the 4-line cap (595 KB, 8.6%).** `monaco-editor` 590: its versions are all 0.x,
   each minor is a line, and only 0.52-0.57 fit; the apps bundle 0.24-0.44.
4. **Selected but the build produced no rows (186 KB, 2.7%).** `tsx` 144: every pinned version's shard is empty
   (esbuild "Expected ';' but found ')'" on its entry). `tslib` / `@babel/runtime` (42 KB) build fine but own no
   module rows (helper libraries; the helper tables cover them). Index-wide, 1,379 pinned versions of 448
   packages own no rows: 647 built clean (types-only / all rows given to origins), 409 esbuild syntax errors, 148
   `.node` / 48 `.woff2` / 19 `.png` / 12 `.sass` / 12 `.wasm` / ... "no loader" errors.
5. **Older majors past the cap (143 KB) / the 12-release cap in the last year (97 KB)**: `npm` 2/5/6,
   `stylelint` 9/13, `vite` 2, `oxfmt` 0.32/0.57 (`oxfmt` cuts a 0.minor per release).
6. **Bundled by no training app (107 KB)**, held-out 81 of it: the corpus-pool limit exp108 measured.

## 3. A generic carrier rule, sized and simulated

**Version rules first (corpus-blind, `policy_sim.py`)**, sized over the index's own 9,897 packages at as-of
2026-10-15, with the class-b bytes each would bring under the index (an upper bound on what it fixes):

| added to year-last12+major4                   | versions added | packages |    class-b KB brought under the index |
| --------------------------------------------- | -------------: | -------: | ------------------------------------: |
| 8 older lines instead of 4                    |          9,768 |    3,255 |                                    45 |
| every older line released in the last 3 years |          8,515 |      775 |                                    87 |
| every older line released in the last 5 years |         14,336 |    1,319 |                                   318 |
| newest prerelease of an unreleased next line  |            635 |      600 | 375 (vitepress, which fails to build) |
| 5 years + prereleases                         |         14,971 |    1,762 |                                   693 |

The line rules cost 6-9x exp142's whole delta (an 8-11 h build) for <= 318 KB of reach, most of it one package
(monaco-editor). The prerelease rule is cheap but its whole reach is vitepress, which the builder cannot build
(`.vue`). Neither was built.

**The carrier rule (the brief's lever).** The pool must not come from the corpora: the training-app count
(class a "apps = 1") is drawn from the same pool as pop / remin / remin3 / remin4 / remin2, so "add packages
bundled by one training app" would index the dev and judge apps' own packages. The corpus-blind pool that exists
is exp133's download top-25k (exp118's 13,785 roots, already fingerprinted in
`/work/exp133-data/full/ix/k2-all-r5-origin-dl25k`). It holds 433 KB of class a's truth packages (agentmail and
temml among them; not @putout/minify or tsx).

`carriers.py` (one streaming pass over dl25k's esbuild unit table) marks a dl25k-only package a **carrier** when it
has at least one **copied file**: >= 3 units of >= 20 nodes, module >= 1 KB, every unit's whole print also owned
by a package of the recommended index. 324 of 11,389 dl25k-only packages are carriers (C1); a looser share rule
(>= 50% of the package's prints held, C2) gives 921.

**Simulation from exp133's two scored labels** (`sim133.py`: per unit, base = k2-all-r5-origin vs dl = + all of
dl25k, scorer 14 verdicts on both labels' stored reports; a changed unit is credited to the dl25k package it now
names, else to its truth package if that is a dl25k package; base-to-base changes are "unattributed" and left out).
New wrong KB (negative = wrong removed), attributable part only:

| set                    | all of dl25k | C1 (324 carriers) | C1d (269: C1 minus copies of a declared dependency / same scope) |
| ---------------------- | -----------: | ----------------: | ---------------------------------------------------------------: |
| heldout                |       +743.7 |              +1.7 |                                                             +1.7 |
| esbuild-app            |       +309.6 |             +21.3 |                                                            +21.3 |
| apps-esbuild           |       +150.6 |            +156.8 |                                                             -7.4 |
| apps-rollup            |       +330.0 |            +358.4 |                                                             -7.6 |
| apps-webpack           |         +7.1 |             +23.1 |                                                            +23.1 |
| pop-bun                |       +473.4 |             -18.5 |                                                            -22.2 |
| pop-esbuild            |       +513.9 |            +136.9 |                                                            -35.5 |
| remin-esbuild / terser |  -9.8 / +0.9 |        +1.5 / 0.0 |                                                       +1.5 / 0.0 |

- **Indexing a carrier is symmetric.** It removes the wrong claim where the carrier's copy is bundled, and STEALS
  where the original is bundled. C1's cost is one carrier: `@mermaid-js/layout-elk` (+695 KB over three sets,
  all of it mermaid's own code that today is named right). It peer-depends on mermaid; dropping carriers whose
  copied code's top origin is a declared dependency (deps / peer / optional, any version) or a same-scope sibling
  removes it (C1d).
- **C1d still steals where a carrier vendors a package it does not declare**: `@finom/zod-to-json-schema` (a scoped
  republish of zod-to-json-schema) +21.3 KB on esbuild-app, `@architect/utils` (vendored chalk) +35 KB on
  apps-webpack. It fixes `agentmail` (-79.5 KB over pop), `@stoplight/yaml-ast-parser` (-36 KB over apps) and
  `ng-packagr` (-13 KB), and names +670 / +682 KB more on pop-bun / pop-esbuild.
- The sim predicts the strict bar FAILS on esbuild-app and apps-webpack, and it cannot see remin3 / remin4 (no
  exp133 labels) or interactions. Built anyway at small cost (595 versions), to measure it on every dev set.

**Proposed rule (C1d), stated generically:** from a corpus-blind pool of candidate packages (the npm download
top-25k), add a package when its own build holds at least one verbatim copied file of code an indexed package
owns, unless the copied code's top owner is one of its declared dependencies or a same-scope sibling (then the
index's sponge / origin crediting is the right owner). Index it at exp118's `year-last3+major1`. **Cost: 269
packages, 595 versions (+0.9% of the index's 64,372 pins); detection is one 2-minute pass over a pool index's
unit table (if the pool is already fingerprinted, as dl25k is; fingerprinting a fresh pool costs ~11 h, exp133).**

## 4. The delta build and its scores

**Build** (`experiments/142-index-coverage/scripts/build-delta.sh`, exp142's recipes unchanged: both Bun slices
`production`, `--sponge-regions origin`; humanify-index at `673c72cc0`, the builder main still has): the 595 C1d
versions (`year-last3+major1` at as-of 2026-10-15, corpus-blind) into a hard-link copy of exp142's store, merged
with every version of the recommended index. **`/work/exp148-data/full/ix/k2-all-r5-origin-prod-asof20261015-carriers`
(7.3 GB, index id `8f8203a0...`)**: 0 versions lost, +1,046 owner-versions (262 of the 269 carriers own rows; the
rest are origins credited by the sponge regions), 563 of 595 pins installed (32 refused: unpublished). Shards
143 min on 30 cores (6 chunks; 3 of them spent ~110 min on one carrier, `openclaw`, whose webpack builds hit the
builder's 1,800 s timeout twice per version, as in exp133), merge 19 min at 38.6 GB.

**Main's defaults** (crates = main's, `--commit 673c72cc0`; scorer 14, structural, both scopes), base
`*exp142-asof*`, candidate `*exp148-car*`. Bar: P >= base, F0.5 >= base - 0.002, app claimed <= base:

| set            |        base P / R / F0.5 |   candidate P / R / F0.5 |      d P / d R / d F0.5 | wrong+app KB base / new | bar  |
| -------------- | -----------------------: | -----------------------: | ----------------------: | ----------------------: | ---- |
| cc             | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | +0.000 / +0.00 / +0.000 |               0.0 / 0.0 | PASS |
| esbuild-app    |  99.115 / 75.60 / 93.310 |  99.019 / 74.23 / 92.821 | -0.096 / -1.36 / -0.488 |           122.5 / 133.5 | FAIL |
| heldout        |  99.641 / 68.64 / 91.385 |  99.641 / 68.42 / 91.308 | +0.000 / -0.22 / -0.076 |           568.4 / 566.3 | FAIL |
| wrangler       |  99.069 / 33.14 / 70.870 |  99.032 / 23.96 / 60.880 | -0.037 / -9.18 / -9.990 |               5.6 / 4.2 | FAIL |
| apps-esbuild   |  99.908 / 77.65 / 94.491 |  99.887 / 77.73 / 94.500 | -0.020 / +0.08 / +0.010 |           107.3 / 131.2 | FAIL |
| apps-rollup    |  99.112 / 79.31 / 94.398 |  99.146 / 79.35 / 94.434 | +0.034 / +0.04 / +0.036 |           678.8 / 652.8 | PASS |
| apps-webpack   |  99.599 / 73.51 / 92.997 |  99.599 / 73.26 / 92.916 | +0.000 / -0.25 / -0.081 |           215.5 / 214.7 | FAIL |
| dev-rollup     |  99.873 / 77.53 / 94.430 |  99.872 / 77.44 / 94.403 | -0.001 / -0.09 / -0.027 |               2.1 / 2.1 | FAIL |
| dev-webpack    |  99.642 / 62.89 / 89.216 |  99.640 / 62.63 / 89.110 | -0.001 / -0.26 / -0.106 |               4.0 / 4.0 | FAIL |
| pop-bun        |  99.512 / 77.81 / 94.255 |  99.523 / 77.40 / 94.142 | +0.011 / -0.41 / -0.114 |         1214.7 / 1181.0 | FAIL |
| pop-esbuild    |  99.311 / 79.33 / 94.549 |  99.323 / 78.95 / 94.449 | +0.013 / -0.38 / -0.100 |         1753.9 / 1713.3 | FAIL |
| remin-esbuild  |  99.720 / 51.89 / 84.197 |  99.711 / 50.92 / 83.675 | -0.009 / -0.97 / -0.522 |           141.8 / 143.4 | FAIL |
| remin-terser   |  99.916 / 20.43 / 56.191 |  99.748 / 18.69 / 53.412 | -0.168 / -1.74 / -2.778 |             15.6 / 43.0 | FAIL |
| remin3-esbuild |  99.679 / 49.71 / 82.994 |  99.677 / 49.41 / 82.826 | -0.002 / -0.30 / -0.168 |           323.5 / 323.5 | FAIL |
| remin3-terser  |  99.849 / 17.58 / 51.585 |  99.858 / 17.33 / 51.140 | +0.009 / -0.26 / -0.444 |             51.1 / 47.2 | FAIL |
| remin4-esbuild |  99.401 / 46.37 / 80.896 |  99.383 / 46.01 / 80.665 | -0.018 / -0.36 / -0.231 |         1441.2 / 1472.0 | FAIL |
| remin4-terser  |  99.598 / 17.59 / 51.547 |  99.597 / 17.21 / 50.890 | -0.001 / -0.38 / -0.657 |           347.9 / 341.0 | FAIL |

**exp144's `veto_support = "shown"`** (`--commit 8592e0e9d --pipeline-set library.discovery.veto_support=shown`) on
the carrier index (`*exp148-car-shown*`) against `shown` on the recommended index (`*exp144-shown2*`):

| set            |        base P / R / F0.5 |   candidate P / R / F0.5 |      d P / d R / d F0.5 | wrong+app KB base / new | bar  |
| -------------- | -----------------------: | -----------------------: | ----------------------: | ----------------------: | ---- |
| cc             | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | +0.000 / +0.00 / +0.000 |               0.0 / 0.0 | PASS |
| esbuild-app    |  99.167 / 80.35 / 94.731 |  99.074 / 78.71 / 94.199 | -0.093 / -1.65 / -0.532 |           122.5 / 133.5 | FAIL |
| heldout        |  99.656 / 72.11 / 92.581 |  99.657 / 71.89 / 92.509 | +0.001 / -0.22 / -0.072 |           572.4 / 568.7 | FAIL |
| wrangler       |  99.106 / 34.51 / 72.109 |  99.084 / 25.33 / 62.617 | -0.022 / -9.18 / -9.492 |               5.6 / 4.2 | FAIL |
| apps-esbuild   |  99.909 / 79.85 / 95.129 |  99.890 / 79.94 / 95.140 | -0.019 / +0.09 / +0.011 |           108.4 / 131.9 | FAIL |
| apps-rollup    |  99.113 / 79.38 / 94.419 |  99.147 / 79.42 / 94.455 | +0.034 / +0.04 / +0.036 |           678.8 / 652.9 | PASS |
| apps-webpack   |  99.599 / 73.64 / 93.041 |  99.599 / 73.39 / 92.960 | +0.000 / -0.25 / -0.081 |           215.5 / 214.7 | FAIL |
| dev-rollup     |  99.873 / 77.67 / 94.471 |  99.872 / 77.58 / 94.444 | -0.001 / -0.09 / -0.027 |               2.1 / 2.1 | FAIL |
| dev-webpack    |  99.642 / 62.89 / 89.216 |  99.640 / 62.63 / 89.110 | -0.001 / -0.26 / -0.106 |               4.0 / 4.0 | FAIL |
| pop-bun        |  99.522 / 80.06 / 94.908 |  99.535 / 79.64 / 94.800 | +0.013 / -0.42 / -0.108 |         1224.6 / 1184.0 | FAIL |
| pop-esbuild    |  99.328 / 81.81 / 95.249 |  99.344 / 81.40 / 95.149 | +0.016 / -0.41 / -0.100 |         1763.1 / 1713.7 | FAIL |
| remin-esbuild  |  99.733 / 54.60 / 85.583 |  99.725 / 53.63 / 85.095 | -0.008 / -0.97 / -0.488 |           142.0 / 143.6 | FAIL |
| remin-terser   |  99.919 / 21.17 / 57.294 |  99.757 / 19.41 / 54.573 | -0.162 / -1.76 / -2.721 |             15.6 / 43.0 | FAIL |
| remin3-esbuild |  99.702 / 53.75 / 85.143 |  99.700 / 53.44 / 84.988 | -0.001 / -0.31 / -0.155 |           324.9 / 324.5 | FAIL |
| remin3-terser  |  99.855 / 18.31 / 52.814 |  99.864 / 18.04 / 52.368 | +0.009 / -0.27 / -0.446 |             51.1 / 47.2 | FAIL |
| remin4-esbuild |  99.425 / 48.38 / 82.102 |  99.408 / 48.02 / 81.880 | -0.017 / -0.37 / -0.222 |         1443.0 / 1473.7 | FAIL |
| remin4-terser  |  99.611 / 18.21 / 52.587 |  99.611 / 17.82 / 51.938 | -0.001 / -0.39 / -0.649 |           347.9 / 341.0 | FAIL |

- **The bar fails on 15 of 17 dev sets, both configurations.** F0.5 falls on 14 (wrangler -9.99, remin-terser
  -2.78, remin4-terser -0.66, esbuild-app -0.49, remin-esbuild -0.52); precision falls on 10 (remin-terser
  -0.168, esbuild-app -0.096, wrangler -0.037). It passes cc and apps-rollup only. App claimed is unchanged
  everywhere.
- **The targeted fix works exactly as exp147 predicted**: on pop, `agentmail`'s copy of the generated SDK runtime is
  no longer claimed as `@elevenlabs/elevenlabs-js` (76.8 KB wrong -> 0) and 1.30 MB of agentmail is now named
  right. `ng-packagr` -12.8 KB wrong. Pop precision rises (+0.011 / +0.013, wrong -34 / -41 KB), and with `shown`
  the pop sets' added wrong over main drops from +9.9 / +9.2 KB to +3.0 / +0.4 KB.
- **But the same ambiguity check now refuses the ORIGINALS.** Where the original package's own code is bundled, a
  carrier that copies it makes the print ambiguous and the claim is refused: -4.30 MB named on dev+pop (outside
  held-out) is refusals and their knock-on, with no carrier named at all (wrangler -182 KB: `forge-light`, a
  node-forge copy; esbuild-app -336 KB: `ccxt` vendoring `long`, `@launchql/protobufjs`,
  `vite-compatible-readable-stream`, `@architect/utils` vendoring chalk / color-convert / ansi-styles). Where
  the tie is broken instead, the carrier steals (`@finom/zod-to-json-schema` +21.3 KB wrong on esbuild-app, `eve`
  +41 KB). Held-out loses 494 KB named (net); of the units it stops naming, 559 KB have their whole print co-held by a
  carrier.
- `shown`'s remin4 `ag-psd` app claim (+592 B) is app code, untouched by any index change (55,549 B on both).

The simulation (§3) saw the steals but not the refusals: exp133's labels ran with ALL 13,785 dl25k packages, where
the same refusals were already spread over everything, so they landed in the "unattributed" column.

## 5. Gate

| check                                                | result                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| `npm run check` (bare, `/work/exp148-data/gate.log`) | exit 0, ALL 13 STAGES PASSED (on the tree committed with this README) |
| neutrality                                           | not run: no crates change, no key, no default                         |

## Files

- `scripts/census.py` — joins `experiments/142-index-coverage/scripts/misses.py` rows (scorer 14, both scopes) of the
  `*exp142-asof*` labels to the index's `versions.json` / manifest pins / refusals / shard build notes, a registry
  snapshot (exp142's plus this lane's fetch), and `closure_counts.py`'s training-app counts. Totals reproduce every
  card's `wrongFamilyBytes`.
- `scripts/closure_counts.py` — how many training apps bundle each package (exp108 `select_roots.py`'s count).
- `scripts/exact_owners.py` — which index packages hold each wrong unit's whole print.
- `scripts/policy_sim.py` — sizes version-policy changes (more release lines, prereleases) over the index's own
  package list and the class-b bytes each would bring under the index.
- `scripts/carriers.py` — the carrier test over a candidate index's unit table (copied files).
- `scripts/fetch_deps.py` — declared dependencies of the carrier candidates (registry).
- `scripts/sim133.py` — per-unit simulation of adding a SUBSET of exp133's dl25k packages, from exp133's labels.
- `scripts/score-all.sh`, `scripts/table148.py` — dev scoring (one label at a time) and the bar table.
- Row-level outputs (they name bundles) stay in `/work/exp148-data`.
- `data/`: `census.json` (per-set KB by class and reason, aggregates only), `policy-sim.json`, `carriers-C1d.txt`
  (the rule's 269 packages), `carrier-pins.txt` (its 595 versions), `table-*.json` (the §4 bar tables).

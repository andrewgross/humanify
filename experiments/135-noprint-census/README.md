# exp135: why the truth package has no print — the no-print census, and Bun's development builds

## STATUS (2026-10-22)

DONE on branch `exp135-noprint-census` (off main `4b1d1a49`). One builder recipe flag (`production`, default
off: no existing recipe or index changes), census scripts, a test index and its labels. **Not merged.**

- **Why the truth has no print** (exp134's largest wrong class: held-out 572 KB, pop-bun 610, pop-esbuild 946,
  apps-rollup 123), per unit from the index's own shards (section 1):
  - **held-out: the index's Bun slices hold DEVELOPMENT builds.** Bun inlines an undefined
    `process.env.NODE_ENV` as "development", so a package that picks its file by NODE_ENV (react, react-dom,
    scheduler, ...) has only its development file in the Bun slices, while app bundles (and every corpus) define
    production. 207 KB held-out, 86 KB test-bun (aggregate), 26 KB pop-bun, plus most of "no shard of that
    version" (held-out 52, pop-bun 51: versions seen only inside other packages' Bun builds).
  - held-out's next classes: same file, different print 164 KB (not NODE_ENV; open); multi-module rule 42 KB
    (Bun 1.4 map defect on the builder side); package not indexed 43 KB.
  - **pop / apps: mostly NOT coverage.** Sponge-rule drops of vendored copies (pop 229 KB, apps 52; deliberate:
    keeping them makes carriers, exp133/134), the truth's version published after the index's as-of (pop-esbuild
    270 KB: the corpus is newer than the index), packages not indexed (pop 329; dl25k measured worse), versions
    released in the last year but not indexed (pop 180), and units whose truth family DOES hold the print but
    the lookup reads the winner's whole print first (pop-esbuild 188, test-esbuild 757 aggregate).
- **Fix built: `k2-all-r5-origin-prod`** = k2-all-r5-origin with both Bun slices built under
  `production` (3.3 h + 20 min merge on cores 34-63, installs reused). Scorer 13, both scopes, main defaults:
  - **precision AND recall up on every Bun set**: held-out P 99.41 -> 99.56, F0.5 90.90 -> 91.29; pop-bun
    99.44 / 93.85 -> 99.49 / 94.06; test-bun 99.51 / 91.42 -> 99.55 / 91.85; test2-bun 99.15 / 90.87 ->
    99.26 / 91.38 (summary only).
  - every other set (esbuild, rollup, webpack, remin, apps, bundlers, cc) byte-identical; app 0.
  - **ship bar MET.** RECOMMEND `k2-all-r5-origin-prod` as the default index (docs/design/library-index.md §10)
    and `production` on both Bun recipes for every future build. Not flipped here.
- **Neutrality** of the crates change: warm, 2.1.215:2.1.216 vs `4b1d1a49e`, NEUTRAL 0 / 0, cache +0 / +0 in
  default AND report mode (`library-report.json` byte-identical).
- Other fixes sized, not built (section 2): year-all versions for every package (+56.6k versions, ~18 h) for
  ~180 KB of pop; a re-dated index for the corpus newer than the as-of; the multi-module rule (keep the file's
  real module) as the next small builder lever; esbuild loader retries for ~900 failed builds.
- Gate: 13/13 bare at `6ab6da5c` (the crates change) and at the final commit.

## 1. The census

exp134 found that the largest wrong-bytes class on every dev set, under main's defaults on the recommended index
`k2-all-r5-origin`, is an index seed whose truth package holds NO print of the unit the index reads ("truth family
not read for the unit": held-out 572 KB, pop-bun 610, pop-esbuild 946, apps-rollup 123). This experiment asks the
index's own build artefacts why.

`scripts/noprint.py` takes every wrong-family index seed of a label (exp133's `judge13.py` rows, which reproduce the
cards; for the frozen test split an inline judge that keeps aggregates only), keeps the ones exp134's census puts
in this class, and reads the truth row scorer 13 gives the unit: package T, version V, file F (and origin O). Then
it asks, in order:

| class | question                                                                                                                                                                                                                     |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| a     | Is T in the index at all (`versions.json`)? If not, is it in exp133's dl25k index, at V?                                                                                                                                     |
| b     | T is indexed, V is not. Was V published after the index's as-of date, in the 12 months before it, or is it the latest patch of its minor (what a version policy would have picked)? Nearest indexed version by semver/date. |
| c     | T@V is indexed but its rows do not carry the unit. Read T@V's shard in the bundle's slice (`store/shards/<slice>/T@V.json`): build failed, no shard, file not a module, sponge rule, multi-module rule.                      |
| d     | F has unit rows at T@V, none is the unit's print.                                                                                                                                                                            |
| e     | The truth row is not a plain map label: misattributed (scorer 13), no version in the map, or F is not a file the package ships (a pre-build source path from the package's own map).                                         |
| f     | T's family DOES hold a print of the unit, the lookup does not read it: every row over the specificity bound, the whole print read first (held by the winner only), or more owners than `max_packages`.                      |

Held-out and test appear as aggregates only (no script prints their package names; held-out rows are written
without names). test2 is a frozen judge and was never read.

### Bytes per class (KB, base = `*exp133-base*` = k2-all-r5-origin, main `4b1d1a49` defaults, scorer 13, both scopes)

| class | esbuild-app | heldout | wrangler | apps-esbuild | apps-rollup | apps-webpack | pop-bun | pop-esbuild | remin-esbuild | remin-terser | test-bun | test-esbuild | test-rollup | test-webpack |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| a  package not indexed (in dl25k at that version) | 1.9 | 25.0 | 0.0 | 8.1 | 34.6 | 0.0 | 10.4 | 14.6 | 0.0 | 0.0 | 6.5 | 7.7 | 3.2 | 2.9 |
| a  package not indexed (in dl25k, other version) | 0.0 | 7.8 | 0.0 | 0.0 | 0.0 | 0.0 | 28.2 | 25.6 | 0.0 | 0.0 | 33.2 | 105.5 | 52.2 | 43.0 |
| a  package not indexed (not in dl25k either) | 1.7 | 9.8 | 0.0 | 16.7 | 12.8 | 19.9 | 106.6 | 144.0 | 14.2 | 0.0 | 126.3 | 77.3 | 41.6 | 68.0 |
| b  version not indexed: published after the index's as-of | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.4 | 269.8 | 0.0 | 0.0 | 13.5 | 41.4 | 0.7 | 0.0 |
| b  version not indexed: released in the last 12 months | 2.7 | 4.3 | 0.0 | 0.0 | 0.0 | 0.0 | 90.5 | 89.6 | 0.0 | 0.0 | 5.4 | 12.6 | 11.9 | 3.3 |
| b  version not indexed: latest patch of its minor | 0.0 | 2.1 | 0.0 | 0.0 | 29.8 | 0.8 | 10.7 | 10.1 | 0.0 | 2.0 | 0.0 | 105.2 | 0.0 | 36.2 |
| b  version not indexed: older | 0.0 | 11.9 | 0.0 | 0.0 | 0.0 | 0.0 | 6.4 | 0.0 | 2.8 | 0.0 | 3.0 | 1.7 | 0.0 | 0.1 |
| c  dropped: Bun built the development file (NODE_ENV) | 0.0 | 207.1 | 0.0 | 0.0 | 1.0 | 0.0 | 25.9 | 0.0 | 0.0 | 0.0 | 86.2 | 0.0 | 0.2 | 0.6 |
| c  dropped: multi-module rule (file credited to 2+ modules) | 0.0 | 42.2 | 0.0 | 0.0 | 0.0 | 0.0 | 21.2 | 0.0 | 0.0 | 0.0 | 1.4 | 0.0 | 0.0 | 0.0 |
| c  dropped: sponge rule (vendored path / foreign region) | 1.3 | 27.1 | 0.0 | 19.1 | 4.3 | 28.4 | 109.6 | 119.2 | 6.0 | 0.0 | 17.8 | 8.3 | 0.5 | 22.8 |
| c  dropped: the slice build failed | 6.6 | 1.3 | 0.0 | 0.7 | 0.0 | 0.0 | 0.0 | 0.2 | 15.3 | 0.6 | 0.4 | 0.7 | 0.4 | 0.0 |
| c  dropped: no shard of that version in the slice | 0.0 | 52.4 | 1.4 | 0.0 | 0.0 | 0.0 | 51.4 | 1.3 | 0.0 | 0.1 | 8.3 | 0.0 | 0.0 | 0.0 |
| c  dropped: other (entry not built, no rows) | 0.0 | 0.6 | 0.0 | 0.3 | 10.6 | 0.0 | 0.0 | 0.8 | 0.0 | 0.0 | 0.0 | 0.2 | 0.2 | 0.0 |
| d  print differs (same file has rows, not this unit's) | 1.9 | 164.4 | 0.0 | 0.3 | 1.6 | 1.1 | 14.8 | 5.1 | 0.5 | 1.7 | 59.5 | 3.8 | 11.5 | 0.5 |
| e  truth: file not in the package (pre-build source path) | 0.0 | 0.0 | 0.0 | 6.4 | 0.0 | 0.0 | 0.0 | 61.7 | 0.1 | 0.0 | 0.0 | 10.7 | 0.0 | 0.0 |
| e  truth: no version in the map / misattributed | 3.3 | 9.2 | 0.0 | 0.0 | 0.0 | 34.0 | 6.5 | 11.9 | 0.5 | 0.0 | 11.1 | 158.0 | 0.0 | 0.0 |
| f  family holds the print, lookup does not read it: spec bound | 0.0 | 0.0 | 4.1 | 0.0 | 2.8 | 0.0 | 69.4 | 0.0 | 0.0 | 0.0 | 1.3 | 1.0 | 410.5 | 0.0 |
| f  family holds the print, lookup does not read it: whole print read first | 11.7 | 6.4 | 0.0 | 2.1 | 17.4 | 3.2 | 56.0 | 188.2 | 30.7 | 0.0 | 45.7 | 757.3 | 18.5 | 18.0 |
| f  family holds the print, lookup does not read it: other | 0.0 | 0.0 | 0.0 | 0.0 | 8.0 | 1.8 | 2.0 | 4.3 | 0.0 | 0.0 | 0.0 | 3.0 | 0.0 | 4.2 |
| **total** | **31.1** | **571.6** | **5.5** | **53.7** | **122.9** | **89.2** | **610.0** | **946.4** | **70.1** | **4.4** | **419.6** | **1294.4** | **551.4** | **199.6** |

Totals reproduce exp134's class (held-out 571.6, pop-bun 610.0, pop-esbuild 946.4, apps-rollup 122.9).

What the classes are, concretely (dev examples; held-out/test only by aggregate):

- **c, Bun built the development file.** The unit's file F is a module of T@V in the esbuild/rollup/webpack
  shards but not in the Bun slice the bundle reads, and no drop rule explains it. Bun inlines an undefined
  `process.env.NODE_ENV` as `"development"`: a package whose entry picks its build by NODE_ENV
  (`if (process.env.NODE_ENV === "production") module.exports = require("./cjs/x.production.js")`) gets only its
  development file into the index's Bun slices (react, react-dom, scheduler, use-sync-external-store, headlessui,
  tanstack devtools in the dev set), while app bundles — and every corpus — define NODE_ENV as "production" and
  ship the production file. esbuild (no define, platform node) keeps both branches; webpack's recipe script
  already defines production; rollup's keeps both. Reproduced by hand: `bun build` of @headlessui/react's entry
  maps `react.development.js` without the define and `react.production.js` with it.
  **held-out 207 KB, test-bun 86 KB (aggregate), pop-bun 26 KB.**
- **c, no shard of that version.** T@V is in `versions.json` only as a dependency built inside other packages'
  shards (e.g. react-dom 18.x, never built on its own), so the Bun slice again holds its development file.
- **c, multi-module rule.** The builder drops every row of a source file its map credits to two or more modules
  (exp132's Bun 1.4 map defect, builder side): held-out 42 KB, pop-bun 21 KB.
- **c, sponge rule.** The truth credits a shipper's vendored copy (`vendor/…`, `third-party.js`) or a foreign
  bundler region the builder deliberately drops (exp097/exp124): the claimer is the code's origin or another copy.
  Keeping those rows makes carriers (exp133/134's failure). pop 229 KB, apps 52, held-out 27.
- **b, after the as-of.** pop-esbuild's 270 KB is one shipper version published a day after the index's as-of
  (2026-10-11) and one not in the packument snapshot: the corpus is newer than the index.
- **e.** pop-esbuild 62 KB and apps-esbuild 6 KB are units the shipper's own source map names under a source path
  it does not ship (`src/saxes.ts` inside jsdom's dist): the code is the dependency's.

## 2. Sizing the fixes

Each class against the generic fix that would reach it (KB from the table above; "dev" = held-out aggregate,
pop, apps; test = aggregate):

| class | generic fix | sized how (no rebuild where possible) | build cost | verdict |
| ----- | ----------- | ------------------------------------- | ---------- | ------- |
| c, Bun built the development file (held-out 207, test-bun 86, pop-bun 26) and the Bun part of "no shard of that version" (held-out 52, pop-bun 51) | build the Bun slices with `process.env.NODE_ENV` defined as "production" (new recipe flag `production`, section 3) | **mini rebuild** of the 68 dev truth versions in classes c/d (`mini_check.py`): pop-bun's class 23.0 of 25.9 KB now carries the unit, plus 7.3 of 7.7 KB "sponge region re-owned"; the esbuild production rebuild recovered 0 on every esbuild set (esbuild already keeps both branches) | **3.3 h shards + 20 min merge** on cores 34-63 (both Bun slices + every literals shard, 62,739 versions, installs reused); ~300 GB of new builds; index 6.5 GB | **BUILT and scored** (section 4): ship bar met |
| b, released in the last 12 months (pop-bun 91, pop-esbuild 90, test 33, held-out 4) | version policy `year-all` for EVERY indexed package (today: roots only; pins at their pinned versions) | policy flags per unit from the registry snapshot | +56,586 versions (+90%), ~18 h at exp133's rate | not built: one dev pop set carries it; cost 5x the cap |
| b, latest patch of its minor (test-esbuild 105, apps-rollup 30, test-webpack 36) | latest patch of every minor for every package | as above | +151,950 versions | not built |
| b, published after the as-of (pop-esbuild 270, test-esbuild 41) | re-date the index (the corpus is newer than 2026-10-11) | registry dates | a dated rebuild of recent versions | not a policy gap; next index refresh |
| a, package not indexed (pop 329, test 520 aggregate, held-out 43, apps 92) | a larger package set | in dl25k at the exact version: held-out 25 of 43 KB, pop 25 of 329 | exp133 built it (10.9 h) | measured by exp133/134: dl25k LOWERS precision on 15/21 sets (carriers); not a coverage win |
| c, multi-module rule (held-out 42, pop-bun 21) | keep the module whose code IS the file's (Bun 1.4 credits a file to a second module that holds none of it, exp132) instead of dropping every row of the file | not simulated (needs the builder's module attribution) | crates change + re-fingerprint from cached builds (~4.4 h) | next lever, small |
| c, sponge rule (pop 229, apps 52, held-out 27) | none: the rule drops vendored copies on purpose | exp133/134: keeping a shipper's copies makes carriers that outscore and tie origins | - | keep the rule |
| c, slice build failed (remin 16, esbuild-app 7) | retry esbuild with `--loader:.X=empty` for "No loader is configured" (`scan_builds.py`: 459 `.node`, 86 `.wasm`, 76 `.html` ... of 1,607 failed esbuild versions; Bun 1,180, rollup 2,447, webpack 2,009 fail mostly on syntax/Flow) | `scan_builds.py` over the store | small rebuild of ~900 esbuild versions | small; worth folding into the next builder change |
| d, print differs (held-out 164, test-bun 60, pop-bun 15) | none identified: the production rebuild leaves it untouched (held-out 164 -> 164) | `mini_check.py`: 0 of pop-bun's 15 KB | - | open: same file, different print |
| e, truth (test-esbuild 158 no version in the map, pop-esbuild 62 pre-build source path) | scorer: a unit whose map names a file the package does not ship is the dependency's | - | - | truth lane |
| f, held but not read (test-esbuild 757 / test-rollup 410 aggregate, pop-esbuild 188, pop-bun 125) | lookup side: whole print read first (the winner holds the whole unit, the truth only its functions), specificity bound | - | - | not coverage; the next big class |

The unit floor (20 nodes) never decides this class: the bundle side reads prints of 30+ nodes only.

## 3. The `production` recipe flag and the test index

`[recipe]` (crates `humanify-core::libindex::recipe`, `humanify-index::recipe`): a recipe may carry `production`
(`bun:target=node,format=cjs,minify,production`), Bun and esbuild only (webpack's recipe script always defines it,
rollup's never: refused rather than ignored). It adds `--define process.env.NODE_ENV="production"` (Bun) /
`--define:process.env.NODE_ENV="production"` (esbuild), exactly what the corpus bundles are built with
(exp095 `closure.py`, exp100 `build.py`). The SLICE NAME is unchanged, so a bundle reads it the same way; the
recipe spelling (in every shard key and the manifest) changes, so those shards rebuild. Existing recipes and
indexes are byte-identical (no index ever carried the flag). Tests, red first:
`a_production_recipe_reads_back_and_keeps_its_slice`, `production_is_refused_where_the_recipe_script_decides_it`
(core), `a_production_recipe_defines_node_env_as_production_and_the_default_does_not` (builder).

**Test index `k2-all-r5-origin-prod`** (`/work/exp135-data/full/ix/k2-all-r5-origin-prod`, 6.5 GB, index id
`9d40265a…`): exp130's full build (same roots, pins, `year-all+minor`, `--sponge-regions origin`, bundlers) with both
Bun recipes `production`. `scripts/build-prod.sh`: store and work dir are hard-link copies of exp130's (installs
reused, a rebuilt slice writes its own build dir), `flock /work/index-build.lock`, cores 34-63, 124 chunks all
exit 0, 11,791 s of shards, 1,177 s merge at 34.8 GB peak. /work stayed above 1.1 TB free.

- The esbuild, rollup and webpack unit tables are byte-identical in content (`.bin` tables identical; the
  `.jsonl` spell version ranges over a `versions.json` that lost the dev-only dependency versions).
- The literals tables changed (their shards read every slice's builds).
- `helpers.jsonl` is new: main's builder writes exp129's helper table, read only with `helper_prints` on
  (default off).
- In the Bun slices, 14,624 of 62,739 versions (bun-node; bun-bun 13,550) changed rows: 2.17 M rows only in
  the development build, 1.74 M only in the production one (`diff_shards.py`).

## 4. Results

Main `4b1d1a49`'s plain defaults (factory_bodies and its three keys on), structural rule, scorer 13, both
scopes; base = `*exp133-base*` (k2-all-r5-origin, exp133's labels), new = `*exp135-prod*`. test and test2 are
summaries only; test2 was never diagnosed. P / R / F0.5 in %.

| set | k2-all-r5-origin P / R / F0.5 | k2-all-r5-origin-prod P / R / F0.5 | dP | dF0.5 | wrong KB (base -> last) | verdict |
|---|--:|--:|--:|--:|--:|---|
| cc | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | +0.00 | +0.00 | 0.0 -> 0.0 | identical |
| esbuild-app | 99.07 / 74.74 / 93.01 | 99.07 / 74.74 / 93.01 | +0.00 | +0.00 | 131.1 -> 131.1 | identical |
| heldout | 99.41 / 67.71 / 90.90 | 99.56 / 68.51 / 91.29 | +0.15 | +0.39 | 938.2 -> 704.2 | up (P and F0.5) |
| wrangler | 99.07 / 33.14 / 70.87 | 99.07 / 33.14 / 70.87 | +0.00 | +0.00 | 5.8 -> 5.8 | identical |
| apps-esbuild | 99.89 / 77.65 / 94.48 | 99.89 / 77.65 / 94.48 | +0.00 | +0.00 | 135.0 -> 135.0 | identical |
| apps-rollup | 99.09 / 79.31 / 94.38 | 99.09 / 79.31 / 94.38 | +0.00 | +0.00 | 710.6 -> 710.6 | identical |
| apps-webpack | 99.58 / 73.50 / 92.98 | 99.58 / 73.50 / 92.98 | +0.00 | +0.00 | 230.8 -> 230.8 | identical |
| dev-rollup | 99.87 / 77.53 / 94.43 | 99.87 / 77.53 / 94.43 | +0.00 | +0.00 | 2.1 -> 2.1 | identical |
| dev-webpack | 99.64 / 62.89 / 89.22 | 99.64 / 62.89 / 89.22 | +0.00 | +0.00 | 4.1 -> 4.1 | identical |
| pop-bun | 99.44 / 76.61 / 93.85 | 99.49 / 77.19 / 94.06 | +0.05 | +0.21 | 1400.8 -> 1277.1 | up (P and F0.5) |
| pop-esbuild | 99.30 / 78.71 / 94.37 | 99.30 / 78.71 / 94.37 | +0.00 | +0.00 | 1799.4 -> 1799.4 | identical |
| remin-esbuild | 99.69 / 51.76 / 84.11 | 99.69 / 51.76 / 84.11 | +0.00 | +0.00 | 160.0 -> 160.0 | identical |
| remin-terser | 99.90 / 20.33 / 56.04 | 99.90 / 20.33 / 56.04 | +0.00 | +0.00 | 18.6 -> 18.6 | identical |
| test-bun | 99.51 / 68.99 / 91.42 | 99.55 / 70.16 / 91.85 | +0.04 | +0.43 | 1029.7 -> 953.8 | up (P and F0.5) |
| test-esbuild | 99.26 / 73.37 / 92.72 | 99.26 / 73.37 / 92.72 | +0.00 | +0.00 | 1740.0 -> 1740.0 | identical |
| test-rollup | 98.96 / 72.43 / 92.21 | 98.96 / 72.43 / 92.21 | +0.00 | +0.00 | 829.8 -> 829.8 | identical |
| test-webpack | 99.48 / 72.19 / 92.49 | 99.48 / 72.19 / 92.49 | +0.00 | +0.00 | 568.4 -> 568.4 | identical |
| test2-bun | 99.15 / 68.12 / 90.87 | 99.26 / 69.37 / 91.38 | +0.12 | +0.52 | 1445.6 -> 1269.5 | up (P and F0.5) |
| test2-esbuild | 99.63 / 71.04 / 92.21 | 99.63 / 71.04 / 92.21 | +0.00 | +0.00 | 723.1 -> 723.1 | identical |
| test2-rollup | 96.35 / 75.65 / 91.35 | 96.35 / 75.65 / 91.35 | +0.00 | +0.00 | 1693.7 -> 1693.7 | identical |
| test2-webpack | 99.27 / 70.09 / 91.64 | 99.27 / 70.09 / 91.64 | +0.00 | +0.00 | 874.1 -> 874.1 | identical |

- **Every Bun set goes up in precision AND recall**: held-out P 99.41 -> 99.56, F0.5 +0.39 (234 KB less wrong,
  1.89 MB more named); pop-bun +0.05 / +0.21; test-bun +0.04 / +0.43; test2-bun +0.12 / +0.52 (176 KB less wrong,
  3.07 MB more named).
- **Every other set is byte-identical** (summary sets compared field by field). The one exception is cc, whose
  sample-method exploratory names count 20.6 KB more exact bytes; its scored numbers are identical.
- App claimed bytes 0 everywhere.
- **Ship bar** (F0.5 and precision at or above k2-all-r5-origin on held-out, pop, test, test2; no material loss
  elsewhere): **MET.**
- Control: this branch's binary on k2-all-r5-origin (`*exp135-ctl*`, dev) reproduces `*exp133-base*`'s summary
  exactly.

**The class after the fix** (`noprint.py` on `*exp135-prod*`, its own store): held-out 571.6 -> 398.0 KB (the
development-file class 207 -> 84, no shard 52 -> 4; print-differs unchanged at 164), pop-bun 610.0 -> 571.1
(development file 26 -> 3, no shard 51 -> 42: one misattributed-looking unit), test-bun 419.6 -> 368.8 (aggregate; development file 86 -> 42). The remaining
84 KB held-out "development file" signature is files the Bun slice still does not contain while the other
bundlers do; the dev residual is one SDK whose Bun build holds no modules at all (its runtime shims), not NODE_ENV.

**Neutrality** (the crates change; worktree frozen at `6ab6da5c`, `flock /work/neutrality.lock`, warm cache,
2.1.215:2.1.216 vs `4b1d1a49e`): default mode NEUTRAL 0 files / 0 lines, cache +0 / +0, exit codes equal; report
mode (`library.matching=report`, `library.index=k2-all-r5-origin`) NEUTRAL 0 / 0, +0 / +0, `library-report.json`
byte-identical.

## Reproduce

```bash
S=experiments/135-noprint-census/scripts; J=/work/exp133-data/judge; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# census (base): exp133's judge13 rows; test inline, aggregates only
$PY -I $S/noprint.py dev $J/dev-base-dl.jsonl c0 exp133-base /work/exp135-data/dev > dev.txt      # apps/pop/remin likewise
$PY -I $S/noprint.py test - c0 test-exp133-base /work/exp135-data/test --private > test.txt
python3 -I $S/summarize.py dev.txt apps.txt pop.txt remin.txt test.txt --json $S/../census-base.json
python3 -I $S/scan_builds.py /work/exp130-data/full/store/shards builds.json          # failed slice builds
# mini rebuild of the dev truth versions under the production recipes, then: does the truth hold the unit now?
humanify-index build --packages empty.txt --pinned mini-pkgs.txt --as-of 2026-10-11 --offline --sponge-regions origin \
  --recipe bun:target=node,format=cjs,minify,production --recipe esbuild:target=node,format=cjs,minify,production ...
python3 -I $S/mini_check.py mini-units.json mini/store/shards --old /work/exp130-data/full/store/shards
# the test index and its scores
$S/build-prod.sh /work/exp135-data/full <humanify-index at 6ab6da5c>
python3 -I $S/diff_shards.py /work/exp130-data/full/store/shards /work/exp135-data/full/store/shards bun-node-cjs-min
$S/score-all.sh exp135-prod /work/exp135-data/full/ix/k2-all-r5-origin-prod
python3 -I experiments/133-dl25k-r5/scripts/table.py exp133-base exp135-prod
$PY -I $S/noprint.py dev - c0 exp135-prod post-dev --store /work/exp135-data/full/store/shards --work /work/exp135-data/full/work
```

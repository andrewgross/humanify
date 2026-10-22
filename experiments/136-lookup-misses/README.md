# exp136: the lookup misses (class f) and the same-file different prints (class d) — mostly the truth's map, not the lookup

## STATUS (2026-10-22)

DONE on branch `exp136-lookup-misses` (off main `869931920`). **Nothing shipped, nothing to merge into the pipeline**:
one default-off builder recipe flag (`static`) built, scored and NOT recommended; census and audit scripts; labels.

- **Census re-run on the new base** (`k2-all-r5-origin-prod`, main defaults, scorer 13, both scopes): exp135's
  no-print class reproduces exactly (held-out 398.0 KB, pop-bun 571.1, test-bun 368.8 aggregate; section 1).
- **Class d is the TRUTH's source-map defect, not a print difference.** Held-out's 164.4 KB: in **151.2 KB the
  truth's package holds none of the unit's code and the CLAIMER's package does** (an encoding table credited to
  another package's `index.js`, a spinners JSON credited to a cursor package ...): Bun 1.4's bundle map credits a
  module's code to another module's file (the defect exp132 found builder-side). pop-bun 6.7 of 14.6 KB, pop-esbuild
  2.2 of 5.1 likewise. No build shape recovers them (static entry, ESM output: 1.6 KB of 164).
- **The multi-module class (held-out 42.2 KB, pop-bun 21.2) is the same defect**: the truth's file lacks those
  units too (pop-bun: the claimer holds 18.8 KB). Keeping "the file's real module" in the index recovers none of it.
- **Class f is not a lookup bug either** (section 2): test-esbuild's 757 KB and pop-esbuild's 188 are units whose
  truth has NO version in its map (711 / 133 KB: truth lane) or a version the index lacks (46 / 56: coverage);
  test-rollup's 410 KB is version coverage (406); pop-bun's 69 KB spec-bound is a shipper's inlined copy the claimer
  is the origin of. Only ~35 KB (pop-bun), 25 (test-bun, aggregate), 16 (apps-rollup) is "the truth version is
  indexed and the whole print differs" — a build-shape gap.
- **The biggest precision number found is a measurement one**: auditing every wrong-family byte against the
  installed code (section 3), the truth's map defect accounts for **held-out 314 KB of 704 KB wrong (P 99.56 ->
  99.76 audited)**, pop-bun 148 of 1,277 (99.49 -> 99.55), pop-esbuild 100 of 1,799 (99.30 -> 99.34). The pipeline
  is right on those bytes; scorer 13 calls them wrong. **Recommend a truth-lane fix** (scorer: a unit whose labelled
  package's installed files lack its name evidence while the claimed package's hold it is the claimer's).
- **Built and scored: recipe flag `static`** (a third app entry, `import * as mN from "<subpath>"`, the eager
  scope-hoisted form an app's own ESM source makes; neither the `require()` nor the `import()` entry produces it;
  sized: 11.6% of a Bun build's and 24% of an esbuild build's unit bytes are new prints). Index
  `k2-all-r5-origin-prod-static` (Bun x2 + esbuild static, 4.9 h + 24 min merge): cc F0.5 +0.73, wrangler +0.80,
  pop-bun +0.05, test2-esbuild +0.02; but **precision down on held-out -0.03, test-bun -0.04, test2-bun -0.07,
  remin-esbuild -0.04** and F0.5 down on test-bun, test-esbuild, pop-esbuild, remin. **Ship bar NOT met; not
  recommended.** Held-out's whole loss is one right claim the truth's map mislabels (audited: held-out P 99.759 ->
  99.762, pop-bun 99.554 -> 99.561 — Bun up); the esbuild loss is real (carriers: both packages hold the code,
  pop-esbuild +9 KB, remin-esbuild +18 KB). No neutrality run (nothing met the bar).
- **Refuted at simulation: an index-side evidence check** (drop a unit row whose file and package lack the unit's
  name evidence; built, then removed): it drops 2.5-11x more RIGHT claims than wrong ones on every dev set (section 4).
- Gate: 13/13 bare at `47c644884` and `7bd37903a` (crates) and at the final commit.

## 1. The census on the new index

`experiments/135-noprint-census/scripts/noprint.py` (exp135's, unchanged rules) on the `*exp135-prod*` labels with
`--store/--work` of the new index, every corpus inline (judge13's verdicts; test aggregates only; test2 never read).
KB of wrong index seeds whose truth family has no print of the unit the lookup reads:

| class                                                      | esbuild-app |   heldout | wrangler | apps-esbuild | apps-rollup | apps-webpack |   pop-bun | pop-esbuild | remin-esbuild | remin-terser |  test-bun | test-esbuild | test-rollup | test-webpack |
| ---------------------------------------------------------- | ----------: | --------: | -------: | -----------: | ----------: | -----------: | --------: | ----------: | ------------: | -----------: | --------: | -----------: | ----------: | -----------: |
| a package not indexed                                      |         3.6 |      42.8 |      0.0 |         24.8 |        47.4 |         19.9 |     145.2 |       184.2 |          14.2 |          0.0 |     166.0 |        190.5 |        97.0 |        113.9 |
| b version not indexed                                      |         2.7 |      18.3 |      0.0 |          0.0 |        29.8 |          0.8 |     110.6 |       369.5 |           2.8 |          2.0 |      21.9 |        160.9 |        12.6 |         39.6 |
| c dropped: Bun built the development file (residual)       |         0.0 |      84.4 |      0.0 |          0.0 |         1.0 |          0.0 |       3.0 |         0.0 |           0.0 |          0.0 |      42.2 |          0.0 |         0.2 |          0.6 |
| c dropped: multi-module rule                               |         0.0 |      42.2 |      0.0 |          0.0 |         0.0 |          0.0 |      21.2 |         0.0 |           0.0 |          0.0 |       1.4 |          0.0 |         0.0 |          0.0 |
| c dropped: sponge rule                                     |         1.3 |      27.1 |      0.0 |         19.1 |         4.3 |         28.4 |     102.3 |       119.2 |           6.0 |          0.0 |      17.8 |          8.3 |         0.5 |         22.8 |
| c dropped: other (build failed, no shard, entry not built) |         6.6 |       4.9 |      1.4 |          1.0 |        10.6 |          0.0 |      42.2 |         2.3 |          15.3 |          0.7 |       3.2 |          0.9 |         0.6 |          0.0 |
| **d print differs (same file has rows)**                   |         1.9 | **164.4** |      0.0 |          0.3 |         1.6 |          1.1 |      14.6 |         5.1 |           0.5 |          1.7 |  **59.5** |          3.8 |        11.5 |          0.5 |
| e truth questionable                                       |         3.3 |       9.2 |      0.0 |          6.4 |         0.0 |         34.0 |       6.5 |        73.6 |           0.6 |          0.0 |      11.1 |        168.7 |         0.0 |          0.0 |
| **f family holds the print, lookup does not read it**      |        11.7 |       4.7 |      4.1 |          2.1 |        28.2 |          5.0 |     125.5 |       192.5 |          30.7 |          0.0 |      45.7 |        761.3 |       429.0 |         22.2 |
| **total**                                                  |    **31.1** | **398.0** |  **5.5** |     **53.7** |   **122.9** |     **89.2** | **571.1** |   **946.4** |      **70.1** |      **4.4** | **368.8** |   **1294.4** |   **551.4** |    **199.6** |

Per-subclass rows: `census-new.json` (exp135's 19 classes).

## 2. Class f and class d, broken down

`noprint.py --detail` adds, per row: the bundle's output format (ESM / CJS), whether the truth's version is indexed
or present in its map at all, and which side holds the code in the smaller index module (`fd_table.py`,
`fd-table.json`; KB):

| sub-cause                                                                                       | heldout | pop-bun | pop-esbuild | apps-rollup | esbuild-app | remin-esbuild | test-bun | test-esbuild | test-rollup | test-webpack |
| ----------------------------------------------------------------------------------------------- | ------: | ------: | ----------: | ----------: | ----------: | ------------: | -------: | -----------: | ----------: | -----------: |
| f: truth has no version in its map (truth lane)                                                 |     0.0 |     0.0 |       132.7 |         0.0 |         8.3 |          28.3 |      0.0 |        711.1 |         0.0 |          0.0 |
| f: truth version not indexed (coverage)                                                         |     4.0 |    21.1 |        55.5 |         2.5 |         2.0 |           0.4 |     20.1 |         46.2 |       406.4 |         12.6 |
| f: spec bound, claimer holds it in a 4x smaller module (truth credits a shipper's inlined copy) |     0.0 |    69.4 |         0.0 |         2.8 |         0.0 |           0.0 |      0.4 |          1.0 |         9.8 |          0.0 |
| f: whole print read first, truth version indexed (build shape)                                  |     0.7 |    35.0 |         3.5 |        16.3 |         1.3 |           2.0 |     25.1 |          0.0 |        12.1 |          8.4 |
| f: other (too many owners, ...)                                                                 |     0.0 |     0.0 |         0.8 |         6.6 |         0.0 |           0.0 |      0.0 |          3.0 |         0.6 |          1.3 |
| d: same file has rows, none of this unit's                                                      |   164.4 |    14.5 |         5.1 |         1.6 |         1.9 |           0.5 |     59.4 |          3.8 |        11.5 |          0.5 |

(apps-esbuild 2.1 KB, apps-webpack 6.6, wrangler 4.1, remin-terser 1.7 KB in all: in `fd-table.json`.)

**f, "whole print read first"**: a carrier (a package whose dist pre-bundles the truth's code) holds the unit's
whole statement print; the truth family holds only its function prints. Hand-checked (pop-bun, dev): the carrier's
copy matches the app's form, the truth's own index builds do not — the `require()` entry wraps the package's ESM
modules as CJS factories and reads a subpath's exports through a namespace object (`u.signals`), the `import()`
entry wraps them as lazy module bodies (`E=class E ...`); an app's own ESM `import` scope-hoists them eagerly
(`class X ...`, `for (... of W)`). A static `import * as` entry reproduces the app's whole print exactly
(`variants.py`: re-built with Bun 1.4.0 / esbuild and fingerprinted with `humanify discover --unit-prints`;
pop-bun 34.2 of 34.9 KB recovered by the static entry). Rollup hoists in every entry and webpack wraps every module,
so neither slice gains (0.03% / ~0% new prints).

**d, "same file, different print"** — `dcheck.py` / `whohas.py` read the unit's NAME EVIDENCE (string and template
values, `.property` names and object keys of 4+ characters, tokenized by acorn: `evtokens.mjs`; less a stoplist)
against the installed files of the truth's version (the package and its dependencies) and of the claimer:

| d bytes (KB)                                                                            |   heldout |  pop-bun | pop-esbuild |
| --------------------------------------------------------------------------------------- | --------: | -------: | ----------: |
| the truth's package (and its deps) lack the unit's code; the claimer's package holds it | **151.2** |      6.7 |         2.2 |
| the truth's package holds it in another file                                            |       2.4 |        — |           — |
| nobody installed holds it (pop-bun: +0.2 a third package)                               |       5.6 |      1.6 |           — |
| the truth's file holds it (a real print difference) / under 3 evidence tokens           |       5.2 |      6.3 |         2.9 |
| **total**                                                                               | **164.4** | **14.6** |     **5.1** |

One unit is 126.6 KB of held-out's 151 (a 42 KB encoding table in three apps, labelled another package's
`index.js`). The multi-module class reads the same way: held-out 42.2 KB, the truth's file lacks every unit
(`dcheck2-*.json`); pop-bun 21.2 KB, 18.8 of it the claimer's code.

## 3. The audit: how much "wrong" is the truth's map

`audit.py` reads every wrong-family claim of a label (judge13 rows, all rules, both scopes) against the installed
code: map-defect = the truth's package lacks the unit's code AND the claimer's package holds it (the claim is right;
scorer 13's verdict stands on everything else). An audit column only; the scorer is unchanged.

| set                   | wrong KB | map-defect KB | claimer lacks the code (real wrong) | both hold (copy / carrier) | under 3 tokens / not installed |      P |  audited P |
| --------------------- | -------: | ------------: | ----------------------------------: | -------------------------: | -----------------------------: | -----: | ---------: |
| heldout (exp135-prod) |    704.2 |     **314.1** |                                93.6 |                       90.0 |                          206.5 | 99.565 | **99.759** |
| pop-bun               |  1,277.1 |         148.2 |                               101.7 |                      404.8 |                          622.4 | 99.495 |     99.554 |
| pop-esbuild           |  1,799.4 |         100.0 |                               133.8 |                      925.0 |                          640.7 | 99.304 |     99.343 |
| remin-esbuild         |    160.0 |           3.2 |                                 4.0 |                       84.7 |                           68.1 | 99.691 |     99.697 |
| esbuild-app           |    131.1 |           0.0 |                                19.7 |                       82.8 |                           28.5 | 99.066 |     99.066 |

The defect is Bun's (held-out and pop-bun are Bun-built; esbuild-built sets carry little). A truth-lane fix (scorer
14: such a unit is the claimer's) would move held-out precision by +0.19 at no pipeline change, more than any lever
in this campaign. test / test2 cannot be audited (frozen).

## 4. Refuted: an index-side name-evidence check

The same defect sits in the index (Bun's map files one package's code under another package's file). A builder check
was built (`--unit-evidence`, commit `47c644884`) and simulated before any rebuild (`evsim.py`: every index seed of
the base labels; a seed is removed when all the claimer's rows of the print sit in files that lack the unit's
evidence AND the claimer's installed package, all files together, lacks it):

| set          | right seed bytes removed | wrong seed bytes removed |
| ------------ | -----------------------: | -----------------------: |
| heldout      |                 170.3 KB |                  68.0 KB |
| pop-bun      |                    414.9 |                     38.2 |
| pop-esbuild  |                     43.7 |                      4.5 |
| apps-esbuild |                     56.2 |                      0.0 |
| apps-rollup  |                    186.2 |                      8.8 |
| apps-webpack |                     17.6 |                      1.1 |

A unit's strings come from inlined JSON data (a CSS grammar from a dependency's data file), regex sources and
dependencies, so "the package's files hold its names" fails on right claims far more often than it catches wrong
ones. The `multi-module` variant (keep the module a multi-credited file's text holds) recovers 0 of the census class
(section 2). Both removed in `7bd37903a`; the simulation scripts stay.

## 5. The static entry, built and scored

`[recipe] static` (crates `humanify-core::libindex::recipe`, `humanify-index::build`): a recipe may carry `static`
(any bundler); its slice builds a third entry `entries/sta.mjs` (`import * as mN from "<subpath>"` for every subpath
the `require()` entry requires, re-exported). The slice name is unchanged; the recipe spelling (shard keys, manifest)
changes; a build's stamp spells the recipe WITHOUT `static`, so the `req` / `dyn` builds are reused. Tests (red
first): `a_static_entry_recipe_reads_back_and_keeps_its_slice` (core),
`a_static_entry_recipe_builds_a_third_entry_importing_every_subpath_statically`,
`the_literal_shard_reads_the_static_entrys_build_only_for_a_static_recipe` (builder).

**Index `k2-all-r5-origin-prod-static`** (`/work/exp136-data/full/ix/k2-all-r5-origin-prod-static`, 6.7 GB, id
`c7e48b76…`): exp135's build with `static` on both Bun recipes and the esbuild recipe (`scripts/build-static.sh`;
store and work dir hard-link copies of exp135's, `flock /work/index-build.lock`, cores 34-63, 124 chunks all exit
0, 17,680 s shards + 1,448 s merge at 36.6 GB peak; /work stayed above 840 GB free). Built by the builder at
`47c644884` with `--unit-evidence` off (byte-identical shards to `7bd37903a`'s builder: the off path and keys are
unchanged). The rollup / webpack tables are byte-identical to the base's (their scores below are identical: the
control).

Main defaults, structural rule, scorer 13, both scopes; base = `*exp135-prod*`, new = `*exp136-static*`; test and
test2 summaries only:

| set           | k2-all-r5-origin-prod P / R / F0.5 | k2-all-r5-origin-prod-static P / R / F0.5 |        dP |     dF0.5 |         wrong KB |
| ------------- | ---------------------------------: | ----------------------------------------: | --------: | --------: | ---------------: |
| cc            |             100.00 / 81.90 / 95.77 |                    100.00 / 84.63 / 96.49 |     +0.00 | **+0.73** |       0.0 -> 0.0 |
| esbuild-app   |              99.07 / 74.74 / 93.01 |                     99.07 / 74.74 / 93.01 |     +0.00 |     +0.00 |   131.1 -> 131.1 |
| heldout       |              99.56 / 68.51 / 91.29 |                     99.53 / 68.53 / 91.27 | **-0.03** |     -0.01 |   704.2 -> 758.2 |
| wrangler      |              99.07 / 33.14 / 70.87 |                     99.09 / 34.01 / 71.67 |     +0.03 | **+0.80** |       5.8 -> 5.8 |
| apps-esbuild  |              99.89 / 77.65 / 94.48 |                     99.89 / 77.67 / 94.48 |     +0.00 |     +0.01 |   135.0 -> 135.0 |
| apps-rollup   |              99.09 / 79.31 / 94.38 |                     99.09 / 79.31 / 94.38 |     +0.00 |     +0.00 |        identical |
| apps-webpack  |              99.58 / 73.50 / 92.98 |                     99.58 / 73.50 / 92.98 |     +0.00 |     +0.00 |        identical |
| dev-rollup    |              99.87 / 77.53 / 94.43 |                     99.87 / 77.53 / 94.43 |     +0.00 |     +0.00 |        identical |
| dev-webpack   |              99.64 / 62.89 / 89.22 |                     99.64 / 62.89 / 89.22 |     +0.00 |     +0.00 |        identical |
| pop-bun       |              99.49 / 77.19 / 94.06 |                     99.49 / 77.40 / 94.11 |     -0.01 |     +0.05 | 1277.1 -> 1302.2 |
| pop-esbuild   |              99.30 / 78.71 / 94.37 |                     99.30 / 78.61 / 94.33 |     -0.00 |     -0.03 | 1799.4 -> 1808.3 |
| remin-esbuild |              99.69 / 51.76 / 84.11 |                     99.65 / 51.59 / 84.00 | **-0.04** |     -0.11 |   160.0 -> 181.8 |
| remin-terser  |              99.90 / 20.33 / 56.04 |                     99.90 / 20.09 / 55.67 |     -0.00 |     -0.37 |     18.6 -> 18.6 |
| test-bun      |              99.55 / 70.16 / 91.85 |                     99.51 / 70.04 / 91.79 | **-0.04** |     -0.07 |  953.8 -> 1029.6 |
| test-esbuild  |              99.26 / 73.37 / 92.72 |                     99.26 / 73.23 / 92.67 |     -0.00 |     -0.04 | 1740.0 -> 1737.0 |
| test-rollup   |              98.96 / 72.43 / 92.21 |                     98.96 / 72.43 / 92.21 |     +0.00 |     +0.00 |        identical |
| test-webpack  |              99.48 / 72.19 / 92.49 |                     99.48 / 72.19 / 92.49 |     +0.00 |     +0.00 |        identical |
| test2-bun     |              99.26 / 69.37 / 91.38 |                     99.20 / 69.39 / 91.35 | **-0.07** |     -0.04 | 1269.5 -> 1384.9 |
| test2-esbuild |              99.63 / 71.04 / 92.21 |                     99.63 / 71.10 / 92.23 |     +0.00 |     +0.02 |   723.1 -> 717.1 |
| test2-rollup  |              96.35 / 75.65 / 91.35 |                     96.35 / 75.65 / 91.35 |     +0.00 |     +0.00 |        identical |
| test2-webpack |              99.27 / 70.09 / 91.64 |                     99.27 / 70.09 / 91.64 |     +0.00 |     +0.00 |        identical |

App claimed 0 everywhere. **Ship bar (precision >= base on every set, F0.5 >= base on held-out, pop, test, test2):
NOT met.** Read against the audit (section 3; dev sets only):

| set           | audited P base -> static | real wrong (claimer lacks) | copy / carrier (both hold) |
| ------------- | -----------------------: | -------------------------: | -------------------------: |
| heldout       |         99.759 -> 99.762 |               93.6 -> 92.5 |               90.0 -> 87.9 |
| pop-bun       |         99.554 -> 99.561 |             101.7 -> 104.0 |             404.8 -> 394.0 |
| pop-esbuild   |         99.343 -> 99.339 |             133.8 -> 133.8 |             925.0 -> 934.2 |
| remin-esbuild |         99.697 -> 99.655 |                 4.0 -> 5.5 |              84.7 -> 102.6 |

Held-out's +54 KB wrong is ONE 19.5 KB data module in three apps (a spinners table) the static build lets its own
package claim — right; the truth's map credits it to another package. On the esbuild slice the extra prints add
carrier claims (pop-esbuild, remin-esbuild) and more owners per print (recall down on pop-esbuild, test-esbuild,
remin). test-bun / test2-bun cannot be read (frozen). **Not recommended.** A Bun-only static slice would keep cc
+0.73 and wrangler +0.80 but still lose test-bun / test2-bun precision under scorer 13; revisit after the truth fix.

## Reproduce

```bash
S=experiments/136-lookup-misses/scripts; N=experiments/135-noprint-census/scripts; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
ST="--store /work/exp135-data/full/store/shards --work /work/exp135-data/full/work"
# census (section 1) and its f / d detail (section 2); test: --private (aggregates only)
$PY -I $N/noprint.py dev - c0 exp135-prod out/dev $ST --detail --side > dev.txt     # apps/bundlers/pop/remin likewise
$PY -I $N/noprint.py test - c0 test-exp135-prod out/test --private $ST --detail > test.txt
python3 -I $N/summarize.py dev.txt ... --json census-new.json; python3 -I $S/fd_table.py fd-table.json dev.txt ...
# build shapes (static / ESM entries) of the truth versions, d / multi-module against installed code
python3 -I $S/variants.py variants-dev.json out/dev.side.jsonl out/pop.rows.jsonl ...
$PY -I $S/dcheck.py dev exp135-prod out/dev.side.jsonl --classes c-dropped,d-print-differs,f-filtered --index
$PY -I $S/whohas.py dev exp135-prod out/dev.side.jsonl
# audit (section 3) and the refuted evidence check (section 4), from judge13 rows
$PY -I experiments/133-dl25k-r5/scripts/judge13.py dev j.jsonl exp135-prod exp136-static
$PY -I $S/audit.py dev j.jsonl exp135-prod exp136-static; $PY -I $S/evsim.py dev exp135-prod j0.jsonl
# the static index and its scores
$S/build-static.sh /work/exp136-data/full <humanify-index> k2-all-r5-origin-prod-static
$N/score-all.sh exp136-static /work/exp136-data/full/ix/k2-all-r5-origin-prod-static <log dir>
python3 -I experiments/133-dl25k-r5/scripts/table.py exp135-prod exp136-static
```

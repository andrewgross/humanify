# exp128: the scorer's scope gap — bundled CommonJS modules

## STATUS (2026-10-17)

DONE on branch `exp128-cjs-scope` (off main `0e418cb8`; main `ca6353ee` (exp125, scorer 10) and `a72cce9f`
(exp130, index k2-all-r5-origin) merged in). Gate 13/13 bare. Warm neutrality NEUTRAL 0/0, both legs +0, default and report mode (library-report.json identical), vs
`0e418cb8` and again vs `ca6353ee` after the merge. **Not merged** (the coordinator sequences merges).

- **The gap is real and large (rule 8).** Scorers 1-10 judged statement and lazy-body units only. A bundled
  CommonJS module (`__commonJS` in Bun and esbuild) is ONE external unit in discovery's query graph: no print, never
  claimed, and outside every number. It holds most of the library code on every Bun and esbuild set: held-out
  **87%** of library bytes, apps-esbuild 83%, pop 69%, remin 93%, test 79%, test2 83-84%, task-master 74%,
  wrangler 61% (table below). Webpack and rollup sets have none (their modules are already lazy bodies).
- **The shipped pipeline matches none of it.** Discovery never claims a factory: no seed (no print), no module /
  glue rule (external), and the index's whole-module keys (`modules.<slice>.jsonl`, `VENDOR_CONTENT`) are written
  by the builder but read by no lookup. Factory-scope recall is 0.00 on every set. Over both scopes, held-out F0.5
  is 31.98, not 90.79.
- **Scorer 11** (`exp096-score/11`, on top of exp125's 10): factories are units under the same truth rules; the
  headline (`totals`, `f05`) covers both scopes; `scopes.statement` keeps scorers 1-10's scope (history stays
  comparable: with the new truth rule off it equals scorer 10's card exactly, checked on dev incl. held-out and on
  bundlers, `verify11.py --rules-off`), `scopes.factory` is the new one. Plus one truth rule, `notinstalled`
  (frozen layer `chained11`). Every committed label (362, incl. exp130's) re-scored once under scorer 11.
- **The generic fix exists, default OFF: `[library.discovery] factory_bodies = true`.** It cuts each factory into
  its body statements on the query side, the unit model the index has always used (its rows hold factory bodies).
  On the recommended index k2-all-r5-origin, over both scopes: held-out F0.5 32.06 -> 91.15, apps-esbuild
  47.34 -> 95.49, pop 63.9 / 64.8 -> 94.5 / 94.7, test 48.3 / 51.3 -> 90.8 / 93.0, test2 43.2 / 39.8 -> 89.6 /
  91.5, remin 22.5 / 19.6 -> 85.1 / 57.1; app claimed 0 everywhere. It is **not clean on precision**: the factory
  scope reads P 96.3 (test2-bun) to 99.98 (wrangler), under the 99.8 bar on 10 of 12 Bun / esbuild sets; over
  both scopes precision is DOWN against today's (statement-scope) precision on held-out (99.31 -> 98.60), test-bun
  (99.31 -> 98.23), test2-bun (98.85 -> 96.78), test2-esbuild (98.56 -> 97.91), remin-terser and apps-esbuild
  (up on pop, task-master, wrangler, remin-esbuild). So it stays off; a precision lane must clear the factory scope first (section 3).

## 1. The scope, and the fix, per set (scorer 11; k2-all-r5-origin, structural, main defaults)

Base = main's shipped defaults (labels `*exp130-origin*`, factory_bodies off); cand = the same plus
`factory_bodies = true` (labels `*exp128-fbo*`, binary `73415c14`). "factory share" = factory-scope library bytes
/ all library bytes. "all" = the scorer-11 headline (both scopes). test / test2 are aggregates only.

| set           | factory share | statement P / R / F0.5 (base) | all P / R / F0.5 (base) | factory P / R / F0.5 (cand) | all P / R / F0.5 (cand) | statement F0.5 (cand) | app B (cand) |
| ------------- | ------------: | ----------------------------: | ----------------------: | --------------------------: | ----------------------: | --------------------: | -----------: |
| cc            |         0.00% |        100.00 / 81.90 / 95.77 |  100.00 / 81.90 / 95.77 |                           - |  100.00 / 81.90 / 95.77 |                 95.77 |            0 |
| esbuild-app   |        73.78% |         96.04 / 69.20 / 89.13 |   96.04 / 18.15 / 51.67 |       99.97 / 90.23 / 97.86 |   99.10 / 84.67 / 95.83 |                 89.13 |            0 |
| heldout       |        87.25% |         99.31 / 67.82 / 90.87 |    99.31 / 8.65 / 32.06 |       98.51 / 70.52 / 91.26 |   98.60 / 69.98 / 91.15 |                 90.32 |            0 |
| wrangler      |        61.43% |         96.45 / 21.00 / 56.12 |    96.45 / 8.10 / 30.31 |       99.98 / 46.19 / 81.09 |   99.15 / 36.26 / 73.61 |                 56.12 |            0 |
| apps-esbuild  |        83.49% |         99.77 / 92.44 / 98.21 |   99.77 / 15.26 / 47.34 |       99.64 / 79.61 / 94.87 |   99.67 / 81.76 / 95.49 |                 98.21 |            0 |
| apps-rollup   |         0.00% |         99.06 / 79.28 / 94.35 |   99.06 / 79.28 / 94.35 |                           - |   99.06 / 79.28 / 94.35 |                 94.35 |            0 |
| apps-webpack  |         0.00% |         99.39 / 73.36 / 92.80 |   99.39 / 73.36 / 92.80 |                           - |   99.39 / 73.36 / 92.80 |                 92.80 |            0 |
| dev-rollup    |         0.00% |         99.87 / 77.53 / 94.43 |   99.87 / 77.53 / 94.43 |                           - |   99.87 / 77.53 / 94.43 |                 94.43 |            0 |
| dev-webpack   |         0.00% |         99.64 / 62.89 / 89.22 |   99.64 / 62.89 / 89.22 |                           - |   99.64 / 62.89 / 89.22 |                 89.22 |            0 |
| pop-bun       |        69.18% |         98.79 / 85.79 / 95.88 |   98.79 / 26.44 / 63.85 |       99.43 / 76.27 / 93.74 |   99.22 / 79.26 / 94.46 |                 95.90 |            0 |
| pop-esbuild   |        69.28% |         98.59 / 88.93 / 96.49 |   98.59 / 27.32 / 64.79 |       98.93 / 77.83 / 93.84 |   98.81 / 81.31 / 94.73 |                 96.51 |            0 |
| remin-esbuild |        92.95% |         99.04 / 78.17 / 94.02 |    99.04 / 5.51 / 22.53 |       99.33 / 52.23 / 84.16 |   99.30 / 54.11 / 85.09 |                 94.02 |            0 |
| remin-terser  |        92.34% |         99.82 / 60.64 / 88.40 |    99.82 / 4.65 / 19.59 |       99.30 / 17.73 / 51.72 |   99.41 / 21.15 / 57.13 |                 88.55 |            0 |
| test-bun      |        78.58% |         99.31 / 73.79 / 92.89 |   99.31 / 15.80 / 48.29 |       97.91 / 68.61 / 90.21 |   98.23 / 69.73 / 90.81 |                 92.89 |            0 |
| test-esbuild  |        78.84% |         99.10 / 82.82 / 95.35 |   99.10 / 17.53 / 51.33 |       99.10 / 72.30 / 92.26 |   99.10 / 74.58 / 92.99 |                 95.35 |            0 |
| test-rollup   |         0.00% |         98.68 / 72.23 / 91.94 |   98.68 / 72.23 / 91.94 |                           - |   98.68 / 72.23 / 91.94 |                 91.94 |            0 |
| test-webpack  |         0.00% |         99.48 / 72.19 / 92.49 |   99.48 / 72.19 / 92.49 |                           - |   99.48 / 72.19 / 92.49 |                 92.49 |            0 |
| test2-bun     |        83.33% |         98.85 / 79.55 / 94.28 |   98.85 / 13.26 / 43.15 |       96.29 / 66.79 / 88.48 |   96.78 / 68.95 / 89.55 |                 94.26 |            0 |
| test2-esbuild |        84.35% |         98.56 / 75.07 / 92.75 |   98.56 / 11.75 / 39.77 |       97.79 / 72.10 / 91.28 |   97.91 / 72.58 / 91.52 |                 92.77 |            0 |
| test2-rollup  |         0.00% |         96.34 / 75.65 / 91.35 |   96.34 / 75.65 / 91.35 |                           - |   96.34 / 75.65 / 91.35 |                 91.35 |            0 |
| test2-webpack |         0.00% |         99.27 / 70.09 / 91.64 |   99.27 / 70.09 / 91.64 |                           - |   99.27 / 70.09 / 91.64 |                 91.64 |            0 |

The same pair on the previous default k2-all-r5 (labels `*exp124-r5*` vs `*exp128-fb2*`, binary `32c25969`, also
scorer 11): factory-scope numbers within 0.1 pt of these; held-out all-scope 31.98 -> 91.14, apps-esbuild
47.16 -> 95.41, test2-bun 43.06 -> 89.54.

Read the table this way: the statement scope (scorers 1-10's whole number) is unchanged by the key within a few
hundredths except held-out (90.87 -> 90.32: the cut moves module groups the presence rules read); the factory
scope goes from recall 0 to 52-90%; the headline therefore moves by 30-60 points on every Bun / esbuild set.

## 2. Scorer 11

- `score.py`: `factory_units` (FactoryBody rows, else an uncut Factory as one unit), `scored_units` = statement
  scope + factory scope (disjoint). `label_units` labels both (majority-mapped package, origin layers, copies,
  carried); claims are matched by span as before. A card gains `scopes.{statement,factory}` (bytes, P/R/F0.5/F1,
  shipper-only, miss bytes); the summary per set too; an unclaimed uncut factory's miss class is `factory` (it has
  no print to classify). Claude Code's labelled sample has statement units only: its factory scope is reported as
  not judged. A floored bundle keeps all its mapped bytes in the statement scope, as before.
- Rule `notinstalled` (`truth_audit.RULES11`, layer `chained11`, built by `scorer11/chained11.py`): scorer 9's
  `npmchain` refused to relabel a chained source whose path names a REAL npm name (`node_modules/src/...`,
  `node_modules/packages/...`). Judging factories exposed it: async-validator's map lists `../src/<x>.ts`,
  typeorm's `../../src/...` (esbuild resolved its `sourceRoot` against the map file, one level up), Angular 5's
  `packages/...`. The truth labelled 713 KB of typeorm and 391 KB of @angular/compiler `src` / `packages`, and the
  right claims read as wrong. New condition: the labelled package is NOT INSTALLED in the app (no
  `<node_modules>/P/package.json`), the path is not on disk, and an installed package's own map lists it (spec
  resolution, plus the map-file-relative `sourceRoot` reading). Second case, same shape: a URL source
  (`raw-loader:/home/runner/work/custom-electron-titlebar/...css?raw`) the truth gave `<app>` and a package's map
  lists verbatim takes that package (the 8,887 "app claimed" bytes on pop-esbuild). Relabelled sources: apps-esbuild
  1,026, pop-esbuild 1,683, remin 602 + 590, test-esbuild 1,859, test2-esbuild 577, task-master 6; none on any Bun,
  webpack or rollup set.
- `leaderboard.py` prints both scopes under each row; `scope.py` the table above; `eval.ts`'s `cannot prove` text
  no longer says modules are unscored.

## 3. The pipeline today, and `factory_bodies`

Shipped: factory scope recall 0 on every set (no claim lands on a factory span; the report's unitRows carry no
print for one).

`library.discovery.factory_bodies` (default false; `DiscoveryOptions::factory_bodies`):

- `graph::references::build_reference_graph_with(.., factory_bodies)`: `container_units(.., true)` on the query
  side; body statements are `FactoryBody` units, the factory a wrapper with `LazyWrapper` edges to them (not
  external when cut: `is_cut_factory`); edges between a body and its OWN wrapper (reads of the factory's
  `exports` / `module` / `require` parameters) are dropped, so a body statement does not depend on its whole
  module (the caller's `require_x()` does).
- discovery treats `FactoryBody` as a lazy body everywhere it asks (is_statement, groups, glue, banners), and its
  prints are cut the same way (`unit_rows(.., factory_bodies)`).
- Two linear-time rewrites the cut made necessary, SAME answers (default-mode neutrality is byte-identical, and the
  `factory_bodies` reports were compared before / after: identical): the module rule's per-member closure search
  became one reverse pass per group (`closed_members`); the candidate queue's closure counts bytes when a unit is
  first reached (stops at the cap at once) and, with cut factories only, shares a region between candidates that
  reach each other (a module body whose statements all read one binding all of them write is a 6,455-statement
  clique; one pop bundle went from >1 h to 35 s).

Lab results: section 1.

What the factory scope's wrong bytes are (dev diagnosis only: apps-esbuild, pop; `wrong.py`): the same classes the
statement scope already has — sponge / pre-bundled copies (rollup's dist inlining yargs-parser, debug vs ms,
pdfjs-dist inside @pdfslick/react), forks (react-dom vs @hot-loader/react-dom, expr-eval vs expr-eval-fork,
pngjs vs pngjs-nozlib), and on Bun a TRUTH defect: Bun 1.4's source map misattributes whole modules in large bundles
(an iconv-lite big5 table mapped to `unpipe`, `file-type`, `text-encoding`), which the index builder already works
around (`shard.rs`). A truth rule for it (a unit whose long strings appear in another installed source and in none
of its labelled one) is the obvious next scorer fix; the matcher-side next step is running exp114 / exp121's
presence rules over factory groups (they read module groups, which now include factory bodies, but were tuned on
statement-scope data).

## 4. Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
for c in dev apps bundlers pop remin test test2; do $PY -I experiments/128-cjs-scope/scorer11/chained11.py build --corpus $c; done
$PY -I experiments/120-truth-families/rescore_many.py --corpus C --cache <scratch> <labels...>
python3 -I experiments/128-cjs-scope/scope.py --md exp124-r5:exp128-fb2 ...
$PY -I experiments/128-cjs-scope/verify11.py --rules-off <scratch> [--corpus C] <label>
C="--index /work/exp124-data/full/ix/k2-all-r5 --rule structural --cpus 0-11 --jobs 4"
npm run eval -- library exp128-fb2 $C --pipeline-set library.discovery.factory_bodies=true   # + --corpus ...
$PY -I experiments/128-cjs-scope/wrong.py exp128-fb2-apps --corpus apps --sets apps-esbuild   # dev only
$PY -I experiments/128-cjs-scope/stmtdiff.py exp124-r5 exp128-fb2 --sets esbuild-app          # dev only
```

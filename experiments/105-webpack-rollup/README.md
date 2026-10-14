# exp105: library matching on webpack and rollup bundles

## STATUS (2026-10-14)

- **State:** DONE, branch `exp105-webpack-rollup` @ the commit carrying this README (code at `f0d220c5`, merged with
  main `5a04e816`/scorer 4 at `c3e64c90`), NOT merged. Gate ALL 13 STAGES PASSED (bare, at `c3e64c90`). Warm
  neutrality vs main `6842df59` on 2.1.215->2.1.216: NEUTRAL, 0 files / 0 lines, cache candidate +0 / baseline +0,
  at `12b14647` and again at `f0d220c5` (main changed no `crates/` since). Default-path code changed (detection,
  toolchain, the unit cut and locator), and Claude Code's bytes did not move.
- **Totals first.** webpack and rollup go from no discovery to a working matcher. TEST split (per-set summaries
  only; never tuned on), scorer 4, vs `test-step4f-*`:

  | set          | structural P / R / F1                     | agreement P / R / F1                       | app claimed (struct / agr) |
  | ------------ | ----------------------------------------- | ------------------------------------------ | -------------------------- |
  | test-webpack | 0.0 / 0.0 / 0.0 -> 85.4 / 34.0 / **48.6** | 0.0 / 0.0 / 0.0 -> 95.4 / 23.0 / **37.1**  | 0 -> 3,962 B / 0 -> 611 B  |
  | test-rollup  | 9.9 / 0.1 / 0.1 -> 75.7 / 28.6 / **41.6** | 17.4 / 0.0 / 0.1 -> 86.9 / 15.6 / **26.5** | 76 -> 248 B / 72 -> 242 B  |
  | test-bun     | 52.7 / 14.1 / 22.2 -> 52.8 / 14.1 / 22.3  | 58.6 / 12.9 / 21.1 -> 58.7 / 12.9 / 21.2   | 0 / 0                      |
  | test-esbuild | 51.4 / 14.6 / 22.7 (unchanged)            | 58.9 / 12.0 / 19.9 (unchanged)             | 13,806 / 0 (unchanged)     |

  DEV-SIDE builds (corpus `bundlers`, 13 webpack + 14 rollup builds made here, index `step4-wr`):

  | set         | structural P / R / F1 | agreement P / R / F1 | app claimed |
  | ----------- | --------------------- | -------------------- | ----------- |
  | dev-webpack | 99.5 / 59.2 / 74.2    | 99.5 / 45.3 / 62.2   | 0           |
  | dev-rollup  | 99.4 / 70.0 / 82.1    | 99.4 / 45.0 / 61.9   | 0           |

  The exp096 DEV corpus (`exp105-structural` / `exp105-agreement` vs `step4f-*`, scorer 4) is unchanged at one
  decimal on every set (CC 100.0/70.6/82.8, held-out 96.6/31.4/47.4, wrangler 95.1/14.8/25.6, task-master
  98.0/40.5/57.4 structural); the two held-out apps that hit the Bun adapter's one-file floor now run discovery
  (+1.4 KB named, measured on step 4, scorer 3).

- **Caveats.** The test split's app-claimed bytes on webpack/rollup are NOT zero (3.9 KB structural on webpack).
  Not read per app (that would spend the test set); test-esbuild already claims 13.8 KB on main, and test precision is
  dominated by truth gaps (STATE.md: judge test precision by delta). Dev builds claim 0 app bytes. Before a default
  flip, the app-claim cause needs finding on DEV-side builds of bigger, real apps (ours are small).

## What this adds

Before: minified webpack 5 output kept none of webpack's runtime names, so detection said `unknown`, the unpack fell
to Bun's one-file floor and no library discovery ran (test-webpack P/R/F1 0/0/0). Rollup had no detector, no grammar
and no index slice (test-rollup recall ~0.1%, read through the esbuild slice).

1. **Detection by shape** (`detect::signals`). webpack: the bootstrap opening on a module table (`(()=>{var
e={585(e){…` — position matters: an app's own object of functions is never the first statement, a bundled copy of
   a webpack build never opens the bundle), the require function's `M[id](module, module.exports, require)` call,
   webpack 5's export definer. rollup writes no runtime; what it writes is interop: its own `_interopNamespaceDefault`
   and `@rollup/plugin-commonjs`'s `getDefaultExportFromCjs`, `commonjsGlobal`, and two guarded require functions
   (`if(h)return m;h=1`). A text holding another bundler's runtime (a Bun/esbuild module helper, webpack's shapes) is
   never rollup, whatever its modules hold. A rollup build of pure ES modules with no namespace import of an external
   carries nothing rollup-specific and stays `unknown` (the fallback; the corpus runner passes the bundler that built
   each bundle anyway). Dev builds: webpack 13/13, rollup 13/14 (one has a hoisted Bun/esbuild module helper from a
   bundled dependency: the conservative gate leaves it Bun).
2. **Their own module grammars and webpack's layout** (toolchain P3/P9, `modules::inline_modules`). webpack: the
   module table's entries and the `.cw(function(module, exports){…})` CommonJS wrapper are lazy modules (each body
   runs on first require), the entry IIFEs holding concatenated modules are scopes (their statements are cut like
   hoisted ones); the bootstrap IIFE is the container whatever its binding count. rollup: the commonjs plugin's
   guarded require functions (a declaration, an IIFE, an IIFE as a call's argument) are lazy modules; everything else
   is the program's own statements. The SAME cut runs on the index side and the run side. Bun/esbuild units are
   byte-identical (the unit-model pin did not move); a second pin guards the webpack/rollup model. The unit locator
   now handles nesting to any depth.
3. **Discovery without a vendor record.** webpack's adapter is webcrack (a subprocess) and rollup has none
   (passthrough): discovery now runs on the raw bundle for those adapters, and on the Bun adapter's one-file floor
   (a helper-less bundle is still statements). Report-only, as before.
4. **webpack module ids are blurred in unit prints.** webpack hashes a module's path into an id range sized by the
   BUILD's module count (5% fill), so one module gets another id in an app than in an index build of its package:
   every require call's id prints as 0 under the webpack grammar only.
5. **Index recipes** `webpack:target=node,format=cjs,minify` (webpack 5.111.1 production + terser, one chunk) and
   `rollup:target=node,format=cjs,minify` (rollup 4.64.5 + node-resolve + commonjs + json + terser) — exp100's test
   recipes, as Node scripts (`crates/humanify-index/recipes/`, `--webpack` / `--rollup`).
6. **Harness:** a third corpus, `bundlers` (dev-side builds made here: 14 apps × webpack/rollup from step-4
   packages, `scripts/dev_apps.py`); `run.py` passes each bundle's `builtWith` as `--bundler` (exp100 recorded
   webpack/rollup bundles as `bundler: esbuild` only because the binary had no slice for them).

## Results

- **Index** `/work/exp105-data/ix/wr-asof` (164 MB, id `552c9c6e`): the step-4 base list (1,821 roots) at the `as-of`
  policy (latest, previous patch, two previous majors; no pins) = 5,503 versions, 5,379 shards per slice; 10,612 of
  10,758 webpack entry builds and 10,690 of 10,758 rollup builds succeeded; 297k / 274k unit rows. 30 minutes on 30
  cores (installs hard-linked from index v2's work tree, `link-installs.sh`). Merged with step 4 into
  `/work/exp105-data/ix/step4-wr` (667 MB, id `2cbffa3b`; `merge_index.py`: step 4's tables hard-linked, the new
  slices appended, step 4's literals).
- **Detection on the dev builds:** webpack 13/13, rollup 13/14 definitive (the miss holds a hoisted Bun/esbuild
  module helper from a bundled dependency: the gate that keeps rollup's shapes from ever outranking another bundler's
  runtime leaves it Bun — the unsure fallback).
- **Dev misses** (structural): webpack `e-spec` 11.5%, `b` 10.0%, `d-file` 9.9%; rollup `e-spec` 10.5%, `b` 7.9%.
  `b` (version missing) is the `as-of` policy's thinner version set; `e-spec` is the specificity rule (index module ≤
  4× the query + 4 KB), which webpack's/rollup's module grouping reads differently from Bun's.
- **Labels:** `exp105-dev-{structural,agreement}` (corpus bundlers, cards committed), `exp105-{structural,agreement}`
  (dev corpus), `test-exp105-{structural,agreement}` (test, summary only). Build commit `c3e64c90`.

## What remains

1. **App claims on test webpack/rollup** (3.9 KB / 0.2 KB): find the cause on bigger dev-side real-app builds
   (never on the test cards).
2. **Version density:** build the webpack/rollup slices at `year-all+minor` + pins like step 4 (67.5k shards; ~4×
   this build) — `b` is ~8-10% of dev misses.
3. **Specificity for module grammars:** `e-spec` is the largest dev miss class on both; the query-side module size
   for a webpack table module or a rollup guarded module is its own body, worth checking against the index side's.
4. **Detection gaps:** a pure-ESM rollup build with no namespace import has no rollup-specific bytes (stays
   unknown); webpack 4's array table and `n.t`/`n.e` module ids are not cut / blurred.
5. **The pipeline side:** webpack still unpacks through webcrack and rollup through passthrough; only discovery
   (report-only) uses the new grammars. A native webpack unpack (modules as vendor files) is the next step for
   naming.
6. **Copies layer for `bundlers`** was built with `copies.py --jobs 1` (the pool hit a pickling error at 8 jobs).

## How to reproduce

```bash
D=/work/exp105-data
experiments/105-webpack-rollup/scripts/link-installs.sh $D/versions-asof.txt /work/exp095-index-v2-data/selABC/work $D/work
experiments/105-webpack-rollup/scripts/build-slices.sh $D wr-asof 34-63 <humanify-index> /work/exp097-data/base-step4.txt \
  2026-10-11 as-of -- --registry /work/exp097-data/registry --recipe webpack:target=node,format=cjs,minify \
  --recipe rollup:target=node,format=cjs,minify --webpack $D/tools/webpack-build.cjs --rollup $D/tools/rollup-build.mjs
python3 -I experiments/105-webpack-rollup/scripts/merge_index.py /work/exp097-data/ix/step4 $D/ix/wr-asof $D/ix/step4-wr
<exp095 venv python> -I experiments/105-webpack-rollup/scripts/dev_apps.py build && … freeze
npm run eval -- library exp105-dev-structural --index $D/ix/step4-wr --rule structural --corpus bundlers
```

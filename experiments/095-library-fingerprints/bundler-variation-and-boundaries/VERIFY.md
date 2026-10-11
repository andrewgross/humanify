# VERIFY — adversarial review of "bundler-variation-and-boundaries"

Reviewer's re-measurements are in `verify/` (scripts + JSON outputs). Nothing in the humanify repo was touched.

## Bottom line

The method holds up on precision: exact hashes of normalised functions and statements, plus growth along emitted
order. Three of the headline numbers are rosier than the data supports:

1. **"Function match" percentages are counted per function, and nested functions are included.** Weighted by code
   size, the matches are much lower. Bun vs esbuild is 93.7% by count but **70.0% by tokens, and 62.2% when only
   outermost functions are counted**. Bun vs terser (masked) is 72.2% by count but 34.0% by tokens and **9.3%** on
   outermost functions. A minor library version step is 88.5% by count but **63.8%** by tokens.
2. **"98.0% recall with an index built only from other bundlers" is really "an index built from other Bun versions".**
   After normalisation, Bun 1.3.14 is 100% identical to 1.4.0, so the index effectively holds the same bundler. With a
   genuinely different bundler (an esbuild-only index), recall on Bun is 86.5–88.8%. With the recommended setting it
   also **flags 553 of 9,525 lab app bytes (5.8%)**.
3. **The lab queries' exact library versions are in the index.** The combo apps use the "latest" versions, and those
   are also single-package apps. If those exact versions are dropped, so that only the earlier patch, minor or major
   versions remain, recall falls from **98.0% to 86.4%**. With an esbuild-only index it falls to **77.6%**.

Upheld:

- 0 app bytes flagged on Claude Code at the recommended setting. This also holds on a **held-out** set 4–8× larger.
- The MSAL ground truth: 94.3% recall with exact versions, 0 bytes labelled MSAL outside MSAL.
- Raw text and module-root hashes are fragile.
- The index size is small.

## What I re-ran

| check                                                                                       | result                                                                                |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| `selftest.mjs`                                                                              | 20/20 SAME                                                                            |
| `variation.py` (from the same scan data)                                                    | `results/variation.json` **byte-identical**                                           |
| `cc_exact.py`                                                                               | `results/stepb-cc-exact.json` **byte-identical** (MSAL 13.7% → 94.3%, 0 outside)      |
| recommended config, headline index, Bun 1.4.0 combos (control in `verify/version_drift.py`) | 97.99% recall, 99.95% right package, 0/9,525 app bytes. Reproduces `stepb-final.json` |

## 1. Precision on Claude Code app code that was not used for tuning — UPHELD

The content rule (a statement-level hit needs at least 3 content tokens) was chosen _because_ the original 100
negatives showed 696 / 215 false bytes. So "0 bytes after the rule" was measured on the tuning set.

`verify/heldout_negatives.py` builds held-out sets from all lazy-ESM groups in 2.1.216, excluding the 100 picked
negatives, with no size cap, no vendor-literal filter and no `/*!` banner:

| set                                 | groups | KB    | recommended: flagged bytes | without content rule                                |
| ----------------------------------- | ------ | ----- | -------------------------- | --------------------------------------------------- |
| H2: at least 2 product-string kinds | 183    | 4,129 | **646 (0.016%)**           | 1,620                                               |
| H1: at least 1 product-string kind  | 858    | 7,331 | **690 (0.009%)**           | 7,896 (6.4 KB of it is ink `A(tt(),1)` import glue) |

What the 690 bytes are:

- 646 bytes are TypeScript's private-field helpers (`gYt=function(e,t,r,n,o){if(n==="m")throw TypeError("Private
method is not writable")…`). They sit in a group full of `ANTHROPIC_GOOGLE_CLOUD_*` env names, which is almost
  certainly compiled SDK code. The code is library, but the label **"zod"** is wrong.
- 44 bytes are lodash `listCacheClear` (`function mfm(){this.__data__=[],this.size=0}`).

So the rule generalises: no real app code is flagged. One residual: generic compiler helpers get attributed to
whichever package happens to carry them, which is a wrong-package problem.

## 2. "Index from other bundlers" — WEAKENED

`results/stepb-lab.json` and `verify/version_drift.py` both measure Bun 1.4.0 combo queries:

| index                                                  | recall | right package | app bytes flagged      | runtime bytes flagged |
| ------------------------------------------------------ | ------ | ------------- | ---------------------- | --------------------- |
| bun-1.3.14 + bun-1.4.3 + esbuild (headline)            | 97.99% | 99.95%        | 0                      | 2,225 / 6,345         |
| esbuild only, content ≥ 3 on functions too (stepb_run) | 86.53% | 99.92%        | 0                      | 5,012 / 6,345         |
| **esbuild only, RECOMMENDED config** (fn-content ≥ 0)  | 88.75% | 99.70%        | **553 / 9,525 (5.8%)** | 5,012 / 6,345         |

The 553 bytes (`verify/app_fp_text.py`) are combo-5's app functions: `function Ze(e,t){return{event:"labapp_"+e,…}}`,
`class St extends Error{…"[LabApp 5] "…}`, and `Sx`, `Lx` and `Px`. All of them were absorbed into **lodash-es** by
wrapper-group completion (S3). In other words, the recommended config's 0 app bytes depends on which index it runs
against. The `fn-content≥0` choice was itself picked because it raised recall on Claude Code positives
(42.6 → 50.6%), which is tuning on the test set.

**Bundler runtime is labelled as a library** at every setting: 35% of runtime bytes with the headline index and
79% with esbuild only (e.g. combo-1's runtime labelled `marked`, combo-3's labelled `zod`). Group growth absorbs the
runtime into the first library. The score counts only `<app>` as a false positive, so this never shows up in
"app bytes flagged". A shipped detector needs an explicit runtime veto.

## 3. Exact versions in the index — WEAKENED (the recall headline is an exact-version number)

`verify/version_drift.py` drops from the index every unit whose (package, version) appears in the combo queries. Each
package must then be found from its other versions:

| index                                      | recall     | right package | app flagged |
| ------------------------------------------ | ---------- | ------------- | ----------- |
| headline index, exact versions present     | 97.99%     | 99.95%        | 0           |
| headline index, **exact versions dropped** | **86.37%** | 99.09%        | 0           |
| esbuild only, exact versions present       | 88.75%     | 99.70%        | 553 B       |
| esbuild only, **exact versions dropped**   | **77.55%** | 99.06%        | 553 B       |

Packages that drop the most with exact versions removed:

| package              | recall   |
| -------------------- | -------- |
| web-streams-polyfill | 0%       |
| @smithy/core         | 11%      |
| fast-uri             | 15%      |
| picomatch            | 43%      |
| CJS overall          | 99 → 78% |

The report does say "coverage of exact versions is the binding constraint", and the MSAL result (13.7% vs 94.3%)
shows it. But the lab headline of 98% is an exact-version number and should be quoted as one. **Precision does not
degrade with version drift**: right package stays at 99%, and app bytes flagged stay at 0 with the Bun-family index.

## 4. "Function match" weighting — WEAKENED

`verify/nested_fn_check.py` uses the same pairing as `variation.py`: same app, mode, package, version and file.

| pair                                     | published (count, all fns) | tokens, all fns        | tokens, outermost fns only | count, outermost |
| ---------------------------------------- | -------------------------- | ---------------------- | -------------------------- | ---------------- |
| Bun 1.3.14 vs 1.4.0                      | 100.0                      | 100.0                  | 100.0                      | 100.0            |
| Bun 1.4.0 vs 1.4.3                       | 98.9                       | 94.3                   | 92.4                       | 98.6             |
| Bun vs esbuild (L3)                      | 93.7                       | 70.0                   | **62.2**                   | 91.9             |
| Bun vs rollup+terser (L4 masked)         | 72.2                       | 34.0                   | **9.3**                    | 25.0             |
| esbuild vs terser (L4)                   | 73.0                       | 34.8                   | 10.4                       | 26.5             |
| version steps patch / minor / major (L3) | 99.2 / 88.5 / 67.7         | 96.2 / **63.8 / 43.9** | –                          | –                |

The count-weighted figures are dominated by many small functions. They also count small inner functions again when
their enclosing function no longer matches, which matters a lot for terser: its inlining breaks the outer function
and leaves the inner pieces matching. The keyNumbers line does say "(count)", but the verdict table presents these
as plain "function match". By code mass, terser matches about 10% at the top level, so "terser is optional at
+64% index" understates how different it is.

## 5. Ground truth — UPHELD

- **MSAL banner truth.** The exact-version Bun 1.4.0 `dyn` build of msal-node 3.8.1 has 48 msal-node and 60
  msal-common lazy modules, 211.4 KB. Claude Code has 108 banner groups, 208.5 KB. That is the same module count and
  near-identical size, so the truth set is right.
- **Caveat on the 94.3%.** The index then holds a near twin of the query (same bundler, same versions, same
  emission), so 94.3% is the best-case ceiling, not a typical number.
- **Leave-one-out "shared code" confusions are mostly real.** msal-node's `lib/msal-node.cjs` does embed msal-common
  (`class AuthError` is inside it). glob 13 ships `dist/esm/index.min.js`, a bundle that contains minimatch.
- **Small veto leak.** 36 of the 122 lab app statements (534 of 9,525 bytes) are already in the index as
  `<nonlib>`, because the single-package apps share identical app glue. They are vetoed for free. The effect is minor.

## 6. Index size — UPHELD with small corrections

- **Distinct library code.** Distinct Bun 1.4.0 library code in the lab is 13.8 MB (7,722 package/version/file
  triples, largest mode). The claim says 14.1 MB, so about 2.1 fingerprints per KB holds.
- **Size estimate.** The 0.63 MB estimate counts one 12-byte entry per distinct hash. A hash that is shared across
  versions needs one version id per version (105,657 hash-version pairs over all bundlers), so the realistic figure
  is about 0.85 MB.
- **What the count leaves out.** It omits the `<nonlib>` veto table the detector relies on. That table is small,
  but it must ship too.
- **Conclusion unchanged.** Size is not the problem.

## Corrected verdict

The approach is precise. Exact canonical sub-tree hashes produced 0 real app bytes over 7.3 MB of held-out Claude
Code app-like code. The flagged bytes were 690, all compiled library helpers, and some carried the wrong package
name. Recall, however, is an **exact-version, same-bundler-family** number:

| condition                               | recall |
| --------------------------------------- | ------ |
| exact versions, Bun-family index        | 98%    |
| other versions only                     | 86%    |
| exact versions, esbuild-only index      | 87–89% |
| esbuild-only index, other versions only | 78%    |
| one major version off (MSAL)            | 14%    |

On the precision side, two things are still needed:

- the group-growth step must not absorb app or runtime code when the index comes from another bundler: 5.8% of lab
  app code was flagged, plus 35–79% of the runtime;
- the thresholds (`fn-content≥0`, content≥3, support=10) were chosen on the reported test sets and need a held-out
  sweep.

"Function match" by code mass is much lower than the published per-function counts, especially for terser (about
10% at the top level).

## Re-run

```bash
cd /work/exp095-library-fingerprints/bundler-variation-and-boundaries
export PYTHONHASHSEED=0
python3 verify/heldout_negatives.py 2.1.216     # verify/heldout-negatives-2.1.216.json
python3 verify/version_drift.py                 # verify/version-drift.json
python3 verify/app_fp_esbuild_index.py          # verify/app-fp-esbuild-index.json
python3 verify/app_fp_text.py                   # prints the flagged app statements
python3 verify/app_veto_leak.py
python3 verify/nested_fn_check.py               # verify/nested-fn-check.json
python3 scripts/cc_exact.py && cmp results/stepb-cc-exact.json verify/stepb-cc-exact.orig.json
```

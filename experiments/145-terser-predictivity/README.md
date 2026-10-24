# exp145: does dev data predict the terser judge?

## STATUS (2026-10-24)

DONE on branch `exp145-terser-predictivity` (off main `bdd468b5b`). **Harness / analysis only: no crates change, no
key, no default flipped, nothing merged.** DEV corpora only (remin, remin3 and, for one stress check, the other dev
labels' cards). The judge corpora (test, test2, remin2) were not scored, diagnosed or opened; the only judge numbers
used are the remin2-terser precision deltas already quoted in `/work/f1-campaign/STATE.md`.

Question: three terser-side changes in a row (exp140 terser slices, exp141 `terser_anchor`, exp144
`veto_support = shown`) passed precision on dev and then lost it on remin2-terser (36 apps). Was that predictable
from dev, or is a 36-app terser set's precision delta just noise?

**Answer: it was two different things, and neither is "a few dev apps dominate".**

- **exp140 was predictable from dev, and the dev bar we use would have caught it with one more step.** Its judge
  delta (-0.194 / -0.257 points) sits in the MIDDLE of remin3's 36-app subsample distribution (the 47th / 44th
  percentile). Even on remin alone (the only terser dev corpus at the time) the combo read +0.015 overall but 38% of
  36-app draws were below zero (5th percentile -0.067).
- **exp141 and exp144 were NOT predictable by resampling dev apps: the judge result lies outside every one of
  20,000 36-app draws** (remin, remin3 and the pooled 108 apps). Resampling only measures which dev apps you
  happened to draw; it cannot see a mechanism the dev apps do not contain. exp141's rule was also picked among ten
  simulated rules on those same two dev corpora, so its dev reading is optimistic by selection (rule 11's cousin).
- **exp144's terser failure is a resolution problem of the bar, not a terser defect dev could show.** On the two
  terser dev sets exp144 adds 2.1 MB of right bytes and ZERO wrong bytes (108 apps, unit-level, both scorers). Its
  gain is small, so the precision a 36-app terser set can absorb is tiny: (1 - P) x new named bytes per 36 apps =
  **0.9 KB** (on the judge's own numbers, 0.287% x 545 KB = 1.6 KB). One wrong unit bigger than that fails the
  set. Across all 501 dev bundles of exp144, **9 bundles gained at least 0.9 KB of wrong** (none on terser), so a
  36-bundle set drawn like dev contains one with probability **~48%**. exp142's as-of index, the one recent change
  that PASSED on remin2-terser, has the same profile (tolerance 0.2 KB, 8 bundles over it, ~44%): the same coin,
  landing the other way. The judge's +13.6 KB is larger than any single-bundle wrong gain exp144 made on any dev set
  (max 8.3 KB, pop-esbuild), so it is a real error the dev corpora never produced, met in a set that cannot absorb
  it.
- **No guard built.** The generic per-(bundle, family) cap on newly admitted bytes was simulated and is a recall
  dial, not a separator: it drops ~100-5,000 KB of right bytes per KB of wrong on every terser candidate, and on
  exp144 there is no dev wrong to remove (it would only cost recall). exp141's anchor rule already dominates it
  and still failed the judge. Not built, so no neutrality run.
- **Recommended dev bar for terser changes (two parts, both generic):** (1) the 5th percentile of the 36-app
  subsample delta of precision, pooled over remin + remin3 terser (108 apps, without replacement), is >= 0; and
  (2) the candidate's largest single-bundle wrong gain on ANY dev set is smaller than the tolerance a 36-app terser
  set has for it, (1 - P_base) x (new named bytes per 36 pooled terser apps). Against the known judge outcomes:
  **catches all four failures** (exp140 x2, exp141 by part 1; exp144 by part 2) and **rejects the one pass**
  (exp142, by part 2; it was adopted on judge evidence). In-sample on five outcomes, so treat it as a screen, not a
  calibrated rate. Part 2 says plainly when a change is too small for a 36-app terser judge to certify.
- **Gate: 13/13 bare** (see section 6). No crates change -> no neutrality.

## 1. What was compared (dev labels, scorer 14)

| candidate (judge remin2-terser dP, points, quoted) | remin3 base -> candidate                       | remin base -> candidate                      |
| -------------------------------------------------- | ---------------------------------------------- | -------------------------------------------- |
| exp140 terser_slices alone (-0.257 FAIL)           | `remin3-exp141-main` -> `remin3-exp141-terser` | `remin-exp135-prod` -> `remin-exp138-terser` |
| exp140 terser_slices + exp139 keys (-0.194 FAIL)   | `remin3-exp141-main` -> `remin3-exp141-both`   | `remin-exp138-helper` -> `remin-exp139-both` |
| exp141 + `terser_anchor = all` (-0.150 FAIL)       | `remin3-exp141-main` -> `remin3-exp141-x`      | `remin-exp138-helper` -> `remin-exp141-x`    |
| exp144 `veto_support = shown` (-0.069 FAIL)        | `remin3-exp142-asof` -> `remin3-exp144-shown2` | `remin-exp142-asof` -> `remin-exp144-shown2` |
| exp142 as-of index, control (P up, PASS)           | `remin3-exp142-base` -> `remin3-exp142-asof`   | `remin-exp142-base` -> `remin-exp142-asof`   |

The exp141-era labels' cards were gone (reports kept); they were re-scored from their raw reports with `score.py`
(scorer 14, no pipeline run). `remin3-exp141-main` re-scored byte-identical to its committed cards (144/144).

## 2. Per-app deltas and 36-app subsamples

`scripts/all145.py` (per-app deltas from cards; 20,000 draws of 36 apps, without replacement from remin3's 72 and
from the pooled 108, with replacement from remin's 36; seed 145) -> `data/bootstrap.json`. dP in points of
precision; wrong = wrong family + app claimed; top-k = share of the ADDED wrong bytes in the k apps that added most.

| candidate              | set           | apps |     dP | dNamed KB | dWrong KB | top1 | top3 | top5 |    p05 |    p50 | draws < 0 | judge pctl |
| ---------------------- | ------------- | ---: | -----: | --------: | --------: | ---: | ---: | ---: | -----: | -----: | --------: | ---------: |
| exp140 terser alone    | remin3-terser |   72 | -0.234 |    64,652 |     326.2 | 0.38 | 0.62 | 0.73 | -0.423 | -0.234 |     0.989 |      0.437 |
|                        | remin-terser  |   36 | -0.599 |    30,783 |     330.5 | 0.81 | 0.91 | 0.95 | -1.699 | -0.567 |     0.935 |      0.637 |
|                        | pooled terser |  108 | -0.357 |    95,435 |     656.6 | 0.41 | 0.68 | 0.75 | -0.873 | -0.264 |     0.977 |      0.510 |
| exp140 terser + keys   | remin3-terser |   72 | -0.185 |    64,582 |     277.1 | 0.43 | 0.71 | 0.81 | -0.366 | -0.185 |     0.950 |      0.474 |
|                        | remin-terser  |   36 | +0.015 |    30,641 |      22.6 | 0.52 | 0.83 | 0.89 | -0.067 | +0.016 |     0.382 |      0.000 |
|                        | pooled terser |  108 | -0.119 |    95,223 |     299.7 | 0.38 | 0.63 | 0.75 | -0.313 | -0.105 |     0.854 |      0.260 |
| exp141 + anchor        | remin3-terser |   72 | +0.050 |    55,727 |      36.3 | 0.47 | 0.67 | 0.76 | -0.051 | +0.049 |     0.228 |      0.000 |
|                        | remin-terser  |   36 | +0.077 |    27,472 |      -8.6 | 0.28 | 0.64 | 0.92 | +0.021 | +0.074 |     0.005 |      0.000 |
|                        | pooled terser |  108 | +0.058 |    83,199 |      27.7 | 0.42 | 0.60 | 0.68 | -0.031 | +0.053 |     0.151 |      0.000 |
| exp144 shown           | remin3-terser |   72 | +0.006 |     1,423 |       0.0 |    - |    - |    - | +0.001 | +0.006 |     0.000 |      0.000 |
|                        | remin-terser  |   36 | +0.003 |       691 |       0.0 |    - |    - |    - | +0.001 | +0.003 |     0.000 |      0.000 |
|                        | pooled terser |  108 | +0.005 |     2,114 |       0.0 |    - |    - |    - | +0.001 | +0.005 |     0.000 |      0.000 |
| exp142 as-of (control) | remin3-terser |   72 | -0.006 |       397 |       2.8 | 1.00 | 1.00 | 1.00 | -0.017 | +0.000 |     0.499 |          - |
|                        | remin-terser  |   36 | +0.014 |        89 |      -2.7 |    - |    - |    - | +0.000 | +0.014 |     0.000 |          - |
|                        | pooled terser |  108 | +0.001 |       486 |       0.1 | 1.00 | 1.00 | 1.00 | -0.016 | +0.001 |     0.237 |          - |

"judge pctl" = share of the dev draws at or below the quoted judge dP. The esbuild counterparts are in
`data/bootstrap.json`: every candidate moves remin/remin3-esbuild by <= +0.023 points with <= 1.5 KB new wrong;
the terser slices change nothing there (they are read only for terser builds), and exp144 gains 8.4 / 2.7 MB right
there for 1.5 / 0.2 KB wrong.

Reading it:

- The base precisions differ (remin3-terser 99.855, remin 99.902, remin2 99.712 as quoted), so the percentiles
  are approximate; the ordering is not in doubt.
- exp140's judge delta is a typical draw of remin3; nothing about it needed the judge.
- exp141's and exp144's are not draws of any dev distribution. Precision deltas are heavy-tailed (top-5 apps hold
  68-95% of the added wrong bytes wherever there are any), but the tail the judge hit is not in dev's apps at all.

## 3. Concentration of the added wrong bytes (dev)

Per app (cards): see the top-k columns above. Per (bundle, claimed family) EVENT (unit rows from
`experiments/133-dl25k-r5/scripts/judge13.py`, scorer 13 so a few KB off the cards; `scripts/delta145.py`; rows
name apps and stay in `/work/exp145-data/judge`):

| candidate, set               | new wrong KB | events | top1 | top3 | top5 | largest claimed families (KB)                                                                                                                                         |
| ---------------------------- | -----------: | -----: | ---: | ---: | ---: | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| exp140 + keys, remin3-terser |        290.7 |    140 | 0.20 | 0.55 | 0.69 | yargs 59.0 (fork), @bramus/specificity 53.0 (carrier), marked 48.0 (vendored), escope 24.0 (app copy), i18next-fs-backend 16.2, @dual-bundle/import-meta-resolve 14.8 |
| exp140 + keys, remin-terser  |         40.5 |     45 | 0.40 | 0.64 | 0.75 | jju 16.2 (fork), @dual-bundle/import-meta-resolve 4.9, import-meta-resolve 4.7, @vscode/sqlite3 2.6, open 2.4                                                         |
| exp141, remin3-terser        |         67.1 |     83 | 0.36 | 0.59 | 0.70 | escope 24.0, dompurify 10.8, open 6.6, @apollographql/graphql-upload-8-fork 4.7, mermaid 4.1, react-is 3.6                                                            |
| exp141, remin-terser         |          9.2 |     28 | 0.22 | 0.40 | 0.55 | open 2.4, module-alias 2.0, @azure/core-client 1.1, prettier 1.0, got 0.9                                                                                             |
| exp144, remin3/remin-terser  |      **0.0** |      0 |    - |    - |    - | -                                                                                                                                                                     |
| exp144, remin3-esbuild       |          1.5 |     15 | 0.27 | 0.44 | 0.54 | @typespec/ts-http-runtime 0.4, cockatiel 0.3, type-graphql 0.2                                                                                                        |
| exp142, remin3-terser        |          2.8 |      1 | 1.00 | 1.00 | 1.00 | got 2.8                                                                                                                                                               |

Over ALL of exp144's dev bundles (501, every dev label pair): 28.0 KB added in 56 bundles, top 1 / 3 / 5 bundles =
30 / 46 / 55%, the largest 8.35 KB (pop-esbuild), then 2.45 (pop-bun), 1.96, 1.40 (held-out). exp142: 53.4 KB in 12
bundles, top 1 / 3 / 5 = 43 / 91 / 97% (two 23 KB pop events, the ipx / vitepress carrier).

So yes, the tail is heavy — a handful of (bundle, family) copy events carry most of every candidate's new wrong —
and the families are the copy classes exp141 named (forks, carriers, vendored copies, an app's own copy).

## 4. A generic guard, simulated (not built)

`scripts/sim145.py`: drop a candidate's NEW claims of family F in bundle B (units the base did not give F) when they
exceed C bytes and either F had no base claim in B (ratio 0) or they exceed 16x F's base bytes in B. First order
(dropped claims go unclaimed). `data/guard-sim.txt`:

| pair, set (base -> candidate P / F0.5)                | guard                 |      P |  F0.5 | right dropped KB | wrong dropped KB |
| ----------------------------------------------------- | --------------------- | -----: | ----: | ---------------: | ---------------: |
| exp140+keys remin3 (99.855 / 51.24 -> 99.670 / 83.32) | cap 2 KB, new family  | 99.888 | 75.63 |           23,421 |            242.2 |
|                                                       | cap 32 KB, new family | 99.811 | 79.84 |           11,685 |            161.9 |
| exp141 remin3 (-> 99.904 / 80.81)                     | cap 2 KB, new family  | 99.913 | 74.86 |           16,507 |             21.8 |
|                                                       | cap 32 KB, new family | 99.896 | 77.66 |            9,232 |              1.8 |
| exp140+keys remin (99.902 / 56.04 -> 99.917 / 84.97)  | cap 2 KB, new family  | 99.984 | 79.21 |            9,241 |             35.0 |
| exp141 remin (-> 99.978 / 83.19)                      | cap 2 KB, new family  | 99.989 | 78.69 |            6,789 |              5.6 |
| exp144 remin (99.916 / 56.19 -> 99.919 / 57.29)       | cap 2 KB, new family  | 99.919 | 57.26 |               19 |              0.0 |
| exp144 remin3 (99.849 / 51.58 -> 99.855 / 52.81)      | cap 2 KB, new family  | 99.854 | 52.67 |              166 |              0.0 |

It buys precision only by giving back recall at ~100-5,000 KB right per KB wrong; the wrong events are not
distinguishable from the right ones by size or newness of the family. On exp144 it has nothing to remove on dev and
costs 0.03-0.14 F0.5. exp141's anchor rule (8.4 MB right for 205 KB wrong at sim) dominates it and still failed the
judge. Per the brief (build only if it keeps exp144's gains elsewhere AND removes a dev tail exp144 has): no build.

## 5. A dev bar for terser changes

Two generic checks, both computed from dev cards only (`scripts/all145.py`, `scripts/events145.py`):

1. **Subsample check:** p05 of the 36-app dP, pooled remin + remin3 terser (108 apps, 36 drawn without
   replacement, 20,000 draws), >= 0.
2. **Single-event check:** T36 = (1 - P_base) x (new named bytes per 36 pooled terser apps) must exceed the
   candidate's largest single-bundle wrong gain on ANY dev set. T36 is the wrong bytes a 36-app terser set can
   absorb before its precision drops; if the change has already shown it can make one error that big, a 36-app judge
   is a coin flip at best.

| candidate              | pooled p05 | check 1 | T36 KB | largest dev bundle gain KB | check 2 | bar verdict | judge | hit |
| ---------------------- | ---------: | ------- | -----: | -------------------------: | ------- | ----------- | ----- | --- |
| exp140 terser alone    |     -0.873 | fail    |   40.8 |                      276.5 | fail    | reject      | FAIL  | yes |
| exp140 terser + keys   |     -0.313 | fail    |   40.7 |                      120.5 | fail    | reject      | FAIL  | yes |
| exp141 + anchor        |     -0.031 | fail    |   35.6 |                       24.1 | pass    | reject      | FAIL  | yes |
| exp144 shown           |     +0.001 | pass    |    0.9 |                        8.3 | fail    | reject      | FAIL  | yes |
| exp142 as-of (control) |     -0.016 | fail    |    0.2 |                       23.0 | fail    | reject      | PASS  | no  |

- Hit rate on the three named candidates: **3 of 3** (with exp140's terser-alone arm, 4 of 4 failures). The one
  judged pass is rejected (a false alarm). The old bar (dP >= 0 on every dev set) caught only exp140 on remin3.
- Check 1 alone: 3 of 4 failures (misses exp144). Check 2 alone: 3 of 4 (misses exp141). Neither alone suffices.
- In-sample: the bar was chosen after seeing these five outcomes. With n = 5 it is a screen, not a calibrated rate.
  Its value is what check 2 makes explicit: for a small change (T36 under a few KB) a 36-app terser judge cannot
  certify precision either way; exp142 and exp144 are the same coin, P(one event >= T36 in 36 bundles drawn like dev)
  = 44% and 48%. Such a change should be argued on the large sets (held-out, pop, remin3-esbuild) with the terser
  judge read as advisory — that is Andrew's call, not a rule this experiment can set.
- Lever for future terser work: dev cannot reveal a mechanism its apps lack, so the dev pool's breadth is the
  limit. A third terser dev corpus (another seed of the exp126 recipe) would widen it; exp141's rule-picking on both
  dev corpora also argues for fitting on one and barring on the other.

## 6. Gate and reproduction

- Gate: `npm run check` on this branch, bare, 13/13 (log `/work/exp145-data/gate.log`).
- No crates change, no key: no neutrality run.
- Re-scored labels (gitignored cards): `score.py <label> --corpus remin3|remin` for `remin3-exp141-{main,both,terser,x}`,
  `remin-exp138-{helper,terser}`, `remin-exp139-both`, `remin-exp141-x` (`/work/exp145-data/rescore.sh`). The two
  committed labels `remin-exp138-helper` / `-terser` were still scorer-13 cards; their scorer-14 refresh is committed
  here (totals unchanged, only `schema`, the mapdefect layer hash and miss shares move by <= 0.00001).
- Unit rows: `/work/exp145-data/judge.sh`, `judge2.sh` (judge13.py, dev corpora; refuses test splits).
- Numbers: `python3 scripts/all145.py data/bootstrap.json && python3 scripts/events145.py data/bootstrap.json &&
python3 scripts/pctl145.py data/bootstrap.json`; guard: `scripts/sim145.py <rows> <set> <cand summary>
  <base summary> --cap 2,8,32 --ratio 0,16` on the pairs of section 4 -> `data/guard-sim.txt`.

No judge command: no candidate came out of this experiment.

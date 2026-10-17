# exp125: why k2-all-dl25k lost on the exp100 test split — a nested chunk's publisher (scorer 10 `via`)

## STATUS (2026-10-17)

DONE on branch `exp125-dl25k-test` (off main `586cefdba`; main `ebf9a271` and then `0e418cb8` merged in). NOT
merged. The bare gate passed all 13 stages before each commit. Warm neutrality was run against `ebf9a271` (pair
2.1.215 -> 2.1.216, `--priors /work/exp050-cold --cache /work/neutrality-cache`, under
`flock /work/neutrality.lock`, from a clean detached worktree of `e72c75ec`):

- default mode: **NEUTRAL 0 / 0, cache +0 / +0, exit 0 / 0**;
- report mode (`library.matching=report`, `library.index=k2-all`): **NEUTRAL 0 / 0, +0 / +0, exit 0 / 0**.

**What the loss was.** About 93% of the new wrong-family bytes on test-bun and test-esbuild were a **truth gap**, not
a matcher error. The class is a nested chunk:

- A shipper O pre-bundles a package R.
- R's own published file is a chunk named after its origin P (`dist/<P>-<hash>.mjs`), and R declares P as a build
  input.
- Scorer 7's `chunk` rule credits P and drops R, the middle link.
- k2-all-dl25k indexes R, and R holds exactly that file. So the matcher names R.

The same scorer credits R as RIGHT for the same bytes whenever an app ships R's file directly, because R is then the
shipper.

**The fix is the scorer 10 rule `via`** (`origins.py` RULES10, layers `origins10[-<corpus>].json`). A nested chunk
records its region's package R, and a claim naming R's family is judged `origin`. It only moves WRONG -> GOOD:

- On every label of every corpus (313 labels), it moves only dl25k labels, exp124's refuted `orig` labels, and 3.6 KB
  of one apps-rollup unit.
- On every k2-all-family label it moves 0 B, apart from 8 B per test-bun / test-esbuild set on some test labels.

**The test split, after `via` (scorer 10, dl25k + v2 vs k2-all + v2):**

| set          | before (s9)        | after (s10)            |
| ------------ | ------------------ | ---------------------- |
| test-bun     | 84.11 vs 88.96 (-4.85) | **90.72 vs 89.39 (+1.33)** |
| test-esbuild | 87.64 vs 91.49 (-3.84) | **93.38 vs 91.94 (+1.44)** |
| test-rollup  | 89.85 vs 90.12 (-0.27) | **91.72 vs 90.13 (+1.59)** |
| test-webpack | 92.00 vs 91.73 (+0.27) | **93.33 vs 91.78 (+1.55)** |

Test's precision also rises (bun 97.79 -> 98.70, esbuild 96.96 -> 98.36).

**The ship bar is NOT met against the current default k2-all-r5** (exp124; table below). dl25k + v2 + via beats it
on 15 of 19 sets. It loses four, all small recall losses from index growth:

| set          | dF0.5 vs k2-all-r5 |
| ------------ | ------------------ |
| apps-esbuild | **-0.91**          |
| dev-webpack  | **-0.11**          |
| dev-rollup   | **-0.06**          |
| test2-rollup | **-0.29**          |

test2 precision is also lower on two sets:

| set          | dl25k + v2 + via | k2-all-r5 |
| ------------ | ---------------- | --------- |
| test2-bun    | 98.72            | 98.85     |
| test2-rollup | 96.11            | 96.33     |

The coordinator's figures (-0.06 / -0.11 / -0.33 against k2-all + v2) hold here too: -0.06 / -0.11 / -0.33. The rest
of each gap to r5 is r5's own gain, which dl25k (built before unit-rows/5) does not have.

**A second matcher lever, measured and weak:** `[library.lookup] max_packages_by_family` (default off; written red
first). It counts a print's owners by family against `max_packages`.

- On k2-all-dl25k it moves about 0. pop changes by -0.04 / +0.00, bundlers by +0.01 / -0.02, apps by -0.02 / +0.03 /
  +0.02, and test2-rollup gains +0.09.
- The owner overflow it targets (a 622 KB wasm unit whose family holds it alongside one carrier) is refused by the
  isolation veto anyway, because that seed is ambiguous between two families.
- Not recommended. Kept default off.

**Side finding (biggest number here):** under scorer 10, exp124's refuted `--sponge-regions origin` index variant
(labels `*exp124-orig*`) is better than k2-all-r5 on 14 of 19 sets, equal on 4 and lower on 1:

| set          | dF0.5 vs k2-all-r5 | precision        |
| ------------ | ------------------ | ---------------- |
| test-bun     | +3.09              | 99.31            |
| test-esbuild | +3.10              |                  |
| pop          | +4.11 / +4.26      |                  |
| apps-esbuild | +0.44              |                  |
| test2-esbuild | **-0.03**         | 99.46 vs 99.49   |

It was refuted on exactly this truth gap ("test-bun P 97.79 -> 92.59"): its ledger moves 3.26 / 3.29 MB on test and
3.30 / 3.33 MB on pop, WRONG -> GOOD. **Recommend re-judging `origin` mode under scorer 10**, and rebuilding
dl25k with the unit-rows/5 (r5, and possibly origin) builder before re-judging the bigger index. The diagnosis says
the rebuild matters: r5's gains are part of every remaining gap.

**Scorer number:** exp128 also committed a "scorer 10" (CJS wrappers). It had not landed on main at the time of
writing. Whichever lane lands second renumbers.

## 1. Mechanism (aggregate; exp100 TEST split, k2-all-dl25k + v2 vs k2-all + v2)

The exp100 test split was diagnosed in AGGREGATE, as exp112 was allowed to. No app names and no per-app tables are
recorded here. test2 was never diagnosed; only its summaries are used.

The verdicts are score.py's exactly. `judge`, in the lab, re-derives each claim with the scorer 9 layers and
reproduces the cards' wrong-family bytes to the byte: 980,810 / 3,849,275 on test-bun.

**A pitfall.** exp106's `claims.py` (`entry_claims`) skips scorer 5's source patch, scorer 7's origin layer and scorer
6's re-decode. Since scorer 7 its verdicts are stale wherever those layers matter. On test it read 7.7 MB of wrong
for a label whose card says 0.98 MB. Dev labels without lazybody/chunk copies (held-out, apps) are unaffected, but
pop is. **exp121's pop lab numbers were read through it.**

New wrong-family bytes (claims that changed package and are wrong under dl25k):

| class                                                                 | test-bun        | test-esbuild    |
| --------------------------------------------------------------------- | --------------- | --------------- |
| **nested chunk: the claim names the chunk's publisher R (`via`)**     | 3,259,905 (93.4%) | 3,287,730 (92.7%) |
| new index seed on unclaimed code, no origin                            | 65,736 (1.9%)   | 67,142 (1.9%)   |
| seed replacing a good claim, copy with an origin                       | 19,178 (0.5%)   | 63,372 (1.8%)   |
| propagated over a good claim, copy with an origin                      | 76,070 (2.2%)   | 7,019 (0.2%)    |
| propagated into unclaimed code                                         | 35,019 (1.0%)   | 68,440 (2.0%)   |
| other (seed / propagation replacing good or wrong, no origin)          | 35,559 (1.0%)   | 53,396 (1.5%)   |
| **total new wrong**                                                    | **3,491,467**   | **3,547,099**   |

Against that, dl25k removes 0.60 / 1.00 MB of k2-all's wrong. The net is +2.87 / +2.53 MB, which is exp121's
"+2.6 / +2.2 MB".

The nested-chunk units are each bundle's copy of R's 1.6 MB chunk. The bundle's unit equals R's indexed module to
within 0.1%, so R wins the structural rule outright: it covers 266 k of the unit's 372 k nodes, against P's 107 k.

P holds the same code in a module of about the same size, which is why exp121's `copy_presence` (P must hold it in a
module at least 4x smaller) cannot see it. R also shows in the bundle through its own code, so a package-level rule
leaves it. Neither should fire: R is the code's publisher.

**Why "no dev corpus reproduces it" was wrong.** pop carries the identical class: 1,637,099 / 1,650,726 B in pop-bun
/ pop-esbuild, in two training apps. pop still netted a gain under dl25k because the sponge fixes outweighed it. The
held-out, apps and bundlers corpora do not have it: the dev ledger moves 0 B.

## 2. The fix: scorer 10 rule `via` (truth side, developed and checked on pop)

- **`origins.py` RULES10** = RULES7 + `via`. In the nested branch of the `chunk` rule (a marker region naming R
  whose file inside R is a chunk named after P that R declares as a build input), the run's kind is spelled
  `chunk/via:<R>` instead of `chunk`. Nothing else changes, and nothing new is read (the same registry manifests and
  license collections).
- **`score.py`** (`SCORER_VERSION` 10): `label_units` reads `via` from the kind, and `judge_origin` judges a claim
  naming the origin's family OR the via family as `origin`.
- **`truth_audit.py`**: when RULES10 `via` is on, `origin_layer` reads `<DATA>/truth/<id>.origins10.json.gz`, with
  its sha checked against `origins10[-<corpus>].json`.
- **Frozen layers** (`scorer10/origins10.py build`) for all 7 corpora. The check is `origins10.py verify`: with each
  `chunk/via:*` kind read as `chunk`, the layer equals the frozen scorer 7 layer segment for segment.

  | corpus   | entries equal | segments with a via |
  | -------- | ------------- | ------------------- |
  | dev      | 69/69         | 6,882               |
  | apps     | 97/97         | 1,644               |
  | bundlers | 27/27         | 0                   |
  | pop      | 91/91         | 1,083,431           |
  | test     | 191/191       | 1,739,184           |
  | test2    | 188/188       | 0                   |
  | remin    | 72/72         | 0                   |

**Why it is the truth's own rule, one level down.** The scorer already credits a chunk's publisher when the app ships
the file directly: the shipper is judged right and the origin `origin`. In the nested case the outer shipper O and the
origin P are credited, but R, which published the very file O inlined, is not. `via` restores the missing link and
nothing else.

**Both-direction ledger** (`ledger10.py`, every claim judged with and without `via` on the scorer 10 layer; per-label
JSON in `ledger/`):

- WRONG -> GOOD only. GOOD -> WRONG is 0 on every label (by construction, and measured).
- 313 labels: dev 93, apps 53, bundlers 30, pop 28, test 65, test2 35, remin 9.
- Moved, all WRONG -> GOOD:

  | labels                                                   | moved                                                                |
  | -------------------------------------------------------- | -------------------------------------------------------------------- |
  | dl25k labels (`exp118-dl25k*`, `exp121-v2*`)             | test bun / esbuild / rollup / webpack 3,259,905 / 3,287,730 / 1,609,140 / 1,604,830 B; pop 1,637,099 / 1,650,726 B |
  | `*exp124-orig*`                                          | test about the same; pop 3,303,105 / 3,331,289 B                     |
  | one apps-rollup unit (3,610 B)                           | see below                                                            |
  | some k2-all / k2-all-r5 test labels (k2-v2, exp124 base/drop/r5, exp127) | 8 B per test-bun / test-esbuild set                     |
  | everything else (all of dev, bundlers, test2, remin)     | 0 B                                                                  |

- The apps-rollup unit: a file of http-proxy-3 (`.../passes/ws-incoming.js`). The chunk rule misread it as a chunk
  of `ws`, because `ws` is the stem's `-`-prefix and a devDependency. The unit is http-proxy-3's own code, so the
  verdict was wrong under scorer 9 and is right now.

**Hand check (dev, pop; `hand.py` in the lab):**

- Every pop unit the rule moves lies in O's built file, in a `//#region` whose marker path is
  `node_modules/<R>/dist/<P>-<hash>.mjs`.
- k2-all-dl25k holds that exact file under R: 3,556 rows of module bytes 1,635,356, the bundle unit to within 0.1%.
- The claim names R. 4/4 distinct (bundle, unit) groups and the one apps unit are all right.

**Re-score:** every committed label was re-scored under scorer 10 with exp120's `rescore_many.py` (dev 93, apps 53,
bundlers 30, pop 28, test 65, test2 35, remin 9). Cards move only where the ledger says.

## 3. The ship bar (scorer 10; P / R / **F0.5**, dF0.5 vs k2-all-r5)

- r5 = `*exp124-r5*`: the current default index k2-all-r5 with main's defaults (copy_presence v2 on).
- k2 = `*exp121-k2-v2*`: k2-all + v2.
- dl = `*exp121-v2*`: k2-all-dl25k + v2 + via.
- dlf = `*exp125-dlf*`: dl + `max_packages_by_family`.
- orig = `*exp124-orig*`: exp124's `--sponge-regions origin` r5 build.

| set           | r5 (default)              | k2               | **dl (dl25k + v2 + via)**               | dlf              | orig             |
| ------------- | ------------------------- | ---------------- | --------------------------------------- | ---------------- | ---------------- |
| cc            | 100.00 / 81.90 / **95.77** | 95.77 (+0.00)    | 100.00 / 82.46 / **95.92 (+0.15)**      | 95.95 (+0.18)    | 95.77 (+0.00)    |
| esbuild-app   | 96.60 / 69.44 / **89.59** | 87.99 (-1.60)    | 98.53 / 66.50 / **89.87 (+0.29)**       | 89.88 (+0.29)    | 89.65 (+0.06)    |
| held-out      | 99.31 / 67.60 / **90.79** | 90.42 (-0.37)    | 99.56 / 81.23 / **95.26 (+4.47)**       | 95.26 (+4.47)    | 90.87 (+0.08)    |
| wrangler      | 95.99 / 18.53 / **52.28** | 52.28 (+0.00)    | 99.37 / 20.92 / **56.79 (+4.51)**       | 57.28 (+4.99)    | 56.12 (+3.84)    |
| apps-esbuild  | 99.29 / 92.15 / **97.78** | 97.20 (-0.58)    | 99.12 / 88.81 / **96.87 (-0.91)** LOSS  | 96.85 (-0.93)    | 98.21 (+0.44)    |
| apps-rollup   | 98.98 / 78.66 / **94.12** | 93.72 (-0.40)    | 99.00 / 80.91 / **94.76 (+0.64)**       | 94.79 (+0.67)    | 94.35 (+0.24)    |
| apps-webpack  | 99.39 / 73.30 / **92.79** | 92.79 (+0.01)    | 99.33 / 75.04 / **93.30 (+0.51)**       | 93.32 (+0.54)    | 92.80 (+0.01)    |
| dev-rollup    | 99.87 / 77.53 / **94.43** | 94.43 (+0.00)    | 99.87 / 77.35 / **94.37 (-0.06)** LOSS  | 94.38 (-0.05)    | 94.43 (+0.00)    |
| dev-webpack   | 99.64 / 62.89 / **89.22** | 89.22 (+0.00)    | 99.64 / 62.63 / **89.11 (-0.11)** LOSS  | 89.09 (-0.12)    | 89.22 (+0.00)    |
| pop-bun       | 95.51 / 79.34 / **91.77** | 91.13 (-0.64)    | 98.41 / 78.96 / **93.79 (+2.02)**       | 93.75 (+1.98)    | 95.88 (+4.11)    |
| pop-esbuild   | 94.08 / 81.42 / **91.24** | 90.74 (-0.50)    | 97.26 / 81.61 / **93.67 (+2.43)**       | 93.67 (+2.43)    | 95.50 (+4.26)    |
| TEST bun      | 97.85 / 67.56 / **89.80** | 89.39 (-0.40)    | 98.70 / 68.55 / **90.72 (+0.92)**       | 90.72 (+0.92)    | 92.89 (+3.09)    |
| TEST esbuild  | 97.00 / 76.80 / **92.16** | 91.94 (-0.21)    | 98.36 / 77.65 / **93.38 (+1.22)**       | 93.38 (+1.22)    | 95.25 (+3.10)    |
| TEST rollup   | 97.94 / 68.80 / **90.29** | 90.13 (-0.16)    | 98.43 / 72.09 / **91.72 (+1.43)**       | 91.74 (+1.45)    | 91.94 (+1.65)    |
| TEST webpack  | 99.24 / 70.62 / **91.80** | 91.78 (-0.03)    | 99.37 / 75.09 / **93.33 (+1.53)**       | 93.36 (+1.56)    | 92.49 (+0.68)    |
| TEST2 bun     | 98.85 / 79.30 / **94.21** | 93.64 (-0.56)    | 98.72 / 80.34 / **94.40 (+0.20)** P lower | 94.37 (+0.17)  | 94.28 (+0.07)    |
| TEST2 esbuild | 99.49 / 75.80 / **93.64** | 93.10 (-0.53)    | 99.52 / 75.84 / **93.67 (+0.04)**       | 93.71 (+0.07)    | 93.61 (-0.03)    |
| TEST2 rollup  | 96.33 / 75.35 / **91.25** | 91.08 (-0.16)    | 96.11 / 74.90 / **90.96 (-0.29)** LOSS, P lower | 91.05 (-0.20) | 91.35 (+0.10) |
| TEST2 webpack | 99.27 / 70.08 / **91.64** | 91.31 (-0.33)    | 99.27 / 71.32 / **92.05 (+0.41)**       | 92.12 (+0.48)    | 91.64 (+0.00)    |

App claimed is 0 on every label. test and test2 are summaries only. The full table is reproduced by
`python3 -I table.py exp124-r5 exp124-orig exp121-k2-v2 exp121-v2 exp125-dlf exp125-k2f`.

**Bar verdict:** NOT met. dl beats r5 on 15 of 19 sets. It loses:

- apps-esbuild -0.91 (-0.33 against k2-all + v2);
- dev-rollup -0.06;
- dev-webpack -0.11;
- test2-rollup -0.29 (-0.12 against k2);
- and test2 precision is lower on bun (98.72 vs 98.85) and rollup (96.11 vs 96.33).

**The three dev recall losses (lab, `lostwhy.py` / `famsim.py` / `why1.py`, dev only):**

- **apps-esbuild:** 667 KB of k2's good claims are lost because the print's owners overflow `max_packages` in the
  bigger index. 622 KB of that is one unit: a 622 KB base64 wasm payload that a scope family's three packages hold,
  and that one dl25k carrier also holds at the same module size (4 owners, 2 families).
  - With `max_packages_by_family` the seed is made, but the isolation veto refuses it: it is ambiguous between two
    families, and its only neighbours derive from it.
  - The index cannot tell the publisher from the carrier at equal size, so this is the conservative answer.
- **dev-rollup / dev-webpack (recall -0.18 / -0.26 pts):** prints that gained owners (more-owners / overflow) and lost
  propagation behind them. Each is small and spread over packages.
- Not fixed in this lane.

## 4. Neutrality and the matcher key

`max_packages_by_family` (`crates/humanify-core/src/libindex/lookup.rs`):

- `LookupRules::max_packages_by_family` and `families_exceed`. `look` / `structural_scores` / `structural_owners` /
  `structural` take the family judgement (`same`).
- Config key `[library.lookup] max_packages_by_family`, default false.
- Red-first test: `owners_counted_by_family_let_a_family_and_one_carrier_through`.
- On k2-all (`*exp125-k2f*`) it is not clean either: dev-webpack -0.04 and test-bun -0.02. It stays off.

Warm neutrality (STATUS) proves the default is unchanged.

## Reproduce

```bash
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python; E=experiments/125-nested-chunk-publisher
$PY -I $E/scorer10/origins10.py build --corpus C --jobs 4; $PY -I $E/scorer10/origins10.py verify --corpus C
$PY -I $E/ledger10.py --corpus C --json out.json [--rows out.jsonl] <labels...>   # test/test2: --json only
$PY -I experiments/120-truth-families/rescore_many.py --corpus C --cache <scratch> <labels...>
npm run eval -- library exp125-dlf --index /work/exp118-data/idx/ix/k2-all-dl25k --rule structural \
    --pipeline-set library.lookup.max_packages_by_family=true          # + --corpus apps|bundlers|pop|test|test2
python3 -I $E/table.py exp124-r5 exp124-orig exp121-k2-v2 exp121-v2 exp125-dlf exp125-k2f
```

The lab scripts (`judge.py`, the claims.py replacement that applies every scorer layer; `agg2.py`; `seedsim.py`;
`lostwhy.py`; `famsim.py`; `why1.py`; `hand.py`; the private `testlab.py` / `diag_diff.py` wrappers for test's
aggregate diagnosis) stay in `/work/exp125-data`. Their per-app rows are not committed.

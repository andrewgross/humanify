# exp149: a bundle-level vote for exact ties

## STATUS (2026-10-24)

DONE on branch `exp149-bundle-tie-vote` (off main `d84196a24`). Harness scripts and aggregate tables only. **No
crates change, no key, no default changed, nothing scored, nothing merged.** Dev corpora only; test / test2 /
remin2 were not scored, diagnosed or opened.

**REFUTED at simulation, so nothing was built.** The build condition was "net-positive precision on every dev set".
No vote meets it, on either index.

- **Census (§2).** On the recommended index, main names wrong **989 KB of tied units where the truth's family is
  one of the tied families**. That is the 783 KB exp148 counted where the version is indexed, plus the same kind of
  tie where it is not. Main names **108–109 MB of tied units right**.
  - In **61%** of the lost ties (604 KB), both the truth's family and the family main picked show exclusive code
    elsewhere in the bundle.
  - Only the truth's family shows any in 18% (183 KB), only the winner in 14% (138 KB), and neither in 7% (66 KB).
  - The won ties look much the same: 34.3 MB have both families shown, and in 14.0 MB of those the RIVAL shows
    more. So a vote by amount of evidence breaks about 30 times what it fixes. A vote that only decides when it
    is clear-cut leaves about 70% of the lost ties exactly where they are.
- **Per-unit votes (§3), recommended index, scope "the run claimed it":**
  - The **plurality vote** is exp134's rule. Measured again here, it fixes 468 KB and breaks 14.0 MB. Precision
    falls on all 16 sets.
  - **Decisive votes** are the new part. The best is "exactly one tied family shows exclusive code in the bundle,
    and it holds the unit in an index module no bigger than the claimed family's". It fixes 134 KB and breaks
    5 KB. Precision falls on wrangler (-0.018) and rises on 5 sets.
    - 111 KB of the fix is ONE apps-rollup bundle: a fork of react-dom tied with react-dom.
  - The **safest vote** decides only when exactly one tied family has OTHER claims in the bundle, with the same
    context gate. It fixes 22 KB and breaks 0. It is up on 4 sets and exactly unchanged on 12.
    - 19 KB of the fix is one pop-bun bundle.
  - Neither of the last two is net-positive on every set, and together they reach 2–14% of the lost-tie bytes.
- **Carrier index (§4).** With exp148's carriers, **7.08 MB of tied units go unclaimed that main names right**:
  these are the refused originals.
  - A vote names about half of them right (3.1–3.5 MB). It also names 157–161 KB of them wrong, a 4.5% error
    rate against the base's ~0.5%.
  - **Even in the optimistic bound, carrier index + vote stays below main's precision on 11 to 12 of 17 sets.**
    The optimistic bound assumes the vote reaches exactly the refused originals.
  - F0.5 also stays below main on most sets (wrangler 65.4 vs 70.9, remin-terser 53.4 vs 56.2).
  - The pessimistic bound lets the vote also name what other guards refused (23.5 MB of carrier-involved
    unclaimed ties). It collapses precision: remin-terser 98.7, remin4-terser 98.9.
- **Why the evidence is not there.** Main's existing tie rules already use the bundle:

  - the previous-claim tie break (a run of claims);
  - the support window (a weak seed needs a same-package neighbour);
  - `shadow_ties` (a package whose every seed is tied yields to a rival with its own seeds).

  These already guarantee that the family main picks shows itself in the bundle. So the lost ties are mostly
  "both present" cases, and the bundle cannot tell them apart. The truth credits the shipper of an unmarked copy
  (exp110), and the shipper and the origin are both really in the bundle.

- **Gate:** `npm run check` bare, 13/13 PASS. **Neutrality:** not run, because no crates changed. **Judge commands:** none to report;
  nothing was built.

## 1. How this differs from exp133 / exp134 / exp139

| lane                                       | rule                                                                                                                                                                                                                                                                                                                                                                                | verdict                                                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| exp133 `tie_by_context` (lab patch)        | per unit: the tie goes to the tied package holding the prints in the smallest index module (bundle evidence not read)                                                                                                                                                                                                                                                               | cc recall -4.4, dev-webpack P -0.45; with a "decisive" bound it does nothing                   |
| exp134 sim "ties"                          | per unit: the tie goes to the tied family with the most untied SEED bytes in the bundle (plurality)                                                                                                                                                                                                                                                                                 | breaks 1.7–50x what it fixes                                                                   |
| exp139 sim (b) presence / (b') carrier-pkg | a contested seed of a family NOT shown (no declared seed, exclusive seeds < 10%) goes to the shown co-holder with the most exclusive bytes, else REFUSED                                                                                                                                                                                                                            | breaks more than it fixes on every k2 dev set; refusals cost 175–430 KB of right bytes per set |
| main `shadow_ties` (exp114, on)            | package level: a package ALL of whose index seeds are tied yields to a rival with untied seeds and no bigger context                                                                                                                                                                                                                                                                | shipped                                                                                        |
| **exp149** (this lane)                     | per unit, every tied unit the run claimed (seed or propagated): a vote over the bundle's OTHER units, with four kinds of evidence (index-held units the other tied families do not hold; the run's claims; units held by that family alone; claims held by no other family), five decision rules (unique, 2x / 4x / 10x dominance, plurality), each with and without a context gate | no rule raises precision on every set; the safe ones reach 2–14% of the lost ties              |
| **exp149**, carrier index                  | the same votes, plus the votes deciding units the carrier run leaves unclaimed (the refused originals), projected against main per set                                                                                                                                                                                                                                              | below main's precision on 11–12 of 17 sets even in the optimistic bound                        |

What is genuinely new: the decision is DECISIVE rather than by amount ("exactly one tied family shows itself"),
the evidence is read from the index (not only from seeds), the claimed family may have other evidence (so it reaches
past `shadow_ties`), nothing is refused (an undecided vote leaves today's rule), and the carrier index's refused
originals are measured separately. The plurality rule is included only to reproduce exp134.

## 2. Census: main's ties and what a vote does to them

`scripts/ties.py` (per label): for every scored library unit whose WHOLE print the index holds under two or more
families, it records:

- the label's claim and its scorer-14 verdict;
- the truth's family;
- for each tied family F, the bundle's evidence for F (§3). These are bytes of the bundle's OTHER units:
  - `held`: units F's index holds and no other tied family holds;
  - `claim`: units the run claims as F that no other tied family holds;
  - `excl`: units only F holds;
  - `exclClaim`: units claimed as F that no other family holds;
- F's tightest index module for the unit;
- the verdict the unit would get named after F.

Holders are read as the lookup reads them (specificity 4x + 4096) and unfiltered, as exp148 read them. Rows name
packages and stay in `/work/exp149-data`. `scripts/census.py` aggregates them.

**Recommended index** (`*exp142-asof*`). KB right / wrong / unresolved under each vote. "Unresolved" means the
vote does not decide, so today's rule stands:

| ties              |     total |               claim:unique |                 excl:unique |           excl:unique+ctx |              claim:plural |                 excl:plural |
| ----------------- | --------: | -------------------------: | --------------------------: | ------------------------: | ------------------------: | --------------------------: |
| lost (main wrong) |     989.2 |       35.5 / 261.3 / 692.3 |       165.8 / 129.2 / 694.1 |     133.9 / 128.5 / 726.8 |       446.3 / 542.9 / 0.0 |        490.5 / 433.0 / 65.7 |
| won (main right)  | 109,467.1 | 76,789.7 / 10.3 / 32,667.1 | 73,656.8 / 124.4 / 35,685.9 | 73,618.9 / 5.1 / 35,843.1 | 95,528.0 / 13,936.9 / 2.1 | 95,369.6 / 13,693.8 / 403.7 |

(For a won tie, "wrong" is right bytes broken; "right" includes the vote agreeing with main.)

**What each side shows** (`scripts/patterns.py`, all 16 sets, KB). T = the truth's family; W = main's pick for a
lost tie, or the best-shown rival for a won tie. `TW>` means both show exclusive code and T shows more:

| ties | both, T more | both, W more | both, equal |   only T | only W | neither |     total |
| ---- | -----------: | -----------: | ----------: | -------: | -----: | ------: | --------: |
| lost |        322.3 |        278.2 |         3.0 |    182.7 |  137.6 |    65.6 |     989.3 |
| won  |     20,320.3 |     14,016.5 |         0.1 | 73,401.3 |  250.8 |   403.7 | 108,392.8 |

- The "only T" lost ties (183 KB) are what a decisive vote can fix. The "only W" won ties (251 KB) are what it
  breaks. Without a context gate the two are about even, set by set (§3).
- Main's own run already makes W show itself: W has other claims in the bundle in all but 36 KB of the 989 KB
  of lost ties (the support window and the previous-claim tie break see to that).

**Carrier index** (`*exp148-car*`): lost 1,063.8 KB (claim:unique 37.5 / 309.6 / 716.7; excl:unique+ctx
135.2 / 130.1 / 798.4); won 122.7 MB (excl:unique+ctx breaks 5.1 KB); tied units the carrier run leaves unclaimed
that main names right: **7,077.6 KB** (§4).

Per set: `data/census-asof.md`, `data/census-car.md`.

## 3. Simulation: per-unit votes on the recommended index

`scripts/sim.py`: each rule re-decides every tied unit the run claimed. An undecided vote changes nothing.
Projected precision comes from the label's summary card. Propagation off a moved unit is not re-run, so the
numbers give the direction only.

Rules:

- `unique`: exactly one tied family has evidence > 0.
- `domK`: the top family's evidence is at least K times the next one's.
- `plural`: the most evidence wins (exp134's rule).
- `+ctx`: in addition, the voted family must hold the unit in an index module no bigger than the claimed family's.

KB fixed / broken, d P (points):

| set            |                claim:plural (exp134) |                          excl:plural |                      excl:unique |                excl:unique+ctx |              claim:unique+ctx |
| -------------- | -----------------------------------: | -----------------------------------: | -------------------------------: | -----------------------------: | ----------------------------: |
| apps-esbuild   |                5.5 / 1344.4, -1.1528 |                5.1 / 1385.6, -1.1887 |               1.4 / 2.4, -0.0009 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| apps-rollup    |               116.6 / 705.9, -0.7708 |              116.6 / 1029.0, -1.1934 |            111.7 / 52.2, +0.0778 |           111.7 / 5.0, +0.1396 |            0.9 / 0.0, +0.0011 |
| apps-webpack   |                11.9 / 791.0, -1.4514 |                13.8 / 770.6, -1.4099 |              1.9 / 24.4, -0.0421 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| dev-rollup     |                   0.0 / 1.1, -0.0700 |                   0.0 / 1.1, -0.0700 |               0.0 / 1.0, -0.0611 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| dev-webpack    |                   0.4 / 1.4, -0.0892 |                   0.4 / 2.1, -0.1495 |                     0.0 / 0.0, 0 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| esbuild-app    |                 17.7 / 80.4, -0.4527 |                 17.7 / 80.8, -0.4561 |                     0.0 / 0.0, 0 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| heldout        |               35.2 / 2408.0, -1.5002 |               35.6 / 2304.3, -1.4344 |              10.9 / 2.3, +0.0055 |             0.3 / 0.0, +0.0002 |            0.3 / 0.0, +0.0002 |
| pop-bun        |               71.0 / 3188.5, -1.2527 |               71.4 / 3040.8, -1.1932 |             26.1 / 32.1, -0.0024 |            20.5 / 0.0, +0.0082 |           20.8 / 0.0, +0.0083 |
| pop-esbuild    |              143.9 / 2705.9, -1.0067 |              143.3 / 2630.3, -0.9772 |              7.7 / 13.1, -0.0022 |                   0.0 / 0.0, 0 |            0.4 / 0.0, +0.0001 |
| remin-esbuild  |                 9.0 / 394.9, -0.7626 |                 8.8 / 352.2, -0.6785 |               0.0 / 1.0, -0.0019 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| remin-terser   |                  0.6 / 24.6, -0.1285 |                  0.1 / 24.1, -0.1287 |                     0.0 / 0.0, 0 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| remin3-esbuild |                31.4 / 633.6, -0.5975 |                 2.9 / 584.0, -0.5766 |               2.3 / 3.3, -0.0010 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| remin3-terser  |                  0.8 / 48.4, -0.1408 |                  2.9 / 31.5, -0.0850 |               1.3 / 0.1, +0.0034 |             0.6 / 0.0, +0.0018 |                  0.0 / 0.0, 0 |
| remin4-esbuild |               19.9 / 1565.6, -0.6430 |               85.6 / 1535.7, -0.6032 |              1.8 / 13.1, -0.0047 |                   0.0 / 0.0, 0 |                  0.0 / 0.0, 0 |
| remin4-terser  |                 4.4 / 124.1, -0.1385 |                  8.4 / 92.6, -0.0975 |               1.6 / 0.4, +0.0013 |             0.8 / 0.0, +0.0009 |                  0.0 / 0.0, 0 |
| wrangler       |                   0.0 / 0.1, -0.0115 |                   0.0 / 0.2, -0.0291 |               0.0 / 0.1, -0.0176 |             0.0 / 0.1, -0.0176 |                  0.0 / 0.0, 0 |
| **all**        | **468.3 / 14,017.9**, down 16 / up 0 | **512.6 / 13,864.9**, down 16 / up 0 | **166.7 / 145.5**, down 9 / up 4 | **133.9 / 5.1**, down 1 / up 5 | **22.4 / 0.0**, down 0 / up 4 |

(cc has no tied library unit. Every dominance rule (2x / 4x / 10x, any evidence) breaks 5.4–17.0 MB. The
dominance rules with `+ctx` break 0.58–0.84 MB. Full grid: `data/sim-asof.json`.)

- **No rule raises precision on every set.** The two that never lose much are concentrated:
  - `excl:unique+ctx`: 111 KB of its 134 KB is one apps-rollup app. Its truth splits the code between react-dom
    and a fork of it, and the vote also breaks 5 KB of that same bundle. It loses on wrangler.
  - `claim:unique+ctx`: 19 KB of 22 KB is one pop-bun CLI that ships copies of its dependencies. It is a
    `shadow_ties` near-miss.
- **The context gate is what makes the decisive votes safe.** Without it `excl:unique` breaks 145 KB.
- On the carrier index the same table (`data/table-sim-car.md`) reads the same:
  - `excl:unique+ctx` 139.1 / 5.2 (down 1, wrangler);
  - `claim:unique+ctx` 22.3 / 0.0 (up 4, unchanged 12);
  - plurality 471 / 13,594.

## 4. The carrier index: do the refused originals come back?

`scripts/refused.py` joins the carrier label's tied units to main's claims by unit span. It finds the tied units
the carrier run leaves unclaimed that main names right (KB right / wrong / unresolved under each vote,
`data/refused.md`):

| set            |       total |                  claim:unique |               excl:unique+ctx |                  claim:plural |
| -------------- | ----------: | ----------------------------: | ----------------------------: | ----------------------------: |
| apps-esbuild   |       832.6 |           546.6 / 8.6 / 277.4 |           428.3 / 0.8 / 403.5 |          556.4 / 83.2 / 193.0 |
| heldout        |       597.2 |          280.8 / 42.6 / 273.8 |          245.8 / 52.1 / 299.3 |          308.2 / 91.5 / 197.6 |
| pop-bun        |     1,470.2 |          662.6 / 38.5 / 769.1 |          537.7 / 30.8 / 901.7 |          695.0 / 81.0 / 694.2 |
| pop-esbuild    |     1,733.6 |          824.4 / 50.1 / 859.0 |          707.8 / 54.3 / 971.5 |         857.7 / 183.8 / 692.1 |
| remin4-esbuild |     1,334.3 |           467.2 / 2.9 / 864.2 |           468.5 / 2.4 / 863.4 |          519.3 / 25.2 / 789.8 |
| other 11 sets  |     1,109.7 |          757.6 / 18.2 / 334.0 |          703.7 / 16.1 / 389.9 |          797.5 / 66.0 / 246.0 |
| **all**        | **7,077.6** | **3,539.2 / 160.9 / 3,377.5** | **3,091.8 / 156.5 / 3,829.3** | **3,734.1 / 530.7 / 2,812.7** |

`scripts/carsim.py` projects carrier index + vote against main on the recommended index, per set
(`data/carsim.md`). It starts from the carrier label's card and moves the claimed ties under the vote. Then it
lets the vote name the unclaimed tied units, in two bounds:

- **refused** (optimistic): only the originals above;
- **all** (pessimistic): every unclaimed tied unit the vote decides.

Precision at or above main's:

| vote             |                                                   refused bound: P >= main | all bound: P >= main | F0.5 vs main, refused bound                                                   |
| ---------------- | -------------------------------------------------------------------------: | -------------------: | ----------------------------------------------------------------------------- |
| excl:unique+ctx  | 6 of 17 (cc, apps-rollup, pop-bun, remin3-terser, remin4-terser, wrangler) |              4 of 17 | below main on 14 of 17 (wrangler 65.43 vs 70.87, remin-terser 53.43 vs 56.19) |
| claim:unique+ctx |                                                                    5 of 17 |              3 of 17 | below main on 14 of 17                                                        |
| claim:unique     |                                                                    6 of 17 |              3 of 17 | below main on 14 of 17                                                        |

- The vote gets back about half of what the carriers made ambiguous. It names the other half's competitors
  about 5% wrong, and it never reaches the propagated (glue) bytes that went with them.
  - apps-esbuild lost 1.24 MB of seeds and 1.19 MB of glue between the two labels. 16 KB of that was vetoed;
    the rest disappeared from the claims. The likely cause is the presence rules that fence or drop an all-tied
    factory or package (factory tie fence, `shadow_ties`); this was not traced unit by unit.
- Even assuming the vote reached exactly the refused originals, the carrier index stays below main on most sets.
  exp148's verdict stands: the carriers are not worth indexing with this lever.

## 5. What would have to be true for a tie lever to work

The lost ties are mostly cases where the bundle really holds both families' other code. A plausible reading (not
traced case by case): the app bundles a package AND a carrier that vendors it, and the truth credits whichever copy
the bundler took. Nothing
in the bundle-level evidence separates the copy from the original; the only discriminator measured here is index
module context, which is exactly what exp133 found too weak on its own. A future lever would need evidence the
bundle's code does not carry, e.g. the module graph's import edges into the tied unit (which file the bundler took
it from) or a version-specific print difference between the copy and the original.

## 6. Gate

| check                                                | result                                                                |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| `npm run check` (bare, `/work/exp149-data/gate.log`) | exit 0, ALL 13 STAGES PASSED (on the tree committed with this README) |
| neutrality                                           | not run: no crates change, no key, no default                         |

## Files

- `scripts/ties.py`: per label, every tied unit with the bundle's evidence per tied family (rows stay in
  `/work/exp149-data`).
- `scripts/census.py`: lost / won ties and what each vote does to them. With `--carriers`, the unclaimed
  carrier-involved ties.
- `scripts/patterns.py`: which side of each tie the bundle shows.
- `scripts/sim.py`: the vote grid (4 evidence kinds x 7 rules x 2 scopes), projected per set from the cards.
- `scripts/table.py`: §3's table.
- `scripts/refused.py`: the carrier index's refused originals and what the votes do to them.
- `scripts/carsim.py`: carrier index + vote, projected per set against main.
- `data/`: the aggregate tables above (set-level only: no package or bundle names).

## Reproduce

```bash
S=experiments/149-bundle-tie-vote/scripts; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# rows (one label per corpus and index; ~1 h for the dev corpus, which holds cc)
$PY -I $S/ties.py <corpus> <label> /work/exp149-data/ties2/<corpus>-<tag>.jsonl
python3 -I $S/census.py exp142-asof --carriers experiments/148-carrier-coverage/data/carriers-C1d.txt /work/exp149-data/ties/*-exp142-asof.jsonl
python3 -I $S/sim.py /work/exp149-data/ties exp142-asof experiments/096-library-matching-score/results data/sim-asof.json
$PY -I $S/refused.py exp142-asof /work/exp149-data/ties2 exp148-car
$PY -I $S/carsim.py exp142-asof <main results> /work/exp149-data/ties2 exp148-car <exp148 results> excl unique+ctx
```

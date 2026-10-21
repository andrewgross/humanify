# exp134: carrier ownership — where library-matching precision goes, and one per-unit copy rule

## STATUS (2026-10-21)

DONE on branch `exp134-carrier-ownership` (off main `4b1d1a49`, exp133's branch merged in for its scripts and
labels). One default-off key, `[library.discovery] copy_presence_context`, plus census scripts. **Not merged.**

- **Ship bar NOT met, so: do NOT flip the key on by default, and do NOT ship dl25k.**
  - **On k2-all-r5-origin the key changes nothing on 20 of 21 sets.** The exception is test-webpack, where
    precision falls by 0.01 (568.4 -> 577.2 KB wrong, F0.5 -0.01).
  - Test is a frozen judge (aggregates only), so that loss is not diagnosed. Strictly, it fails the bar
    "precision at or above base on every set".
- **On dl25k the key does what it was built for:**
  - esbuild-app P +2.07 / F0.5 +1.95, held-out +0.18 / +0.16, pop-bun +0.12 / +0.11 and pop-esbuild
    +0.11 / +0.11;
  - **test2-bun +0.50 / +0.46 and test2-esbuild +0.44 / +0.41** (the frozen judge, summary only);
  - every other set unchanged.
- **dl25k with the key against k2-all-r5-origin without it:**
  - F0.5 is now up on held-out (+0.58), both pop sets (+0.13 / +0.34), all four test sets
    (+0.30..+1.00) and all four test2 sets (+0.11..+0.88). Before this key, test2-bun and test2-esbuild were down
    (-0.10 / -0.21).
  - But precision is still below k2-all-r5-origin's on held-out (-0.24), pop-esbuild (-0.20), test2-bun (-0.11),
    test2-esbuild (-0.15), the apps and remin-terser. remin and dev-rollup / dev-webpack F0.5 are still down.
  - So the "ideal" bar (F0.5 AND precision at or above k2-all-r5-origin on held-out, pop, test and test2) is not
    met either.
  - If dl25k is ever adopted, this key is the fix for its carrier class. The remaining precision gap is the
    "truth family not read" class (section 1), which no ownership rule reaches.
- **The census** (section 1) ranks where the wrong bytes are, under main's current defaults on both indexes.
  - The biggest class on every set, on both indexes, is a seed whose truth family is NOT READ for the unit at all:
    the index has no print of it for the truth's package. This is coverage, not a carrier rule.
  - Next come propagation off those seeds, then ties.
  - exp133's "outscored by a carrier" class is real, but it is about 11 units. They are one carrier's two
    146 KB lazy bodies, seen in four dev sets. It is 0 to 2 KB on k2-all-r5-origin.
- **Per-unit carrier -> origin ownership, as briefed, is refuted at simulation** (section 2).
  - Giving a seed to the smaller-context co-holder breaks 1.7 to 50+ times more right bytes than it fixes, on every
    set but esbuild-app, on both indexes.
  - Breaking ties by the bundle's other evidence is just as bad.
  - The reason is exp110's finding: the truth credits the shipper of an unmarked copy. Per unit, index module
    sizes alone cannot tell the two cases apart.
- **The key** (section 3): a copy share below `copy_presence` can still make a copy seed, but only when the
  package holds the unit's prints in index modules at least as big as the bundle's module (the bundle did not take
  the code from that package's file). It is judged inside exp121's per-package copy presence.
  - The floor is not a knife edge: 0.005 and 0.02 give identical results on every dev set (lab).
- **Neutrality was not run.** It is conditional on meeting the bar. The key is default-off and the default path
  goes through the same branch as before (`copy_share` gained a returned field, which only the new arm reads).
  Before any merge of the key, run the warm neutrality in default and report mode.
- **Gate:** 13/13 bare at the key's commit `66f13e14`.

## 1. Census: every wrong byte by mechanism (dev material)

`scripts/census.py` reads exp133's `judge13.py` rows. Those are per-claim verdicts exactly as scorer 13 gives
them, both scopes. It re-scores each wrong index seed with mmlib's emulation of the structural rule on the label's
own index. Base = `*exp133-base*` (k2-all-r5-origin), dl = `*exp133-dl*` (k2-all-r5-origin-dl25k). Both use main
`4b1d1a49`'s plain defaults. Held-out appears only as an aggregate; no script prints a held-out package name.

Seed classes:

- **outscored, truth smaller context**: the truth's family scores, the winner scores more, and the winner holds
  the code in a module at least 4x bigger (a carrier).
- **tie**: the truth's family scores the winner's nodes (name order or the previous claim decided).
- **truth family not read**: the truth's family holds no print of the unit, or none the lookup reads.

Propagation is split by whether the claiming package has a wrong seed in the same bundle.

KB wrong, base / dl (totals reproduce every card's `wrongFamilyBytes`):

| mechanism                          | apps-esbuild | apps-rollup | apps-webpack | dev-rollup | dev-webpack | esbuild-app |    heldout |     pop-bun | pop-esbuild | remin-esbuild | remin-terser | wrangler |
| ---------------------------------- | -----------: | ----------: | -----------: | ---------: | ----------: | ----------: | ---------: | ----------: | ----------: | ------------: | -----------: | -------: |
| outscored, truth smaller context   |        0 / 0 |       2 / 0 |        0 / 0 |      0 / 0 |       0 / 0 |     0 / 293 |    0 / 313 |     0 / 298 |     2 / 338 |         0 / 0 |        0 / 0 |    0 / 0 |
| outscored, other                   |        0 / 0 |       8 / 8 |        0 / 0 |      0 / 0 |       0 / 0 |       0 / 0 |      0 / 7 |   206 / 294 |    12 / 122 |         1 / 1 |        0 / 0 |    0 / 0 |
| tie                                |      8 / 172 |   203 / 174 |     70 / 122 |      0 / 0 |       0 / 0 |      13 / 9 |  107 / 149 |   165 / 170 |   151 / 295 |         7 / 6 |        2 / 3 |    0 / 0 |
| truth family not read for the unit |      54 / 36 |    123 / 75 |      89 / 37 |      0 / 0 |       0 / 0 |     31 / 17 |  572 / 965 |   610 / 528 |   946 / 929 |       70 / 64 |        4 / 5 |    6 / 0 |
| other seed                         |        1 / 1 |     13 / 13 |        0 / 0 |      0 / 0 |       0 / 0 |       0 / 0 |    42 / 40 |     36 / 32 |       5 / 4 |         0 / 0 |        1 / 1 |    0 / 0 |
| propagation off a wrong seed       |      45 / 49 |   237 / 382 |      56 / 48 |      0 / 0 |       0 / 0 |     76 / 33 |  190 / 154 |   331 / 382 |   553 / 854 |       31 / 28 |       8 / 46 |    0 / 0 |
| propagation, other                 |      27 / 28 |   125 / 186 |      16 / 15 |      2 / 2 |       4 / 4 |      11 / 4 |    28 / 36 |     52 / 46 |   130 / 120 |       50 / 49 |        4 / 4 |    0 / 0 |
| **total**                          |    135 / 286 |   711 / 838 |    231 / 222 |      2 / 2 |       4 / 4 |   131 / 355 | 938 / 1665 | 1401 / 1750 | 1799 / 2662 |     160 / 148 |      18 / 59 |    6 / 1 |

Concentration (`scripts/concentration.py`):

- The "outscored, truth smaller context" class on dl is 2 units on held-out, 2 on esbuild-app, 3 on pop-bun and 4
  on pop-esbuild. They are the same two lazy bodies, about 146 KB each (copy share 0.06, the carrier's index module
  1.11x the bundle's).
- The pop-bun "outscored, other" 206 KB on both indexes is ONE unit.
- The "truth family not read" class is broad: held-out factory 536 / 940 KB over 21 to 22 bundles and 67 to 74
  (bundle, claimer) pairs, top pair 11% / 28%.
- `scripts/featdist.py` finds no bundle-side feature that separates its wrong seeds from right ones. The wrong
  share is 0.5% and flat across cover, context, ties, module share and scope. The claimer holds the same file the
  truth's package ships: an index coverage question, not an ownership one.

## 2. Simulation: per-unit ownership rules, before any crates change

`scripts/sim.py` moves index seeds under two candidate rules and re-judges them seed by seed. Propagation is not
counted, so these numbers give the direction only.

- **carrier**: the seed goes to the family co-holding at least SHARE of the winner's covered nodes in a module at
  most 1/4 of the winner's. This is exp121's `copy_share`, per unit, with no package-level gate.
- **ties**: a tie goes to the tied family with the most untied seed bytes in the bundle.

KB fixed / broken (wrong -> right / right -> wrong), share 0.5:

| set          | carrier, base | carrier, dl | ties, base |  ties, dl |
| ------------ | ------------: | ----------: | ---------: | --------: |
| esbuild-app  |        12 / 1 |       4 / 4 |     12 / 7 |     6 / 5 |
| held-out     |       9 / 254 |    18 / 305 |   30 / 648 |  34 / 363 |
| apps-esbuild |       0 / 171 |     3 / 158 |    5 / 326 | 164 / 264 |
| apps-rollup  |       35 / 87 |     27 / 45 |   93 / 209 | 150 / 226 |
| apps-webpack |      32 / 169 |    31 / 153 |   13 / 256 |  12 / 232 |
| pop-bun      |      42 / 527 |    37 / 466 |   31 / 572 |  38 / 519 |
| pop-esbuild  |      70 / 614 |    74 / 368 |   82 / 448 | 232 / 333 |

At share 0.1 the carrier rule breaks more (pop-esbuild dl 3,750 KB). Both rules lose on every set but
esbuild-app.

The features of the moves (`scripts/featsplit.py` on `sim.py --features`, share > 0) show why the copy share alone
cannot separate them. The fixes (the 146 KB bodies) have share 0.06. So do the breaks: rollup's own 410 KB chunk
co-held 3% by magic-string, and @fortawesome/fontawesome-free's 37 KB statement co-held 6% by pinkie. Both have
truth = the shipper.

What separates them is the CONTEXT. In the fixes the carrier's tightest index module is 1.11x the bundle's module.
In the breaks the carrier holds the code in a module smaller than the bundle's (its own file, perhaps tree-shaken).

Two lab variants failed on dev first:

- `copy_presence` = 0.05 (the existing key, lowered) fixes the dl class but costs apps-rollup P -0.05 on both
  indexes (pinkie).
- An "origin present in the bundle" condition (`copy_presence_present`, rewritten before commit) costs
  apps-rollup P -0.57 on k2-all-r5-origin (magic-string is present in that app on its own).

## 3. The key: `[library.discovery] copy_presence_context`

Default 0 (off), read with `copy_presence` (on by default since 2026-10-16 at 0.1 / factor 4 / origin / own 0.1).

An index seed of P whose copy share is under `copy_presence` but at or over `copy_presence_context` is a copy seed
when P's index holds the unit's prints only in modules at least as big as the bundle module the unit sits in.
`Containment::copy_share` now also returns P's tightest module over its covering prints.

Everything else is exp121's rule unchanged:

- it is judged per package;
- a package whose non-copy seeds hold under `copy_presence_own` of its seed bytes loses its copy seeds;
- with `copy_presence_origin`, those seeds go to the origin.

Tests (red first):

- `copy_presence_context_counts_a_thin_copy_the_carrier_holds_only_in_bigger_modules` (discovery);
- `the_exp134_copy_presence_context_key_defaults_off_and_sets_from_the_file` (config).

## 4. Results (scorer 13, structural, both scopes)

Labels:

- `*exp134-cc*`: k2-all-r5-origin plus `copy_presence_context=0.02`.
- `*exp134-ccd*`: dl25k plus the same key.

Bases are exp133's labels at main `4b1d1a49`: `*exp133-base*` and `*exp133-dl*`. test / test2 are summaries only.
test2 was never diagnosed.

| set           | k2-all-r5-origin (base) |               k2 + key |           dl25k (base) |            dl25k + key | dP / dF0.5 on k2 | dP / dF0.5 on dl25k |
| ------------- | ----------------------: | ---------------------: | ---------------------: | ---------------------: | ---------------: | ------------------: |
| cc            |  100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | 100.00 / 82.46 / 95.92 | 100.00 / 82.46 / 95.92 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| esbuild-app   |   99.07 / 74.74 / 93.01 |  99.07 / 74.74 / 93.01 |  97.49 / 74.24 / 91.75 |  99.56 / 75.81 / 93.69 |    +0.00 / +0.00 |       +2.07 / +1.95 |
| heldout       |   99.41 / 67.71 / 90.90 |  99.41 / 67.71 / 90.90 |  98.99 / 69.70 / 91.32 |  99.17 / 69.82 / 91.48 |    +0.00 / +0.00 |       +0.18 / +0.16 |
| wrangler      |   99.07 / 33.14 / 70.87 |  99.07 / 33.14 / 70.87 |  99.88 / 33.26 / 71.31 |  99.88 / 33.26 / 71.31 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| apps-esbuild  |   99.89 / 77.65 / 94.48 |  99.89 / 77.65 / 94.48 |  99.76 / 79.06 / 94.80 |  99.76 / 79.06 / 94.80 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| apps-rollup   |   99.09 / 79.31 / 94.38 |  99.09 / 79.31 / 94.38 |  98.97 / 82.51 / 95.17 |  98.97 / 82.51 / 95.17 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| apps-webpack  |   99.58 / 73.50 / 92.98 |  99.58 / 73.50 / 92.98 |  99.60 / 74.91 / 93.44 |  99.60 / 74.91 / 93.44 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| dev-rollup    |   99.87 / 77.53 / 94.43 |  99.87 / 77.53 / 94.43 |  99.87 / 77.29 / 94.36 |  99.87 / 77.29 / 94.36 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| dev-webpack   |   99.64 / 62.89 / 89.22 |  99.64 / 62.89 / 89.22 |  99.64 / 62.63 / 89.11 |  99.64 / 62.63 / 89.11 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| pop-bun       |   99.44 / 76.61 / 93.85 |  99.44 / 76.61 / 93.85 |  99.31 / 76.99 / 93.87 |  99.42 / 77.08 / 93.97 |    +0.00 / +0.00 |       +0.12 / +0.11 |
| pop-esbuild   |   99.30 / 78.71 / 94.37 |  99.30 / 78.71 / 94.37 |  98.99 / 80.33 / 94.60 |  99.10 / 80.42 / 94.70 |    +0.00 / +0.00 |       +0.11 / +0.11 |
| remin-esbuild |   99.69 / 51.76 / 84.11 |  99.69 / 51.76 / 84.11 |  99.70 / 50.36 / 83.37 |  99.70 / 50.36 / 83.37 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| remin-terser  |   99.90 / 20.33 / 56.04 |  99.90 / 20.33 / 56.04 |  99.66 / 18.28 / 52.72 |  99.66 / 18.28 / 52.72 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| test-bun      |   99.51 / 68.99 / 91.42 |  99.51 / 68.99 / 91.42 |  99.58 / 70.11 / 91.86 |  99.58 / 70.11 / 91.86 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| test-esbuild  |   99.26 / 73.37 / 92.72 |  99.26 / 73.37 / 92.72 |  99.24 / 74.38 / 93.02 |  99.24 / 74.38 / 93.02 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| test-rollup   |   98.96 / 72.43 / 92.21 |  98.96 / 72.43 / 92.21 |  98.89 / 74.89 / 92.93 |  98.89 / 74.89 / 92.93 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| test-webpack  |   99.48 / 72.19 / 92.49 |  99.47 / 72.19 / 92.48 |  99.40 / 75.50 / 93.48 |  99.40 / 75.50 / 93.48 |    -0.01 / -0.01 |       +0.00 / +0.00 |
| test2-bun     |   99.15 / 68.12 / 90.87 |  99.15 / 68.12 / 90.87 |  98.53 / 69.02 / 90.77 |  99.04 / 69.37 / 91.23 |    +0.00 / +0.00 |       +0.50 / +0.46 |
| test2-esbuild |   99.63 / 71.04 / 92.21 |  99.63 / 71.04 / 92.21 |  99.03 / 71.63 / 92.00 |  99.48 / 71.95 / 92.41 |    +0.00 / +0.00 |       +0.44 / +0.41 |
| test2-rollup  |   96.35 / 75.65 / 91.35 |  96.35 / 75.65 / 91.35 |  96.29 / 76.18 / 91.46 |  96.29 / 76.18 / 91.46 |    +0.00 / +0.00 |       +0.00 / +0.00 |
| test2-webpack |   99.27 / 70.09 / 91.64 |  99.27 / 70.09 / 91.64 |  99.59 / 72.06 / 92.52 |  99.59 / 72.06 / 92.52 |    +0.00 / +0.00 |       +0.00 / +0.00 |

Reading the table:

- **k2-all-r5-origin** (the recommended index): byte-identical on 20 of 21 sets. test-webpack has +8.8 KB wrong
  (P -0.01, F0.5 -0.01). It is a frozen judge, so the cause is unread.
- **dl25k**: the key removes 293 KB (esbuild-app, held-out, pop) and 866 to 870 KB (test2-bun, test2-esbuild) of
  wrong bytes, and recall rises with it. On the dev sets, each unit it fixes now names the truth's origin. test2 is read from its summary only. Every
  other set is unchanged.

## 5. Neutrality

Not run: the ship bar (precision at or above base on every set on k2-all-r5-origin) is not met, because of
test-webpack's 8.8 KB. The default path (key off) runs the same code as main: the new arm in `copy_presence` is
guarded by `copy_presence_context > 0.0`, and the only other change is a third returned field of `copy_share`
that the old arms ignore. A merge of the key, even default-off, should first run
`npm run eval -- neutrality 4b1d1a49e 2.1.215:2.1.216 --priors /work/exp050-cold --cache /work/neutrality-cache`
under `flock /work/neutrality.lock`, in default and report mode.

## Reproduce

```bash
S=experiments/134-carrier-ownership/scripts; J=/work/exp133-data/judge   # exp133's judge13 rows (dev, apps, pop)
PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
$PY -I experiments/133-dl25k-r5/scripts/judge13.py bundlers <out> exp133-base-bundlers exp133-dl-bundlers  # remin likewise
$PY -I $S/census.py dev $J/dev-base-dl.jsonl c0 exp133-base --by-set [--dump rows.jsonl]   # c1 exp133-dl; apps/pop/...
python3 -I $S/census_table.py <base census.txt...> -- <dl census.txt...>
python3 -I $S/concentration.py <dump.jsonl>...
$PY -I $S/sim.py dev $J/dev-base-dl.jsonl c1 exp133-dl --share 0.5 [--features feat.jsonl]
python3 -I $S/featsplit.py feat.jsonl...
$PY -I $S/seedfeat.py dev $J/dev-base-dl.jsonl c1 exp133-dl sf.jsonl; python3 -I $S/featdist.py sf.jsonl --scope factory
$S/lab.sh <label> <binary> <index> <cpu0> --pipeline-set library.discovery.copy_presence_context=0.02   # dev only
$S/score-all.sh exp134-cc /work/exp130-data/full/ix/k2-all-r5-origin 0 --pipeline-set library.discovery.copy_presence_context=0.02
$S/score-all.sh exp134-ccd /work/exp133-data/full/ix/k2-all-r5-origin-dl25k 30 --pipeline-set library.discovery.copy_presence_context=0.02
python3 -I experiments/133-dl25k-r5/scripts/table.py exp133-base exp134-cc
```

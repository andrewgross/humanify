# exp139: carriers — a package credited for a copy of another package's code

## STATUS (2026-10-22)

DONE on branch `exp139-carriers` (off main `6f604195c`). Five default-off `[library.discovery]` keys (`terser_carriers`, `terser_carriers_own`, `terser_factory_majority`, `terser_thin`, `terser_payload_literal`), census and
simulation scripts, labels. **Not merged. No default flipped** (Andrew decides).

- **Headline: `terser_slices = "auto"` + the exp139 keys MEETS the flip bar on all 21 sets** (scorer 14, both
  scopes, base = main's defaults `*exp138-helper*` re-scored under scorer 14):
  - remin-terser: precision **99.902 -> 99.917**, recall 20.33 -> 53.16, **F0.5 56.04 -> 84.97 (+28.93)**; wrong
    bytes 18.6 -> 41.3 KB on 2.6x the named bytes. terser auto alone was 99.32 (339.8 KB wrong).
  - remin-esbuild: P 99.719 -> 99.720, F0.5 +0.044 (terser's own gain, unchanged by the keys).
  - every other set (dev, held-out, apps, bundlers, pop, test, test2): **identical** to the base (the terser
    verdict is off there and every key is gated on it). App claimed 0 everywhere.
  - The keys: `terser_carriers = 0.5` (+ `terser_carriers_own = 0.1`), `terser_factory_majority = 1`,
    `terser_payload_literal = true`, all `[library.discovery]`, all default off.
- **Recommendation (Andrew decides): flip `terser_slices = "auto"` (terser_share 0.2) together with these three
  keys.** Not flipped here.
- **Census** (section 1): with terser auto, 258 of remin-terser's 340 KB wrong is the carrier class (76%), 199 KB
  of it one plugin (`@vitejs/plugin-basic-ssl`) winning name-order ties for its host's (vite's) copied chunk. On
  dl25k the carrier class is most of the NEW wrong bytes on every dev set but held-out (held-out 409 of 913 KB;
  the rest is coverage).
- **Simulations** (section 2): (a) publication-time precedence is REFUTED (breaks 4-20x what it fixes, every set,
  both indexes); (b) bundle presence applied globally is REFUTED (breaks more than it fixes on every k2 dev set);
  (c) dependency direction not re-run (exp110 refuted it; here the rivals are not dependencies); **(d) scoping
  the carrier check to the terser slices' extra seeds works**: 63.8 KB fixed, 0 right bytes touched.
- **dl25k: not unblocked.** No global rule passed simulation on dl25k either (carrier-pkg breaks 22-46 KB of right
  seeds on apps/pop for 1-60 KB fixed); its residue is coverage plus exp134's carrier fix
  (`copy_presence_context`, branch exp134). The terser-gated keys are inert there by construction. dl25k + key
  was not scored (inert by construction, 2 x 70 min).
- **A global payload variant was measured and dropped**: `payload_unique_literal` (commit `28c1de28`, labels
  `*exp139-key*`) cost apps/pop recall 0.44-0.69 points (F0.5 -0.13..-0.20); it is now `terser_payload_literal`,
  read only in bundles that read the terser slices.
- Gate: 13/13 bare at `28c1de28` and at `b7333e9a` (`npm run check`).
- **Warm neutrality** of `b7333e9a` against main `6f604195c` (pair 2.1.215 -> 2.1.216, `--priors
  /work/exp050-cold --cache /work/neutrality-cache`, under `flock /work/neutrality.lock`, from a clean detached
  worktree): default mode **NEUTRAL, 0 files / 0 lines, cache +0 / +0, exit 0 / 0**; report mode
  (`library.matching=report`, `library.index=k2-all-r5-origin-prod`) **NEUTRAL, 0 / 0, +0 / +0, exit 0 / 0**, and
  `library-report.json` identical too. This proves the keys-off code path; a flip lane would re-run it with the
  flipped defaults, as exp138's did.

## 0. The problem

A carrier is a package whose published code holds a copy of another package's code. When the bundle holds that
code, the carrier can be named instead of the package the truth credits. It blocks two known gains:

- `[library.lookup] terser_slices = "auto"`: remin-terser F0.5 +28.7, but precision 99.90 → 99.32 (exp138);
- the dl25k index (exp133/134): F0.5 up, precision down on held-out, pop and test2.

Already refuted, not repeated: index-time dedup to the origin (exp110), per-unit "smaller-module co-holder wins"
and tie-by-context (exp133/134), `copy_presence` lowered to 0.05 (exp134). They all fail for one reason: the
truth credits the SHIPPER of an unmarked copy, not its origin.

Material: dev only (held-out as aggregates, dev, apps, bundlers, pop, remin). test is read as aggregates. test2
(exp116) is frozen: summaries only, never diagnosed. Scorer 14 throughout. Index: k2-all-r5-origin-prod (the
recommended index), except the dl25k census.

## 1. Census: where the carrier-wrong bytes are

`scripts/carriers.py` builds a dossier of every index seed of a label, from a judge13.py file (exp133's per-claim
verdicts; scorer 14 lives in `score.label_units` since exp137) and the label's reports. It reads ALL the slices the
run read for the bundle, so a terser `union` verdict is reproduced. Per seed it records:

- the structural rule's score per package, over every slice read and over the chosen slice alone;
- the earliest registry publication time of an indexed version holding the unit's prints, per scoring package;
- per bundle, each family's EXCLUSIVE seed bytes (seeds no other family scores) and declared seeds.

`scripts/census139.py` classes every wrong byte (`data/census.json`, classes and sizes only):

- **seed/carrier-tie**: the truth's family scores the unit as well as the claimer (name order or the previous claim
  decided);
- **seed/carrier-outscored**: the truth's family scores it, with fewer nodes;
- **seed/truth-not-scoring**: the truth's family holds no print the lookup reads;
- **prop/off-carrier**: propagated by a package that has a carrier-class wrong seed in the same bundle;
- **prop/other**.

A seed's claimer is "shown" when its family has a declared seed, or exclusive seeds of at least 10% of its seed
bytes. "terser-made": the claimer scores more over the slices read than over the chosen slice alone.

### 1a. `terser_slices = auto` (remin-terser; base `*exp138-helper*`, variant `*exp138-both*`)

| class (KB wrong in the variant)              | remin-terser |
| -------------------------------------------- | -----------: |
| seed/carrier-tie, claimer not shown, terser-made |     63.8 |
| seed/carrier-tie, claimer shown, terser-made |         34.7 |
| seed/carrier-tie, other                      |          4.3 |
| prop/off-carrier                             |        155.4 |
| **carrier class**                            |    **258.2** |
| seed/truth-not-scoring, terser-made          |         59.6 |
| seed/truth-not-scoring, other / seed/other   |          1.1 |
| prop/other                                   |         21.0 |
| **total**                                    |    **339.8** |

The carrier class is 76% of the wrong bytes. Most of it is one bundle (an electron starter that ships vite):

- 199 KB is `@vitejs/plugin-basic-ssl`: vite's inlined certificate chunk, which the plugin later copied. Both hold
  it whole in near-identical modules (199,225 vs 199,383 bytes); the plugin wins every tie on name order (`@`
  sorts first). The plugin has 7 KB of seeds no one else scores (9% of its seeds); vite has 306 KB exclusive and a
  declared seed.
- 43 KB rollup / 19 KB `@vue/compiler-core` / 9 KB prettier: code inside vite's own 888 KB and 109 KB dist
  factories, partly ties with vite, partly not scored by vite at all.

The truth-not-scoring class (60 KB) is forks and shippers the index does not hold for that code: `jju` for
`liquid-json` (16 KB, a fork), prettier's copy of `@babel/parser` (4.7 KB x 3 bundles), `import-meta-resolve`
for `@babel/core`. No ownership rule reaches it.

### 1b. dl25k (base `*exp133-base*`, variant `*exp133-dl*`, both at main `4b1d1a49`, judged under scorer 14)

NEW wrong bytes (wrong in dl25k, not in the base), KB:

| class                                  | held-out | esbuild-app | apps-esbuild | apps-rollup | apps-webpack | pop-bun | pop-esbuild |
| -------------------------------------- | -------: | ----------: | -----------: | ----------: | -----------: | ------: | ----------: |
| seed/carrier-outscored, not shown      |    320.2 |       292.8 |            0 |           0 |            0 |   292.5 |       291.5 |
| seed/carrier-outscored, shown          |        0 |           0 |            0 |           0 |            0 |     5.8 |        79.8 |
| seed/carrier-tie, not shown            |     51.6 |         3.4 |          2.3 |        58.1 |         54.8 |    32.6 |         4.5 |
| seed/carrier-tie, shown                |     10.9 |           0 |        167.3 |         4.3 |          0.1 |     8.3 |       169.4 |
| prop/off-carrier                       |     26.7 |        23.8 |         17.7 |       316.3 |          2.0 |    57.4 |       323.1 |
| **carrier class**                      |  **409.4** |   **320.0** |    **187.3** |   **378.7** |     **56.9** | **396.6** |   **868.3** |
| seed/truth-not-scoring                 |    492.3 |         1.6 |          4.1 |         7.0 |          0.8 |   178.5 |       190.1 |
| prop/other                             |     11.2 |         2.2 |          3.1 |         7.1 |          0.2 |    48.8 |        68.8 |
| **total new wrong**                    |  **912.8** |   **323.9** |    **194.5** |   **392.7** |     **57.9** | **623.8** |  **1127.2** |

The carrier class is most of dl25k's new wrong bytes on every dev set but held-out, where the coverage class
(truth not scoring, 492 KB) is as large. The "outscored, not shown" rows are exp134's two 146 KB lazy bodies
(`copy_presence_context`, exp134, fixes them); the rest is ties and propagation.

## 2. Simulations (seed level, before any crates change)

`scripts/simcar.py` moves index seeds under each rule and re-judges them; `scripts/simfactory.py` does the factory
rule. Propagation is estimated only when a rule leaves a package with no seed in a bundle. Outputs:
`data/sim-*.txt` (set-level aggregates).

Rules, W = the claimer, "contested" = another family scores the unit at least as well (alpha 1):

- **(a) pubtime**: a contested seed goes to the family whose indexed version holding the print was published
  first.
- **(a+b) pub+abs**: pubtime only when W is not shown; the earlier publisher must be shown, else refused.
- **(b) presence**: a contested seed of a W not shown goes to the shown co-holder with the most exclusive bytes,
  else refused. **refuse**: the same, always refused.
- **(b') carrier-pkg**: a W not shown, at least half of whose seed bytes a shown family contests, is that family's
  carrier: its contested seeds go to it, its other seeds are refused.
- **(d)** any of these scoped to terser-made seeds.

KB moved, right→wrong / wrong→right (direct seed bytes):

| rule                     | remin-terser (terser-made only) | held-out (k2) | apps-esbuild (k2) | apps-webpack (k2) | pop-bun (k2) | pop-esbuild (k2) | held-out (dl25k) | pop-esbuild (dl25k) |
| ------------------------ | ------------------------------: | ------------: | ----------------: | ----------------: | -----------: | ---------------: | ---------------: | ------------------: |
| (a) pubtime              |                   145.2 / 55.7 |  912.0 / 97.9 |      710.4 / 6.6 |      254.1 / 55.2 | 1710.6 / 89.0 |   2006.1 / 120.1 |    912.6 / 143.9 |      1672.0 / 275.5 |
| (a+b) pub+abs            |                     0.2 / 55.2 |    0.3 / 12.1 |        0.2 / 3.7 |          0 / 0 |   0.1 / 6.4 |        0.1 / 4.0 |       0.3 / 28.6 |           0.1 / 0.6 |
| (b) presence             |                     0.3 / 63.8 |   24.0 / 14.1 |       35.9 / 3.7 |       36.3 / 1.9 |  53.3 / 6.4 |      49.5 / 12.4 |       0.3 / 31.1 |          46.7 / 8.5 |
| (b') carrier-pkg         |                   **0 / 63.8** |   23.8 / 14.1 |       34.9 / 3.7 |       36.3 / 1.9 |  52.1 / 6.4 |      46.7 / 12.4 | 0 (+15.5 right→other) / 17.7 |          45.7 / 8.5 |

Plus refusals (right→refused): pub+abs refuses 303 KB right on held-out (k2), 3.5 MB on apps-esbuild and 2.6 MB on pop-esbuild (dl25k); presence on dl25k refuses 113 KB (held-out) and 2.5 MB (pop-esbuild) of right seeds;
presence refuses 279 / 175 / 430 / 385 KB right on held-out / apps-esbuild / pop-bun / pop-esbuild (k2);
carrier-pkg scoped to terser-made seeds refuses 7.1 KB, all of it wrong.

- **(a) publication time is refuted.** The earlier publisher is the truth's package far less often than not:
  globally it breaks 4 to 20x what it fixes on every set and index. On remin-terser it fixes the plugin/vite case
  (vite's version holding the chunk is older) but breaks more elsewhere. The index only holds sampled versions,
  but the deeper reason is exp110's: the truth credits the shipper, and the shipper is as often the later
  publisher (a fork, a re-bundle) as the earlier.
- **(b) bundle presence, applied globally, is refuted**, as exp134's per-unit rules were: on every k2 dev set it
  breaks more right bytes than it fixes. A package that ships a copy the bundle really took from it is often "not
  shown" by code of its own.
- **(c) dependency direction** was not simulated again: exp110 built it into the index (dedup when B depends on
  A) and it was refuted for the same reason; and in the cases found here the rival is not a dependency at all
  (vite and the plugin both carry a third package's code).
- **(d) scoping to terser-made seeds works**: carrier-pkg on terser-made seeds fixes 63.8 KB and refuses 7.1 KB
  of wrong seeds, 0 right bytes touched. Every non-remin set is untouched by construction (no terser verdict
  there). A factory-majority rule on terser-made seeds (a minority family inside another package's cut factory)
  refuses 63.4 KB wrong / 44.9 KB right at K = 1..2, 23.3 / 0.8 at K = 10.

Global variants on dl25k are no better than on k2 (`data/sim-dl25k-global.txt`): carrier-pkg fixes apps-rollup
60.5 KB but breaks 22.4 KB there and 34.1 / 27.9 / 45.7 KB on apps-esbuild / apps-webpack / pop-esbuild.
No rule reaches dl25k's coverage class. So nothing here makes dl25k shippable; exp134's `copy_presence_context`
remains its carrier fix.

## 3. The keys (all default off; tests red first)

All in `[library.discovery]`; the three terser keys run only in a bundle that reads the terser-built slices
(`library.lookup.terser_slices` union, or auto with a union verdict), so they cannot change any other bundle.

| key                       | default | what it does                                                                                                                                                                                                                                                                                                                                                         |
| ------------------------- | ------: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `terser_carriers`         |     0.0 | (b') A package the bundle does not show (no declared/banner seed in its family; its family's exclusive seeds under `terser_carriers_own` of its seed bytes), at least this share of whose seed bytes a shown family scores as well, is a carrier: its terser-made seeds go to such a rival when the rival scores the unit (the one with the most exclusive bytes), else to no one. |
| `terser_carriers_own`     |     0.1 | The exclusive share that shows a family, for `terser_carriers`.                                                                                                                                                                                                                                                                                                         |
| `terser_factory_majority` |     0.0 | A terser-made seed on a cut CommonJS factory's statement is dropped (fenced) when another family's index seeds in that factory hold at least this many times its family's there.                                                                                                                                                                                      |
| `terser_thin`             |     0.0 | A family every seed of which is terser-made, holding under this share of its smallest index module, is dropped. Inert in the lab at 0.05 and 0.2 (cost recall at 0.2); not recommended.                                                                                                                                                                                |
| `terser_payload_literal`  |   false | When the bundle reads the terser slices: the payload guard (`payload_min_bytes`) takes a long literal in the claimed family's files as its own evidence only when no other family's files hold it too. Found in the lab's residue: 32 KB of `entities`' decode table propagated to `markdown-it` (whose UMD dist carries it) on remin-terser, 36 KB on remin-esbuild even without terser. First built global (`payload_unique_literal`, `28c1de28`); global it costs apps/pop recall (section 4), so it is terser-gated now. |

Lookup support: `LibraryIndex::reads_terser`, `structural_scores_chosen` (the chosen slice alone), and a test
constructor `from_slices` with a terser slice. Tests (watched red by disabling each rule, then green):
`terser_carriers_give_a_plugin_s_terser_made_ties_to_the_host_the_bundle_shows`,
`terser_factory_majority_drops_a_terser_made_minority_seed_inside_another_package_s_file`,
`terser_thin_drops_a_family_shown_only_by_a_sliver_of_a_far_bigger_index_module`,
`terser_payload_literal_takes_no_backing_from_a_long_string_another_family_also_holds`,
`the_exp139_terser_presence_keys_default_off_and_set_from_the_file`.

Lab ladder on remin (prebuilt binary, lab labels not committed), terser auto + :

| keys                                                 | remin-terser P / R / F0.5 | wrong KB | remin-esbuild P / F0.5 |
| ---------------------------------------------------- | ------------------------: | -------: | ---------------------: |
| none (`*exp138-both*`)                               |    99.322 / 53.31 / 84.700 |    339.8 |        99.720 / 84.173 |
| carriers 0.5                                         |    99.718 / 53.31 / 84.933 |    140.9 |        99.720 / 84.173 |
| + factory majority 4                                 |    99.807 / 53.21 / 84.932 |     96.0 |        99.720 / 84.173 |
| + factory majority 2                                 |    99.845 / 53.19 / 84.945 |     76.9 |        99.720 / 84.173 |
| + factory majority 1                                 |    99.852 / 53.19 / 84.948 |     73.5 |        99.720 / 84.173 |
| + factory majority 4 + thin 0.05 / 0.2               | 99.807 / 53.21 / 84.932 (0.05); 99.814 / 53.15 / 84.904 (0.2) | 96.0 / 92.6 | 99.720 / 84.173 |
| carriers 0.5 + factory majority 1 + payload_unique_literal (then global) | 99.917 / 53.16 / 84.968 | 41.3 | 99.781 / 84.191 |
| base (`*exp138-helper*`, terser off)                 |    99.902 / 20.33 / 56.043 |     18.6 |        99.719 / 84.129 |

## 4. Results (scorer 14, structural rule, both scopes, k2-all-r5-origin-prod, every corpus)

Labels, all at main's defaults plus:

- `*exp138-helper*`: nothing (main's defaults; exp138's label, re-scored under scorer 14 here);
- `*exp139-keys*`: the keys (`terser_carriers=0.5`, `terser_factory_majority=1`, `terser_payload_literal=true`),
  built at `b7333e9a`;
- `*exp138-both*`: `terser_slices=auto`, `terser_share=0.2` (re-scored under scorer 14);
- `*exp139-both*`: terser auto + the keys, built at `b7333e9a`.

P / R / F0.5 in %; deltas against main's defaults (P / F0.5). test / test2: summaries only (test2 frozen, never
diagnosed). Bar for the last column: P >= base, F0.5 >= base - 0.002, app claimed 0. (`scripts/table139.py`,
`data/table.json`.)

| set           | main defaults            | + keys                   | + terser auto            | + terser auto + keys     | d keys          | d terser             | d terser + keys      | bar      |
| ------------- | ------------------------ | ------------------------ | ------------------------ | ------------------------ | --------------- | -------------------- | -------------------- | -------- |
| cc            | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | 100.000 / 81.90 / 95.768 | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| esbuild-app   | 99.078 / 74.74 / 93.020  | 99.078 / 74.74 / 93.020  | 99.078 / 74.74 / 93.020  | 99.078 / 74.74 / 93.020  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| heldout       | 99.636 / 68.56 / 91.354  | 99.636 / 68.56 / 91.354  | 99.636 / 68.56 / 91.354  | 99.636 / 68.56 / 91.354  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| wrangler      | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | 99.069 / 33.14 / 70.870  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| apps-esbuild  | 99.908 / 77.65 / 94.490  | 99.908 / 77.65 / 94.490  | 99.908 / 77.65 / 94.490  | 99.908 / 77.65 / 94.490  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| apps-rollup   | 99.112 / 79.31 / 94.398  | 99.112 / 79.31 / 94.398  | 99.112 / 79.31 / 94.398  | 99.112 / 79.31 / 94.398  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| apps-webpack  | 99.599 / 73.50 / 92.994  | 99.599 / 73.50 / 92.994  | 99.599 / 73.50 / 92.994  | 99.599 / 73.50 / 92.994  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| dev-rollup    | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | 99.873 / 77.53 / 94.430  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| dev-webpack   | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | 99.642 / 62.89 / 89.216  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| pop-bun       | 99.518 / 77.21 / 94.082  | 99.518 / 77.21 / 94.082  | 99.518 / 77.21 / 94.082  | 99.518 / 77.21 / 94.082  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| pop-esbuild   | 99.315 / 78.71 / 94.373  | 99.315 / 78.71 / 94.373  | 99.315 / 78.71 / 94.373  | 99.315 / 78.71 / 94.373  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| remin-esbuild | 99.719 / 51.76 / 84.129  | 99.719 / 51.76 / 84.129  | 99.720 / 51.84 / 84.173  | 99.720 / 51.84 / 84.173  | +0.000 / +0.000 | +0.000 / +0.044      | +0.000 / +0.044      | PASS     |
| remin-terser  | 99.902 / 20.33 / 56.043  | 99.902 / 20.33 / 56.043  | 99.322 / 53.31 / 84.700  | 99.917 / 53.16 / 84.968  | +0.000 / +0.000 | **-0.580** / +28.657 | **+0.015** / +28.925 | **PASS** |
| test-bun      | 99.557 / 70.16 / 91.860  | 99.557 / 70.16 / 91.860  | 99.557 / 70.16 / 91.860  | 99.557 / 70.16 / 91.860  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test-esbuild  | 99.272 / 73.37 / 92.726  | 99.272 / 73.37 / 92.726  | 99.272 / 73.37 / 92.726  | 99.272 / 73.37 / 92.726  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test-rollup   | 98.966 / 72.43 / 92.210  | 98.966 / 72.43 / 92.210  | 98.966 / 72.43 / 92.210  | 98.966 / 72.43 / 92.210  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test-webpack  | 99.483 / 72.18 / 92.485  | 99.483 / 72.18 / 92.485  | 99.483 / 72.18 / 92.485  | 99.483 / 72.18 / 92.485  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test2-bun     | 99.361 / 69.43 / 91.475  | 99.361 / 69.43 / 91.475  | 99.361 / 69.43 / 91.475  | 99.361 / 69.43 / 91.475  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test2-esbuild | 99.649 / 71.04 / 92.222  | 99.649 / 71.04 / 92.222  | 99.649 / 71.04 / 92.222  | 99.649 / 71.04 / 92.222  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test2-rollup  | 96.448 / 75.73 / 91.443  | 96.448 / 75.73 / 91.443  | 96.448 / 75.73 / 91.443  | 96.448 / 75.73 / 91.443  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |
| test2-webpack | 99.289 / 70.08 / 91.648  | 99.289 / 70.08 / 91.648  | 99.289 / 70.08 / 91.648  | 99.289 / 70.08 / 91.648  | +0.000 / +0.000 | +0.000 / +0.000      | +0.000 / +0.000      | PASS     |

- **+ keys alone is identical to main's defaults on every set**: all 357 dev cards have identical totals, scopes and
  claimed bytes (every key is gated on reading the terser slices).
- **+ terser auto + keys** is identical to terser auto everywhere but remin-terser, where it removes 298.6 KB of
  wrong claims and 144.5 KB of right ones (recall 53.31 -> 53.16; the moved forge chunk ends unclaimed under the
  factory tie fence: vite and the plugin tie on every statement of it). Wrong bytes 339.8 -> 41.3 KB.
- remin-terser's remaining 41.3 KB (`data/census-after.json`): 16 KB `jju` for `liquid-json` (a fork; the truth's
  package scores nothing), `import-meta-resolve` forks for `@babel/core` (~10 KB), `d3-path` in `victory-vendor`,
  `@vscode/sqlite3` for `sqlite3`, `open`'s split-out helpers: shippers the index does not hold for the code.
- **The global payload variant** (`*exp139-key*`, `payload_unique_literal` at `28c1de28`, not terser-gated): P
  +0.06 on remin-esbuild, +0.05 test2-rollup, +0.03 test2-webpack, but recall -0.44 (apps-esbuild), -0.69
  (apps-rollup), -0.67 (pop-bun), -0.66 (pop-esbuild): F0.5 -0.13 / -0.20 / -0.20 / -0.19, held-out -0.006,
  test2-bun -0.007. A data table more than one family ships is usually the claimer's after all. Dropped for the
  terser-gated key.
- dl25k + key: not scored. Every key is terser-gated and dl25k's sets read no terser slice, so it is inert by
  construction (2 x 70 min saved).

## 5. Reproduce

```bash
S=experiments/139-carriers/scripts; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
J=/work/exp139-data/judge   # judge13.py rows (name packages: outside the repo)
$PY -I experiments/133-dl25k-r5/scripts/judge13.py remin $J/remin-helper-both.jsonl remin-exp138-helper remin-exp138-both
$PY -I $S/carriers.py remin $J/remin-helper-both.jsonl c1 remin-exp138-both /work/exp135-data/full/registry $J/remin-both-seeds.jsonl
$PY -I $S/census139.py $J/remin-helper-both.jsonl $J/remin-both-seeds.jsonl --json census-remin.json
#  dl25k: judge13.py <corpus> $J/<c>-dl.jsonl exp133-base<..> exp133-dl<..>; carriers.py ... /work/exp133-data/full/registry
$PY -I $S/simcar.py $J/remin-both-seeds.jsonl --alpha 1.0 --own 0.1 --cshare 0.5 --terser   # and the k2 / dl25k dossiers
$PY -I $S/simfactory.py $J/remin-helper-both.jsonl c1 $J/remin-both-seeds.jsonl --k 1
K="--pipeline-set library.discovery.terser_carriers=0.5 --pipeline-set library.discovery.terser_factory_majority=1 --pipeline-set library.discovery.terser_payload_literal=true"
T="--pipeline-set library.lookup.terser_slices=auto --pipeline-set library.lookup.terser_share=0.2"
experiments/135-noprint-census/scripts/score-all.sh exp139-both /work/exp135-data/full/ix/k2-all-r5-origin-prod <log dir> $T $K
experiments/135-noprint-census/scripts/score-all.sh exp139-keys /work/exp135-data/full/ix/k2-all-r5-origin-prod <log dir> $K
python3 -I $S/table139.py exp138-helper exp139-keys exp138-both exp139-both
```


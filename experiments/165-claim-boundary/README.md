# exp165 — the names-mode claim boundary: census only, STOP

STATUS: CENSUS COMPLETE — STOP, nothing built (2026-10-30). Branch
`exp165-claim-boundary` off main `6e5f16c93`. The task's stop rule fired:
**every candidate boundary rule's recall cost is measured in megabytes of
library code while its mint gain is zero** — no claim ever covers the junk
statements on either leg of the hop, so no rule that moves the boundary can
reach them. The unlock decision stands on exp164's numbers. This file also
CORRECTS exp164's mechanism claim (rule 9: the retraction lives here, in the
newer file).

## The question

exp164 landed the mint premium's run-side fixes (mints 685 → 501 on the
2.1.215→2.1.216 hop, −184, 30× the band) and attributed the remaining ~500
fillers to "the claim boundary freezing app statements swallowed by claimed
init regions" — jose's region allegedly holding Claude Code's plugin
installer, zod's holding `adjustTokenCount`/`mapperReducer`. The hypothesis:
bundlers merge app code into the same containers as library code, the claim
freezes the whole container, and the app code is placed under `vendor/<pkg>/`
with raw minifier names. exp165 was to census that boundary and, if a rule
with positive net existed, build it.

## Method

The `discover` verb reproduces the run's discovery exactly (verified: the
2.1.216 run's 22,674 claims are byte-identical to `discover-cc-216.json`'s).
Two corpora:

- **bun-dev** — 89 bundles (experiments/096 corpus) with FULL source-map
  truth: every unit's bytes are attributable to app or a library package.
  This is the population where the trade can be measured exactly.
- **cc** — the 2.1.215 + 2.1.216 bundles against the labelled 620-unit
  sample (`/work/cc-library-truth/2.1.216/labels.jsonl`; only the sample's
  labels are usable — most units are unlabelled there).

Reports: `humanify discover` with `--unit-prints --call-context
--bundle-order`, names-mode config, on the full names index (cb64c1f5).
Census scripts: `census.py`/`census2.py` (composition + trade),
`census3.py` (the support-gate family simulated at five thresholds),
`census4.py` (the R-support rule exactly as it would build).

## Census 1 — composition: claimed containers hold ~zero app bytes

bun-dev, 89 bundles, aggregated (`census-bundev.txt`). Every unit inside a
MOVED container, by class and truth:

| class (the claimed family holds…)                   | units   | bytes    | truth                               |
| --------------------------------------------------- | ------- | -------- | ----------------------------------- |
| held (family holds a print)                         | 154,231 | 143.6 MB | 143.4 MB library / 0.24 MB unmapped |
| sub-floor (every print under the 20-node row floor) | 172,209 | 5.9 MB   | 5.86 MB library / 52 KB unmapped    |
| cross-claim (other families hold it)                | 11,315  | 3.27 MB  | 3.26 MB library / 9 KB unmapped     |
| no-family (prints exist, no family holds any)       | 110     | 205 KB   | 201 KB library / 5 KB unmapped      |
| unclaimed interior (no claim)                       | 4,861   | 0.94 MB  | 0.94 MB library / 0.6 KB unmapped   |

Plus 13,833 unclaimed-fold factories that moved on the whole-module key:
13.6 MB, 13.58 MB of it library.

**Not one `*_app_*` key appears anywhere in the bun-dev composition** — with
full source-map truth, not a single app-labelled byte sits inside any moved
container, any stayed claimed container, or any clean/partial/contested lazy
group, across 89 bundles. The app-swallowing the hypothesis predicted does
not happen at the claim boundary.

cc agrees where its sample can speak: the trade table has exactly one
app-labelled entry, `('group', 'partial', 'no-family_app'): 4` units /
1,448 bytes — and PARTIAL groups do not move. Zero app-labelled bytes in any
moved container.

## Census 2 — the trade table: every rule pays library bytes

Stranger bytes (claimed-container bytes the claimed family holds nothing of)
a rule would eject, vs the family bytes the same rule drags into the normal
naming flow (`census-bundev.txt` trade block):

| rule                                                                        | stranger bytes ejected                                                        | composition                                                                               |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| R-eject (every stranger inside a moved container returns to naming)         | **26.2 MB** (26.16 MB library + 76 KB unmapped)                               | 10.35 MB clean-fold factories + 13.6 MB whole-module factories + 2.3 MB clean lazy groups |
| R-composition (refuse the wholesale move when strangers exceed a threshold) | **12.6 MB** (12.58 MB library) — plus the container's family bytes go with it | clean-fold population                                                                     |
| R-fold-min (a fold resting on too few claims does not move)                 | **10.3 MB** (10.25 MB library)                                                | claimed-not-held population                                                               |

All three are library code (the unmapped slivers are 0.3% and unproven app).
The cost is not just bytes: ejecting them hands megabytes of vendor code to
the LLM naming flow — thousands of asks and exactly the placement/naming
churn names mode exists to remove.

cc: `R-eject` stranger = 13.8 KB library-labelled + 1.46 MB unlabelled
(the sample cannot label the rest); `R-composition` stranger = 13.8 KB +
463 KB unlabelled. Nothing app-labelled.

## Census 3 — the support-gate family, simulated at five thresholds

`census3.py` gates a move on how much of the moved content the answering
family HOLDS (≥1 held unit / ≥2 / G2 = verifiable-and-zero-held / ≥25% /
≥50% held bytes). bun-dev: even the loosest gates block thousands of
factories whose bytes are library through and through — gate G2 blocks
4,466 whole-module factories / 4.3 MB, of which 3.66 MB cross-claim library

- 0.66 MB sub-floor library, **0 app**; gate ≥50% blocks 9,089 factories /
  6.2 MB, 3.76 MB cross-claim library. cc: gate G2 blocks 246 whole-module
  factories (135 KB, all unlabelled) — and `census4.py` (the R-support rule
  exactly as it would build, requiring the answering family to hold a print of
  an over-floor unit among the dragged statements) **blocks 0 factories on
  either corpus**: on cc nothing qualifies, and on bun-dev every whole-module
  factory sits outside any placement group, so the rule has nothing to
  evaluate — which is itself the finding (the support signal does not exist
  where the moves happen).

## Census 4 — the mechanism of the cc hop's 284 jose junk mints

The junk census (`/work/exp165-data/junk-census2-exp164.json`, 499 junk
rows) puts 284 of them in four `vendor/jose/` files. That folder holds ONLY
those four modules — and they are not jose. The chain, pinned at every layer:

1. The raw bundle contains TWO copies of Claude Code's plugin installer
   (copy A at ~9.01 M, copy B at ~15.37 M on 2.1.215 — both in the input
   bundles on this box).
2. **Copy A is eager and asked**: its statements sit before the region's
   LazyModule row; the fresh 2.1.215 run asked for them (rename batch at log
   line 7,253,326: `Bsr → installPluginFromMarketplace`, …) and filed them
   under `src/plugin-installer.js`.
3. **Copy B is lazy and never asked**: its two function statements sit
   bracketed INSIDE a lazy wrapper region (unit rows 77170 before,
   77183 after; group 4019 = 3 statements, **0 claims**). In the 395 MB
   fresh-run log its content appears ONLY as callsite context —
   `async function Fzp` = 0 hits, its unique template literal = 1 hit (a
   callsite context line) — against a positive control of 7,392 hits
   (MEMO_CACHE_SENTINEL), so the prompts do carry statement text. Its
   bindings kept raw minifier tokens: Fzp / $zp / Bzp + all-minified
   params = the mints.
4. Copy B is an era fossil module (split-ledger indexes 4019/4025/4027/
   4033), NOT an extracted factory: absent from both `_bun-modules.json`
   manifests (1,624 / 1,647 entries) and with no `bundleOrder` row. Its
   wrapper DID reach the module step's file-naming prompts (m1..m8 batches
   describe it as "Imports modules that declare: … | Fzp, $zp"), so the era
   routed the module at file level while skipping its statements.
5. The fresh run filed it under `vendor/jose/module-*.js` (fossil stems).
   **The origin of the folder name "jose" is UNRESOLVED**: the token appears
   in no LLM response anywhere in the log, in no index module answer, in no
   claim, and not in the prior; real jose code went to `src/jwt-core`.
6. On 2.1.216 nothing re-decides: the file path is inherited by placement
   (`fossil:unique-signature`, `priorFileFrom hash` — placement trails
   30195-97) and the minified names by exact-match from the prior (trails
   152363/64/152450: frf/mrf/hrf → Fzp/$zp/Bzp, reason
   "external-reference"). **No claim ever covers the junk statements on
   either leg** — verified against the run's own discovery (22,674 claims,
   0 overlapping the installer's spans) and the raw discovery alike.

The same falsifies exp164's attribution: the junk statements are NOT "swallowed
by claimed init regions" — they are not inside any claimed region at all.

## Decision — STOP

The mint gain of every boundary rule is ~0 (its mechanism cannot reach
statements no claim ever touched), and its recall cost is measured:
10–26 MB of library code pushed back into the LLM naming flow per corpus
plus the rule's own complexity and LLM-ask volume. Net negative on both
terms — the task's stop rule fires plainly.

**The unlock decision stands on exp164's numbers**: mints 685 → 501 (−184,
30× the band), restoration 9,016 applications / 13,031 restored names at
99.98% cross-version stability, novel/realLn byte-equal, neutrality NEUTRAL
both modes, gate 13/13.

## The follow-up lever (what the census points at instead)

Not the claim boundary — the **lazy-module routing**: an unclaimed lazy
fossil module's statements are never asked (the era routes the module at
file level through the module step), and the placement stage files it under
a vendor package, where the next version's hash-tier placement and
exact-match carry it forever. Two open sub-questions, either of which sizes
the lever:

- why an unclaimed, unmoved lazy module's interior statements are excluded
  from the wave asks (the gate site is not pinned);
- who names the `vendor/jose/` folder (see above — unresolved).

A rule there would recover the 284 jose junk mints (and possibly part of the
zod 167) without touching the index or the claims.

## Artifacts

Everything CC-derived stays out of the repo (repo is PUBLIC); the census
scripts and the bun-dev/count-only outputs are committed here. The discover
reports, junk censuses and log greps live in `/work/exp165-data/` and
`/tmp/eval-work/exp164-names-hop/`.

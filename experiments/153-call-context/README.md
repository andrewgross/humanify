# exp153 — the call-context rule (library-names plan step 2)

STATUS: DONE (2026-10-26). Census executed, variants simulated and measured, one shot at
`bun-held`. **The default is unchanged (the key ships OFF); every number here is an
off-default measurement. The variant that clears the dev bar is `tie-break` — see the
verdict.**

A unit match is supported when the unit's own calls line up with what the index records
for the matched print: the bundle's unit references units that the index says the matched
print references, for the same owner or family. The test is **containment, not equality**
— a bundled app is a tree-shaken subset of the library build, so callers can be dropped,
but a kept function's callees are still its callees. The rule reads only INSIDE and
OUT-OF edges (rule spec `matching::rules::lib::CALL_CONTEXT`; INTO edges are never
identity evidence — matching-architecture §4), and needs a `/4` index: a `/3` one
answers "not recorded" and the rule stays silent.

## What was built (default OFF)

- `crates/humanify-core/src/libindex/callctx.rs` — the engine: the bundle's per-unit
  callees with prints (`bundle_callees`), the containment fit of a unit's callees against
  a family's index rows (`Fit`), and the whole-call-tree hash (iterative Tarjan, SCCs
  collapsed, `tree_hashes`) measured by the census as an alternative signal.
- `crates/humanify-core/src/libindex/lookup.rs` — `CallContextRules` decides what the
  fit may do: `tie-break` (a structural tie goes to the one tied package it confirms),
  `support` / `support-or` (it replaces / joins the position window and the isolation
  veto's neighbour test), `veto` / `veto-or` (the veto's test only), and the relaxers
  `relax-floor` (a whole print of 20 nodes up is looked up for the owners it confirms)
  and `relax-common` (a print held by more than `max_packages` owners is narrowed to
  those it confirms). Named settings under config key `library.lookup.call_context`
  ("off" default) with `library.lookup.call_context_callees` = `all` (every callee with a
  print) or `rowed` (at or over the 20-node row floor).
- The isolation veto reads the same confirmed fit through
  `libdetect::discovery::Containment::call_confirms`.
- The census instrument: `humanify discover --call-context` adds per-unit rows
  (callees, per-family fits, tie resolution, missing-callee classification) to the
  discovery report and **changes no claim** — verified on every census bundle against
  the same-index baseline label's report (90/90 bun-dev, 1/1 cc, 0 mismatches).

The cascade's callee evidence (`matching::cascade::filter_by_callee_hashes`) asks a
different question — equality of sorted callee-hash arrays across two WHOLE programs —
and is not reused: the library side reads a subset of a build, so it asks containment
over statement units, on purpose. Both readers are declared in
`docs/responsibility.md`.

## The census (the full Bun /4 index, `53a1d874...`, main defaults, structural rule)

Run on the `bun-dev` (45 apps, 90 bundles) and `cc` corpora; aggregates in
`results/census-full-{bun-dev,cc}.json`. "Confirm" = every callee the census counted is
one the family's rows record; "refute" = a callee the family's rows lack. `fit` counts
every callee with a print, `rowed` only callees at/over the index's 20-node row floor.

| set, decision class (fit)                             | confirms (right)            | confirms (wrong) | refutes (right) | says nothing (right) |
| ----------------------------------------------------- | --------------------------- | ---------------- | --------------- | -------------------- |
| bun-dev-bun, claims (kept index seeds)                | 45,947 KB                   | 59 KB            | 9,103 KB        | 19,737 KB            |
| bun-dev-node, claims                                  | 49,820 KB                   | 95 KB            | 9,230 KB        | 20,311 KB            |
| cc, claims                                            | 1,102 KB                    | 0 KB             | 174 KB          | 135 KB               |
| bun-dev-bun, isolation-veto drops                     | 230 KB                      | 0 KB             | 151 KB          | 3,314 KB             |
| bun-dev-node, isolation-veto drops                    | 180 KB                      | 7 KB             | 140 KB          | 2,898 KB             |
| bun-dev-bun, window-refused picks                     | 160 KB                      | 0 KB             | 3 KB            | 725 KB               |
| bun-dev-node, window-refused picks                    | 166 KB                      | 3 KB             | 3 KB            | 727 KB               |
| bun-dev-bun, ties: signal resolves to one family      | 1,454 KB right / 2 KB wrong |                  |                 |                      |
| bun-dev-node, ties                                    | 1,347 KB right / 5 KB wrong |                  |                 |                      |
| bun-dev-bun, 20–29-node floor units (unclaimed today) | 673 KB right / 1 KB wrong   |                  |                 |                      |
| bun-dev-node, floor units                             | 1,583 KB right / 1 KB wrong |                  |                 |                      |
| bun-dev-bun, too-common units (>3 owners, unclaimed)  | 28 KB right / 2 KB wrong    |                  |                 |                      |

(ties on cc resolved to nothing; floor units on cc: none. Unit counts and the `rowed`
and `tree` signals are in the aggregate jsons.)

Readings that decided the variants:

- **Refutation is unusable**: 9.1–9.2 MB of RIGHT claim bytes per bun-dev set refute —
  an index-scope gap (the callee is in another package's row, or under 20 nodes so no
  row records it) reads as "the family's rows lack it". The `refute` guard variant
  confirms this: −9.1 MB right for +36 KB wrong avoided.
- **The Replace flavours are catastrophic** (`support`: the window/veto replaced by call
  context): most right claims' rows confirm, but 28–66 MB per set are kept by the window
  alone and unconfirmed — replacing drops them. `tree`'s confirms-too-rarely variant of
  this is the worst. The Or flavours keep the window and add what call context
  confirms.
- **The whole-call-tree hash is dominated by `fit`**: it confirms strictly less
  (12.8 MB vs 45.9 MB right on bun-dev-bun claims — a whole-tree match is far rarer
  than callee containment), it cannot refute, and on unresolved ties it resolves to the
  WRONG family more often than `fit` (40 KB vs 2 KB wrong on bun-dev-bun). Not built
  into a rule.
- `rowed` ≈ `fit` on ties and rescues, weaker on floor units (94–95 KB vs 594–1,583 KB
  right: small callees are most of a small unit's evidence). `all` is the default and
  the measured setting.

## The simulation (first-order, per-set deltas priced against the re-scored /4 base)

`results/sim-full.json` (documented there as first-order only: nothing propagates). The
per-variant headline on bun-dev-node, `fit` signal: `tie-break` +23 KB right / −23 KB
wrong (precision up), `support-or` +347 KB right / +11 KB wrong, `veto-or` +181/+7,
`relax` +1,621 KB right / +4 KB wrong, `tie-break+support-or+relax` ≈ +1.99 MB right /
−8 KB wrong. First-order, the full combination looks best on every dev set and passes
the precision bar everywhere. The measured runs below are what decided instead — the
census cannot price propagation (a rule-11 shape: the first-order sim said the
relaxers were precision-neutral; the pipeline's new seeds propagate into some wrong
families and the measured precision on bun-dev-node fell −0.011 to −0.013 points).

## The measured rule (bun-dev and cc, full /4 index, re-scored /4 base first)

Index change isolated first (same commits, `/3` → `/4`): bun-dev-bun +60 KB named,
bun-dev-node −279 KB named, cc byte-identical; wrong and app bytes unchanged on every
set. Then each variant vs its same-commit `/4` base (`--pipeline-set
library.lookup.call_context=<name>`, ties and relaxers as simulated, `callees=all`):

| set          | variant                    | P (Δpt)         | R (Δpt)         | F0.5 (Δpt)     | wrong KB | app |
| ------------ | -------------------------- | --------------- | --------------- | -------------- | -------- | --- |
| bun-dev-bun  | base                       | 99.825          | 81.593          | 95.554         | 146.4    | 0   |
|              | tie-break                  | 99.828 (+.003)  | 81.640 (+.047)  | 95.569 (+.015) | 144.2    | 0   |
|              | relax-floor                | 99.827 (+.002)  | 82.427 (+.835)  | 95.783 (+.229) | 146.3    | 0   |
|              | tie-break+relax            | 99.827 (+.002)  | 83.250 (+1.657) | 96.004 (+.449) | 147.7    | 0   |
|              | tie-break+support-or+relax | 99.769 (−.056)  | 84.471 (+2.878) | 96.282 (+.727) | 199.7    | 0   |
| bun-dev-node | base                       | 99.742          | 82.702          | 95.795         | 228.7    | 0   |
|              | tie-break                  | 99.747 (+.005)  | 82.774 (+.072)  | 95.817 (+.023) | 224.8    | 0   |
|              | relax-floor                | 99.731 (−.011)  | 84.341 (+1.638) | 96.220 (+.425) | 243.2    | 0   |
|              | tie-break+relax            | 99.729 (−.013)  | 85.169 (+2.467) | 96.432 (+.637) | 247.5    | 0   |
|              | tie-break+support-or+relax | 99.722 (−.021)  | 86.278 (+3.576) | 96.708 (+.913) | 257.7    | 0   |
| cc (sample)  | base                       | 100.000         | 81.903          | 95.768         | 0.0      | 0   |
|              | tie-break                  | 100.000 (+.000) | 81.995 (+.092)  | 95.793 (+.025) | 0.0      | 0   |
|              | relax-floor                | 100.000 (+.000) | 82.546 (+.644)  | 95.943 (+.175) | 0.0      | 0   |
|              | tie-break+relax            | 100.000 (+.000) | 82.746 (+.843)  | 95.996 (+.229) | 0.0      | 0   |
|              | tie-break+support-or+relax | 100.000 (+.000) | 84.082 (+2.180) | 96.352 (+.584) | 0.0      | 0   |

The bar (P ≥ base and F0.5 ≥ base − 0.002 on every dev set, app claimed 0): **only
`tie-break` passes on every dev set** — precision up on all three sets, wrong bytes
down on both bun sets, app 0 everywhere. Every variant with a relaxer in it clears F0.5
easily but loses precision on bun-dev-node to propagation; the full combination also
loses 53 KB wrong-family bytes on bun-dev-bun.

### bun-held, one shot (summaries only, never diagnosed)

| set           | variant   | P (Δpt)        | R (Δpt)        | F0.5 (Δpt)     | wrong KB | app |
| ------------- | --------- | -------------- | -------------- | -------------- | -------- | --- |
| bun-held-bun  | base      | 99.974         | 83.591         | 96.203         | 5.5      | 0   |
|               | tie-break | 99.944 (−.030) | 83.550 (−.041) | 96.170 (−.033) | 11.9     | 0   |
| bun-held-node | base      | 99.708         | 80.986         | 95.302         | 64.9     | 0   |
|               | tie-break | 99.712 (+.004) | 81.045 (+.059) | 95.321 (+.019) | 64.1     | 0   |

The held-out confirmation is MIXED: bun-held-node moves with the dev sets (precision
and F0.5 up, wrong bytes down), but bun-held-bun moves against them — precision −0.030
points and wrong-family bytes 5.5 → 11.9 KB, with recall down slightly too. Absolute
scale is tiny (6.4 KB of wrong-family bytes on ~30 MB named), the measured band is 0,
and the sign is real.

## Verdict

**The default stays OFF; nothing flips without Andrew's call.** The call-context rule is
built, census-priced, measured on dev and shot once at the held split, and it ships
off-default with every variant a named setting away:

- `tie-break` — the only dev-bar-clean variant (precision up everywhere, wrong bytes
  down on both bun dev sets), but the held-out bun target regresses (−0.030pt P,
  +6.4 KB wrong). If the held split is the judge, this does not ship either.
- `tie-break+relax` and the full combination — real F0.5 wins (+0.6 to +0.9 points on
  the bun dev sets, +2.2pt recall on cc) that trade precision points on bun-dev-node;
  they fail the brief's P bar and are documented for Andrew as the recall lever.

The census itself (the plan's question: would call context have confirmed or refuted
today's decisions, right or wrong?) is overwhelmingly positive on claims — 45.9/49.8 MB
right vs 59/95 KB wrong per bun-dev set — and that is what makes the tie-break and
support-Or roles safe-ish; refutation and the Replace flavours are priced out by the
index's scope gaps, and the whole-tree hash is priced out by `fit` dominating it.

## Gate and neutrality

| check                                                                   | result                                                                                    |
| ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `npm run check` (13 stages), branch `exp153-call-context` @ `710a5269e` | ALL 13 STAGES PASSED                                                                      |
| warm neutrality vs main `14d3474a6`, 2.1.85→2.1.86, default             | NEUTRAL: 0 files / 0 lines, both legs +0 cache writes, exit 0 / 0                         |
| warm neutrality vs main `14d3474a6`, report mode, the full `/4` index   | NEUTRAL: 0 files / 0 lines, +0 / +0, exit 0 / 0; `library-report.json` byte-identical too |

Both neutrality runs used a dead endpoint (`--endpoint http://127.0.0.1:9/v1`), so no
prompt could reach a model: every answer was replayed from `/work/neutrality-cache`.
The key-off config serializes nothing (a default config dump is byte-unchanged), which
is why the default-mode legs and configs are byte-identical.

## Where things live

- Labels (committed): `experiments/096-library-matching-score/results/{bun-dev,cc,bun-held}-exp153-ix4-*`
  (bun-held: summaries only, per corpus privacy).
- Census reports and run logs: `/work/exp153-data/census/`, `/work/exp153-data/*.log`.
- The full `/4` index: `/work/library-index/53a1d874...` (exp152's; `current` still
  points at the `/3` index and was not moved).

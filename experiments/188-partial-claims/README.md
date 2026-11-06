# exp188: claiming library statements whose matched-function cover is under 50%

## STATUS (2026-11-06): MEASURED, NOT ADOPTED. Measurement only, on branch `exp188-partial-claims`. Not for merge.

- **Totals first.** Lowering the structural rule's cover floor (`library.lookup.cover`, 0.5 by default) to 0.3
  or 0.1 raises recall by **+0.02 to +0.10 points** on the Bun corpora and **+0.87 points on Claude Code** (one
  sampled unit). It costs precision on the held-out apps: **bun-held-bun 99.975% -> 99.916%**, because the new
  claims there are 13 KB wrong for every 5 KB right. App code claimed stays 0 everywhere. Only the Claude Code gain
  passes the brief's 0.5-point bar, and it rests on one sample unit.
- **The census ceiling was 0.3 to 0.9 points. The real run delivers about 7% of it.** The census is an emulation
  of the structural seed step only. The bytes it counted as reachable sit mostly in a few large pre-built library
  statements (on bun-dev, `@microsoft/applicationinsights-*` copies inside `@vscode/extension-telemetry`: 1.2 MB
  of the 1.6 MB in the 30-50% buckets). The run never claims those, even with `cover = 0.01`,
  `max_packages = 50`, `evidence_conflict = false` and `helper_families = 0`. So a later stage drops them, and
  that stage was not found here (see "What this could not prove").
- **Function-sized claims were not built.** A claim names a whole UNIT (`UnitClaim.unit`), so there is no claim
  shape for "these functions inside a statement". The scorer could not credit one either. On a source-map truth
  it pairs claims with units by exact `(start, end)`, so a sub-statement claim is silently dropped. On the Claude
  Code sample it credits by containment, so it would credit a whole sampled unit and over-count. Its census
  ceiling is also under 0.5 points (<= 0.28 in the 30-50% buckets), and below 30% cover it is mostly wrong.
- **No code changed.** The threshold already is a registry key (`library.lookup.cover`, a flag and a config
  key), so the variants needed only `--pipeline-set`. The binary is main's `f8bbb20a`.
- **Recommendation:** keep `cover = 0.5`. The lever left in this population is the **>= 50% "guard refused"**
  bucket. Those units already pass the cover, and the run still leaves them unclaimed: 4.4-4.6 points of recall
  on bun-dev, ~2 on bun-held, 0.9 on Claude Code by the emulation. Which guard refuses them is the next question.

## 1. The census (no behaviour change)

`census.py <label> --corpus <c>` reads a label's discovery reports (`--unit-prints`) and the scorer's cached
index table, and runs the structural route with NO cover floor, the way mmlib's emulation does: whole print
first, else the largest non-overlapping function hits, with specificity and `max_packages` as shipped. It does
this for every scored unit (statement + factory-body scope) the run left unclaimed. The best package is judged
by the scorer's own `judge_origin`. Bytes are counted for the whole unit (what a lower floor would claim) and for
the matched function spans only (what a function claim would claim). Input: the `exp184-cb64-*` reports (cb64
index, commit 701d7631).

**Validation (rule 3):** of the run's own index-seed bytes, the emulation picks the same family at cover >= 0.5
for 98.9% (bun-dev-bun 78.68 / 79.52 MB), 98.9% (bun-dev-node), 98.2% (bun-held-bun) and 97.5% (bun-held-node).

KB. "+R" = recall points if every right byte in the bucket were claimed. Unit = whole-statement claim, fn = the
matched functions only.

| corpus        | cover bucket | library unclaimed | unit right / wrong / app | +R unit | fn right / wrong / app | +R fn |
| ------------- | ------------ | ----------------: | -----------------------: | ------: | ---------------------: | ----: |
| bun-dev-bun   | no hit       |             7,107 |                        - |       - |                      - |     - |
|               | (0,10)       |             2,483 |          983 / 1,499 / 0 |    0.94 |             2 / 10 / 0 |  0.00 |
|               | [10,20)      |               214 |             96 / 118 / 0 |    0.09 |            12 / 14 / 0 |  0.01 |
|               | [20,30)      |               918 |            130 / 788 / 0 |    0.12 |           31 / 150 / 0 |  0.03 |
|               | [30,40)      |               344 |              337 / 7 / 0 |    0.32 |            116 / 2 / 0 |  0.11 |
|               | [40,50)      |               453 |             442 / 11 / 0 |    0.42 |            183 / 4 / 0 |  0.17 |
|               | >=50 refused |             5,157 |          4,842 / 315 / 0 |    4.62 |        4,329 / 311 / 0 |  4.13 |
| bun-dev-node  | no hit       |             8,638 |                        - |       - |                      - |     - |
|               | (0,10)       |             2,443 |          960 / 1,483 / 0 |    0.88 |             2 / 11 / 0 |  0.00 |
|               | [10,20)      |               186 |              91 / 95 / 0 |    0.08 |            11 / 11 / 0 |  0.01 |
|               | [20,30)      |               912 |             56 / 856 / 0 |    0.05 |           14 / 179 / 0 |  0.01 |
|               | [30,40)      |               355 |             336 / 19 / 0 |    0.31 |            122 / 6 / 0 |  0.11 |
|               | [40,50)      |               293 |              290 / 3 / 0 |    0.27 |            123 / 1 / 0 |  0.11 |
|               | >=50 refused |             5,400 |          4,840 / 560 / 0 |    4.42 |        4,648 / 556 / 0 |  4.24 |
| bun-held-bun  | no hit       |             1,888 |                        - |       - |                      - |     - |
| (totals only) | (0,10)       |               372 |             45 / 327 / 0 |    0.17 |             3 / 12 / 0 |  0.01 |
|               | [10,20)      |               111 |              9 / 102 / 0 |    0.03 |             1 / 11 / 0 |  0.00 |
|               | [20,30)      |               725 |            162 / 563 / 0 |    0.62 |           36 / 129 / 0 |  0.14 |
|               | [30,40)      |                17 |               17 / 0 / 0 |    0.06 |              6 / 0 / 0 |  0.02 |
|               | [40,50)      |                 5 |                5 / 0 / 0 |    0.02 |              2 / 0 / 0 |  0.01 |
|               | >=50 refused |               624 |             575 / 49 / 0 |    2.20 |           541 / 49 / 0 |  2.07 |
| bun-held-node | no hit       |             3,186 |                        - |       - |                      - |     - |
| (totals only) | (0,10)       |               334 |              8 / 327 / 0 |    0.03 |             0 / 12 / 0 |  0.00 |
|               | [10,20)      |               110 |              8 / 102 / 0 |    0.03 |             1 / 11 / 0 |  0.00 |
|               | [20,30)      |               725 |            162 / 563 / 0 |    0.58 |           34 / 121 / 0 |  0.12 |
|               | [30,40)      |                15 |               15 / 0 / 0 |    0.06 |              6 / 0 / 0 |  0.02 |
|               | [40,50)      |                 5 |                5 / 0 / 0 |    0.02 |              2 / 0 / 0 |  0.01 |
|               | >=50 refused |               645 |             564 / 81 / 0 |    2.01 |           560 / 81 / 0 |  2.00 |
| cc (sample)   | no hit       |               362 |                        - |       - |                      - |     - |
|               | (0,10)       |                 0 |              0 / 0 / 47† |    0.00 |              0 / 0 / 1 |  0.00 |
|               | [30,40)      |                19 |               19 / 0 / 0 |    0.87 |              4 / 0 / 0 |  0.17 |
|               | >=50 refused |                21 |               21 / 0 / 0 |    0.93 |             21 / 0 / 0 |  0.93 |

† One sampled APP unit (47 KB estimated) has a function hit under 10% cover. A floor of 0.1 would put it at risk.
The real c10 run did not claim it (app claimed 0).

Reading it: under 30% cover, wrong outweighs right on every Bun corpus. From 30% to 50% it is almost all right.
That is why 0.3 was the threshold worth trying.

## 2. The real runs (exp096 runner, cb64 index, structural rule, main's binary `f8bbb20a`)

`ctl` = the default (cover 0.5) at the same commit. It reproduces `exp184-cb64-*` exactly on every set, so the
only difference between the variants and ctl is the cover key.

| corpus        | variant  | precision | recall | F0.5   | named KB | wrong KB | app |
| ------------- | -------- | --------- | ------ | ------ | -------- | -------- | --- |
| cc            | cb64/ctl | 100.000%  | 81.90% | 95.77% | 1,819    | 0        | 0   |
|               | c30      | 100.000%  | 82.77% | 96.00% | 1,839    | 0        | 0   |
|               | c10      | 100.000%  | 82.77% | 96.00% | 1,839    | 0        | 0   |
| bun-held-bun  | cb64/ctl | 99.975%   | 85.66% | 96.74% | 22,372   | 5.6      | 0   |
|               | c30      | 99.916%   | 85.68% | 96.70% | 22,378   | 18.9     | 0   |
|               | c10      | 99.916%   | 85.71% | 96.71% | 22,386   | 18.9     | 0   |
| bun-held-node | cb64/ctl | 99.711%   | 81.87% | 95.55% | 22,964   | 66.5     | 0   |
|               | c30      | 99.654%   | 81.89% | 95.51% | 22,970   | 79.7     | 0   |
|               | c10      | 99.654%   | 81.91% | 95.52% | 22,978   | 79.7     | 0   |
| bun-dev-bun   | cb64/ctl | 99.830%   | 83.94% | 96.19% | 87,904   | 149.9    | 0   |
|               | c30      | 99.830%   | 83.98% | 96.20% | 87,954   | 150.1    | 0   |
|               | c10      | 99.830%   | 84.04% | 96.22% | 88,018   | 150.2    | 0   |
| bun-dev-node  | cb64/ctl | 99.744%   | 83.15% | 95.92% | 91,115   | 234.2    | 0   |
|               | c30      | 99.743%   | 83.20% | 95.93% | 91,162   | 234.5    | 0   |
|               | c10      | 99.740%   | 83.25% | 95.94% | 91,220   | 237.4    | 0   |

Band 0 (exp096 `bands.json`: the score is deterministic), so every delta is real. It is also small: F0.5 moves
by at most +0.23 (cc) and -0.04 (bun-held-bun). bun-held-node and bun-dev-node were already under the 99.8% bar
on ctl, and both variants push them further under.

## 3. Where the ceiling went (bun-dev, a dev corpus)

Units the census put at 30-50% cover (unclaimed on ctl), and what the c30 run did with them:

| unit kind   | claimed | vetoed (isolation) | not claimed, covered >= 100 nodes | not claimed, < 100 nodes |
| ----------- | ------: | -----------------: | --------------------------------: | -----------------------: |
| FactoryBody |   77 KB |              15 KB |                          1,165 KB |                     8 KB |
| LazyBody    |   19 KB |                  - |                             14 KB |                     2 KB |
| Statement   |    4 KB |             157 KB |                                 - |                        - |

The 1,165 KB is almost all `@microsoft/applicationinsights-core-js` / `-web-basic`. These are large pre-built
statements inside `@vscode/extension-telemetry`, in four bundles (two apps x two targets). The run reports the
statement under `libraryClass` as `@vscode/extension-telemetry`, and it is in neither `claims` nor `vetoed`.
I re-ran discovery on one of those bundles with `cover = 0.01`, `max_packages = 50`, `evidence_conflict = false`
and `helper_families = 0`. It still made no claim on that statement.

## What this could not prove

- **Which stage drops the high-cover pre-built statements.** It is not the cover floor, the package cap, the
  evidence-conflict guard or the helper floor (all four were opened on one bundle). It is not the isolation
  veto either (not in `vetoed`). Candidates not tested: the own-package fence, the class step (`down_class`),
  and the fixpoint's seed conflicts. The same question very likely explains the ">= 50% refused" bucket, which is
  10x larger.
- **The emulation is the seed step only.** It does not model support, veto, growth, call-context relaxers or
  tie-breaks. It reproduces 97.5-98.9% of the run's index-seed bytes, so it is a ceiling, not a prediction (the
  real runs show how far apart those are).
- **Claude Code's +0.87 points is one sampled library unit** (19 KB estimated). It is a sample estimate with a
  wide interval, not 0.87 points of the 13 MB bundle.
- bun-held is a private judge: totals only, and nothing on it was diagnosed.

## Files

- `census.py`: the census (reuses `score.py`'s unit, label and judge owners, and `mmlib`'s index table). `table.py`
  prints its JSON as tables.
- `run-variants.sh`: the nine scored labels `exp188-{ctl,c30,c10}-{cc,bun-held,bun-dev}`, run as
  `flock /work/heavy.lock experiments/188-partial-claims/run-variants.sh <commit>` (one lock layer).
- Cards and summaries: `experiments/096-library-matching-score/results/exp188-*` (gitignored, local), raw
  reports under each corpus's data dir.

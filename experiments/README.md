# Experiments

## Read this first: an experiment directory is a LAB NOTEBOOK, not documentation

Every `README.md` under a numbered directory is a **brief** — what someone
believed _before_ doing the work. Every `RESULTS.md` is what the numbers said
_afterwards_. They are kept as written, including the parts that turned out to be
wrong, because the corrections are the most valuable thing in here.

That means:

- **A brief's premise can be false.** exp042's brief fenced off a case as
  effectively a coin flip; exp043 read the actual pairs, found the brief was
  wrong, and shipped that case for −1,807 lines on one hop. A brief's caution is
  a hypothesis, not a constraint — including its caution about what _not_ to try.
- **A number in an older document may have been retracted by a later one.** Five
  published figures in this series were later corrected: alias churn 38% → 7.2%;
  "93% of the 6+ bucket is alias churn" → 98% is not; an 83× metric over-charge →
  1.0×; reorder "~2,800 recoverable, a lower bound" → 1,176; and a damage ceiling
  that was correct within its own scope and still cost +3,742 lines through
  second-order effects.
- **Present tense has expired.** Titles like "the top remaining lever" and "the
  best-shaped one left" were true the day they were written and are not now.

Each README in the active arc below carries a **STATUS block** at the top giving
its outcome, what superseded it, and which of its own claims are refuted. A
directory with no STATUS block has not been audited — check its numbers before
repeating them.

**Before sizing any lever from a document in here, read
[`docs/measurement-pitfalls.md`](../docs/measurement-pitfalls.md)** — eight
rules, seven of them learned by publishing a wrong number first and the eighth
by meeting one of those numbers later.

## The cross-version noise arc (033–045) — the current work

Goal: a humanified release should diff against the prior one showing **only real
source change**. [034](./034-eval-harness/) is the gate every change in this arc
has to pass.

| #                                        | What it was                                                             | Status                                                      |
| ---------------------------------------- | ----------------------------------------------------------------------- | ----------------------------------------------------------- |
| [033](./033-naming-noise/)               | naming-noise harness, Lever B ceiling                                   | landed                                                      |
| [034](./034-eval-harness/)               | **the eval harness and gate** — 4 fixed pairs, KPIs, self-hop invariant | **live tooling, not an experiment**                         |
| [035](./035-naming-state/)               | deterministic naming state, minted census                               | landed                                                      |
| [036](./036-interchangeable-assignment/) | stable names for indistinguishable siblings                             | **concluded — positional assignment is DEAD, do not retry** |
| [037](./037-noise-source-decomposition/) | handoff decomposing the residual                                        | reference                                                   |
| [038](./038-dependency-aware-reorder/)   | dependency-aware emit order                                             | landed — reorder 33% → 4.4%                                 |
| [039](./039-naming-drift/)               | naming drift                                                            | **brief only; its correspondence hypothesis is REFUTED**    |
| [040](./040-diff-census/)                | per-run census of the real git diff                                     | live tooling (`relocation-churn.ts`)                        |
| [041](./041-content-anchor/)             | content anchor + all-same vote                                          | **shipped** — relocation −50.5%                             |
| [042](./042-anchor-preempt/)             | anchor preempts a minted-counter name vote                              | **shipped** — −41.8%; **its own brief's premise was wrong** |
| [043](./043-name-family/)                | preempt on corroborated content                                         | **shipped** — −69.2%, cumulative −91.1%                     |
| [044](./044-naming-correspondence/)      | naming drift, take two                                                  | **two negative results; axis closed**                       |
| [045](./045-reorder/)                    | the reorder axis                                                        | **ceiling measured, no build** — ~80% irreducible           |

### Where this arc ended

All three noise axes are at their measured floors: **~3,700 reducible lines in a
154,668-line diff (~2.4%)**, down from a 21,656-line headline.

| axis       | charged | reducible | the rest is                                                                |
| ---------- | ------: | --------: | -------------------------------------------------------------------------- |
| naming     |   7,616 |    ~2,600 | exp036 rotation — the same names permuted among indistinguishable bindings |
| reorder    |   6,078 |      ~844 | load-order barriers; the constrained share RISES with base size            |
| relocation |   1,390 |      ~294 | solved by 041–043                                                          |

**Closed — do not reopen without new evidence:** the exp044 import-alias
reservation (failed the gate, +3,742 lines), exp039's correspondence hypothesis
(refuted — it is permutation), exp036 interchangeable-bucket assignment,
positional tie-break (+50,606 noiseLn on one hop), and "outer names only"
(regressed the 118→119 canary).

## Earlier experiments (002–032)

The clustering, split and naming-pipeline work that built the current pipeline.

**These predate the [034](./034-eval-harness/) gate and the measurement
discipline in `docs/measurement-pitfalls.md`, so their methodology is weaker than
anything above.** Several were scored on toy library fixtures that the matcher
cannot really be defeated by (exp020 says so explicitly), and none was gated on
four real version pairs with a boot check and a self-hop invariant. **Read them
for the mechanism, not for the magnitude** — a percentage from this range has not
been shown to survive on a real bundle.

Directories: `002`–`004` (clustering), `007`–`012` (reference analysis, ablation,
perturbation lab, minifier sensitivity), `013` (Bun CJS classification),
`014`–`022` (rename noise, megafunctions, naming floor, prior-aware sweep),
`023`–`026` (stable split, LLM split naming, runnable split, CJS emit),
`028`–`032` (operator variance, graph clustering, hash inheritance, ephemeron
cache, prior-match naming).

Roughly a third have no README at all — only a `RESULTS.md` or bare scripts.
That is a gap, not a filing convention.

## Conventions for a new experiment

1. **Measure the ceiling BEFORE building.** Every entry above that skipped this
   wasted a build.
2. **Read the actual pairs before believing the ceiling.** Two sizing predicates
   in exp044 confirmed hypotheses that reading the same data refuted.
3. Research log reads _Idea → Evidence (table) → Conclusion_. Outcomes are
   **landed** or **failed**, with numbers. Totals-first tables.
4. **Judge every hop on its own** — 118→119 is the regression canary, and a big
   hop masks a regression on a small one.
5. Gate with `experiments/041-content-anchor/gate-verdict.sh <control> <label>`.
6. When it is over, **put a STATUS block at the top of the README** saying what
   happened and which of its own claims did not survive. That block is what makes
   the next person's reading cheap instead of misleading.

## Fixtures and shared tooling

`fixtures/`, `fixtures.json`, `ground-truth.ts`, `metrics.ts`,
`prepare-ground-truth.ts`, `prepare-humanified.ts` — the original library-fixture
harness (Preact, zod, hono). Still used by the perturbation lab and the
clustering experiments; the noise arc scores real Claude Code releases through
[034](./034-eval-harness/) instead.

`EXPERIMENT-PLAN.md` and `EXPERIMENT-RESULTS.md` are the original clustering plan
and its reference-clustering results. They describe the Preact/zod/hono workflow
only and say nothing about the current pipeline. Both are **historical**: the
`001-baseline-clustering` directory they cite no longer exists, and the CLI they
document (`split <input> --min-cluster-size N --proximity`) is not the current
one — the pipeline takes `--split`.

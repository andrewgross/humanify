# CLAUDE.md

## Checks

**One command. Run it before every commit:**

```bash
npm run check
```

It runs all seven stages — typecheck, lint (prettier + biome), knip, unit,
explib, fingerprint, e2e — and prints a summary saying which ran. `check:all`
and `test` are aliases of it. Nothing is outside it.

There used to be three commands and none of them ran everything: `test:e2e` sat
outside the documented gate entirely, and `knip` sat outside the one people
actually ran — which is how `check:all` came to be red on main for two findings
nobody had seen. The whole set takes ~25s, so the split was never about speed.

A subset is available for iteration and is **labelled PARTIAL** so it cannot be
mistaken for a green gate:

```bash
npm run check -- --only typecheck,lint
```

Adding a check is one entry in `STAGES` in `scripts/check.ts`. If it is not in
that list it does not run, and that list is the only place to look to find out
what the gate covers.

Individual stages, if you need to run one directly:

```bash
npm run typecheck          # tsc --noEmit
npm run lint               # prettier --check + biome check (src/, test/, scripts/)
npm run test:unit          # all src/**/*.test.ts files
npm run test:explib        # experiments/lib — the shared measurement library
npm run test:fingerprint   # e2e fingerprint snapshot tests
npm run test:e2e           # *.e2etest.ts against a real build
npm run knip               # dead code / unused exports
npm run knip:prod          # production-only dead code audit
```

## Validating cross-version changes

For any change that could affect deobfuscation output (naming, matching,
splitting), the final gate on top of `npm run check` is the eval harness — it
scores the pipeline on a fixed set of version transitions and grades the
cross-version diff as real change vs reducible noise.

```bash
experiments/034-eval-harness/run.sh <label>   # score current tree on 4 pairs (~1hr)
npx tsx experiments/034-eval-harness/leaderboard.ts archive-shipped baseline-main <label>
```

Confirm the **reducible** KPIs (`noise`, `reloc`, `mints`) went **down** and that
`novel` / `realLn` (real code change) did **not** move — a change that "reduces
noise" by dropping real change is a regression. `archive-shipped` (what the git
history shipped) and `baseline-main` (current main) are committed references to
beat. Details: `experiments/034-eval-harness/README.md`.

The eval diffs a freshly-humanified `v` against the prior `v-1`. If a change
alters **formatting** (not just names) so the archive `v-1` is no longer a
like-for-like base — formatting diffs would swamp the signal — regenerate the
prior first: `REBASE_PRIOR=1 experiments/034-eval-harness/run.sh <label>`
re-humanifies each base version with the current pipeline before scoring. That is
expected and fine.

**Before sizing a lever or believing a decomposition, read
`docs/measurement-pitfalls.md`.** Eleven numbered rules. Seven were each learned by
publishing a wrong number first — a sizing predicate that did not test what its
name implied (38% → 7.2%), a mechanism inferred from the largest single example
and refuted by the population (83× → 1.0×), a syntactic proxy biased opposite to
the predicted direction, and a damage ceiling that measured its own scope
correctly and still cost +3,742 lines through second-order effects.

Rule 8 was learned the hardest way: exp033-045 drove `src/` noise to a measured
floor while `vendor/` — same emitted tree, same reviewer, **2.4x the entire
measured `src` noise** — went unscored for thirteen experiments. Every KPI was
honest; the scope was not. **Enumerate what the harness does not look at before
believing a floor.**

Rule 10 is the one that bites the whole gate: **a determinism aid becomes a lie
the moment it is left on for the verdict.** exp047's first gate ran entirely
through `--llm-cache` — all 24,079 entries pre-dated it, not one new entry was
written, so **not one prompt reached the model** across eight runs. Every KPI
agreed with control because both legs replayed the same answers. Re-running cold,
with a cold control, overturned three settled conclusions. Use the cache for
iteration, never for a verdict; `run.sh` now defaults to no cache. And when the
candidate goes cold, the CONTROL must go cold with it.

Rule 11 is what rule 10 uncovers once the variance is free to move: **a gate
cannot resolve an effect smaller than its own noise floor, and it will still print
a confident number and a sign.** exp048's cold A/B credited the CALM canary hop
with the biggest win in the arc (−2,864 lines) and charged another hop with a
regression — **both hops had shipped zero renames.** The `src/` per-hop draw band
is **±2,800**, 8× the ±350 previously assumed, and the change's real effect was
−335 lines. What resolved it: the change LOGGED every rename it applied (an empty
trail cannot have moved a KPI), a mechanism-derived ceiling computed before the
run, and finally pinning the draws — legitimate here only because the pass is
deterministic and sits downstream of every prompt, and only because the pinning
was PROVEN (the second leg wrote zero cache entries). **Measure what your gate
reads for two runs that should agree before letting it decide anything.**

Rule 9 is the one that bites a reader rather than a measurer: **every retraction
lives in a NEWER file than the claim it retracts**, so an experiment README can
still state a number its own RESULTS file corrected. `experiments/README.md`
explains how to read that directory — briefs are hypotheses, titles expire, and
anything before exp034 was not gated on four real version pairs. Each README in
the active arc carries a STATUS block; one without has not been audited.

## Development workflow

We use red/green TDD. When fixing a bug or adding a feature:

1. Write the test first
2. Run the test and watch it fail (red)
3. Implement the solution
4. Run the test and watch it pass (green)
5. Refactor if needed

Never skip the red step. If the test passes before implementation, the test is not testing the right thing.

## Code style

- Actively unify duplicated code. When two systems do similar things, extract shared functionality rather than duplicating with minor variations. Before writing new helpers, check if an existing one can be reused or generalized.
- Biome enforces cognitive complexity <= 15. Extract helpers to keep functions focused.
- Unit tests are colocated as `*.test.ts` next to source files.
- E2E fingerprint tests live in `test/e2e/` as `*.fptest.ts` with snapshots in `test/e2e/snapshots/`.

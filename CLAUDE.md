# CLAUDE.md

## Checks

**One command. Run it before every commit:**

```bash
npm run check
```

It runs all six stages — typecheck, lint (prettier + biome), knip, unit,
fingerprint, e2e — and prints a summary saying which ran. `check:all` and `test`
are aliases of it. Nothing is outside it.

`unit` finds **every** `*.test.ts` in `src/`, `test/` and `experiments/*/lib/`.
It was scoped to `src/` alone until an audit found 64 tests that no script ran:
`test/e2e/functional.test.ts` and six files under
`experiments/029-graph-clustering-split/lib/`.

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
npm run test:unit          # EVERY *.test.ts: src/, test/, experiments/*/lib/
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

**For a change that is supposed to alter NOTHING — a refactor, a counter, a
type-level fix — the eval is the wrong instrument, and
`experiments/lib/neutrality.sh` is the right one:**

```bash
experiments/lib/neutrality.sh <baseline-ref> [from:to]   # ~25min/pair, both legs
```

It runs the candidate and the baseline over one pair with a shared warm cache
and proves BYTE IDENTITY: 0 differing files, 0 differing lines, zero cache
writes on the baseline leg, matching exit codes. Rule 11 is why — the eval
cannot resolve an effect of zero and will print a confident number and a sign
anyway, so "the KPIs moved a bit" tells you nothing about a refactor. A byte
diff answers the question exactly, in a third of the time.

Three things it will tell you that are easy to misread:

- a MISMATCHED EXIT CODE fails the verdict on its own, which is correct — but
  if the change was _meant_ to fix a failing run, that mismatch is the point.
  Check `differing lines: 0` before reading "NOT NEUTRAL" as a regression.
- a leg that exits non-zero having WRITTEN a tree is recorded and compared, not
  treated as a crash. The pipeline exits 1 on a rename-invariant failure.
- the baseline leg runs in a detached git worktree, so **`src/` must not be
  edited while a candidate leg is running**.

Using the cache here is the use rule 10 permits: it forbids the cache for a
verdict about LLM-dependent behaviour, and this is a verdict about determinism
with the model held fixed.

**Only the BASELINE leg's zero is load-bearing** — this used to read "verify
BOTH legs wrote zero", which the script itself contradicts and which would
reject most valid runs. The candidate leg runs first and POPULATES the shared
cache, so its count is routinely large (7, 1591 and 3525 in one session, all
valid). What the baseline's zero proves is that leg B asked nothing leg A had
not: the two legs put the SAME questions to the model. The verdict logic fails
on that count alone. Read it before the summary line.

**A NOT NEUTRAL verdict can be noise. Re-run before believing it.** A rare
divergence — identical or inert code producing different bytes — has been
observed on BOTH eval pairs tested:

- `2.1.118:2.1.119` — a null control (identical `src/` both legs) diverged by
  15 files / 212 lines. 33 further controls were clean.
- `2.1.85:2.1.86` — diverged by 2 files / 8 lines on a change later PROVEN
  inert (the same comparison re-run came back byte-identical, and a null
  control on the pair was clean).

This file previously named 85->86 as immune and told you to gate on it. That was
wrong, and wrong in an instructive way: it was inferred from a run of clean
results, which is what a ~3% event produces almost every time. Absence of a rare
failure is not immunity.

What to do with a NOT NEUTRAL:

1. **Re-run the identical comparison.** A real regression reproduces; this
   does not. That single step separated the two cases above.
2. **Check whether your change can even reach the difference.** The exp059
   ledger was exonerated on 118->119 because its own counter read ZERO there
   while neutrality reported 420 differing lines.
3. A NEUTRAL pass remains STRONG evidence — it passed despite the noise floor.
   The asymmetry is the point: this bug can only manufacture a false FAILURE,
   never a false pass, so no merge gated on a NEUTRAL result is in doubt.

Both legs writing zero cache entries in the diverging runs proves the prompts
were identical, so the cause is downstream of the model. Tracked as its own
task; the observed diffs are a placement artifact in one case and a
decoration-shaped NAME in the other, so a post-naming pass is the current
suspect.

Before scoring the four pairs it runs `experiments/lib/matcher-preflight.sh`
(~5s, no LLM): the fingerprint matcher against real npm packages. It asserts the
expected OUTCOME SET rather than a threshold, because zustand's
`getState`/`getInitialState` are identical `() => variable` shapes that stay
ambiguous by design — a permanently-red check is one nobody reads. A fixture
moving between the pass list and the known-shortfall list is the signal.
`MATCHER_PREFLIGHT=0` skips it and says so.

`EVAL_HEAP` (default 65536 MB) sizes the pipeline's heap. The old 14336 was
sized for cached runs; cold-by-default keeps far more naming state live and
2.1.215→216 OOMs at 14 GB.

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
- **[`docs/responsibility.md`](./docs/responsibility.md) says who owns which question** — name legality, applying a rename, counting changed lines, walking a tree, reading a ledger, reading a kill switch. Check it before writing a helper, and add a row when you create an owner. The dangerous duplication is not two functions that look alike; it is two that answer the same question DIFFERENTLY with nothing declaring the difference (a guard that read fields one producer never sets was dead for 11,094 accepts and reported as perfect precision).
- **[`docs/pipeline-stages.md`](./docs/pipeline-stages.md) lists the twelve stages that actually run**, in order, and which three have a real strategy registry. Read it before assuming a plug point exists: the working mental model was four stages, and the eight unwritten ones are where the noise has come from — `vendor/` went unscored for thirteen experiments at 2.4x the measured `src/` noise. It also records that `--split-strategy` is registered on the standalone `split` command and is never passed on the unified path.
- Biome enforces cognitive complexity <= 15. Extract helpers to keep functions focused.
- Unit tests are colocated as `*.test.ts` next to source files.
- E2E fingerprint tests live in `test/e2e/` as `*.fptest.ts` with snapshots in `test/e2e/snapshots/`.

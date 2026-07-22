# CLAUDE.md

## Checks

Run before every commit:

```bash
npm run check        # typecheck + lint (prettier + biome) + unit tests + fingerprint tests
npm run check:all    # above + knip (dead code detection)
```

Individual commands:

```bash
npm run typecheck          # tsc --noEmit
npm run lint               # prettier --check + biome check
npm run test:unit          # all *.test.ts files
npm run test:fingerprint   # e2e fingerprint snapshot tests
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

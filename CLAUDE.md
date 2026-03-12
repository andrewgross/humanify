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

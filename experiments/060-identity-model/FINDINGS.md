# 060 — FINDINGS: what three independent reviews said, and what was verified

> ## STATUS: **Findings recorded. Every claim below marked VERIFIED was re-checked against the code by hand; everything else is a PROPOSAL and is labelled as one. No redesign has been built.**
>
> Supersedes [`README.md`](./README.md) wherever they disagree — the brief was
> written first and got two things wrong.

Three fresh-context reviews were run against the brief on 2026-08-04, each given
a different angle and no knowledge of the others: **identity-first**,
**cross-release-stability-first**, and **reduction-only** (explicitly told not to
propose an architecture).

## 1. What they converged on, unprompted

All three arrived at the same diagnosis from different starting points:

> The system has no single answer to "what is this thing" or "what is it
> called". Identity is carried three ways — live Babel objects, name strings,
> and source positions — and none survives what is asked of it.

Two independently proposed the same key, and both noticed the codebase **has
already discovered it twice, in isolation**:

| site                     | key                             | arrived at via                         |
| ------------------------ | ------------------------------- | -------------------------------------- |
| `structural-hash.ts:628` | `slotByDeclId` — declaration id | making hashing survive the cache clear |
| `validated-rename.ts`    | `renameClaims` — block node     | exp059's capture                       |

Since those reviews ran, **two more independent arrivals** at the same key were
found and fixed: `strategy-trail.ts` (entries keyed by `Binding` object could not
count a clobber across a scope epoch) and `RenameReport.outcomes` (keyed by old
name, loses 26,270 renames — task filed, not yet fixed).

**Four separate bugs, four separate rediscoveries of "key by the declaration
node".** That is the finding with the most evidence behind it, and none of it
was designed — it was paid for.

Both design reviews also proposed **decide-then-apply**: renames accumulate as
decisions and are written to the AST once, at the end. Both noted the codebase
already proves the mechanism twice — `rename-ledger.ts` reproduces
`generate(ast)` exactly via text splices and self-verifies in-pipeline, and
`wave-scheduler.ts` already defers renames to a barrier so prompts stop
observing completion timing.

## 2. Where the brief was WRONG — verified against Babel's source

**VERIFIED.** The brief said the cache clear "rebuilds every Scope/Binding/
NodePath for the current AST too". It rebuilds nothing:
`clear()` is `clearPath()` + `clearScope()`, each assigning a **new empty
WeakMap** (`@babel/traverse/lib/cache.js:14-22`). Retained objects keep working.
**The hazard is that nothing is rebuilt**, so a retained handle silently becomes
a second tree.

**VERIFIED.** A second scope tree can open with **no clear at all**. `Scope`'s
constructor returns the cached scope only when `cached.path === path`
(`scope/index.js:320-323`), so any fresh `NodePath` over an already-scoped node
mints a fresh `Scope`. The blast radius is _any path-cache eviction_, not the two
`clearBabelTraverseCache()` calls.

**VERIFIED.** The split is **intra-function**, not phase-level. One
`collectOwnedBindingInfos(fnPath)` returns `BindingInfo`s whose `.scope` fields
straddle both epochs, because one helper reads the retained `fnPath.scope` while
two others mint fresh paths.

Consequence for exp059: the fix is unaffected (it keys on the block node, which
is shared however the second tree arose) but the _published cause_ was narrower
than the truth. Corrected in exp059's RESULTS.

## 3. The reduction review's two headline deletions were BOTH unsafe

Recorded at length because each would have caused real damage, and each was
caught only by checking the code rather than trusting the report.

**"A second rename scheduler, ~1,100 lines, reachable only via
`--no-wave-scheduling`."** — WRONG. `ProcessorOptions.waveScheduling` is
optional and `processor.ts:737` branches on it directly; the `?? true` lived
only in `resolveSettings`, i.e. the CLI path. All six `createRenamePlugin` calls
in `src/test/rename.e2etest.ts` omit it, so **the e2e suite was running that
loop**. Deleting it would have deleted code the gate was exercising.
→ Fixed instead by defaulting it properly; the flip surfaced a real reporting
divergence (task on `renamedCount`).

**"A second split implementation, ~5,005 lines, reachable only via a fallback
the target bundles never take."** — reachability true for our fixtures, deletion
dangerous. 326 pipeline stdouts from one session took the stable path 326 times
and the fallback 0 times. But `stableSplitFromCode`'s contract
(`stable-split.ts:1486-1488`) says it _"returns null when the code is NOT a
single wrapper IIFE (the caller falls back to the legacy splitter)"_, and its
null returns are shape checks. **The fallback is the supported path for every
bundle that is not a single wrapper IIFE.** All 326 runs were claude-code (Bun,
single-IIFE) — rule 8, a metric can only be wrong about what it looks at.
Deleting it would break most inputs to a public tool.

**The transferable lesson: "no production caller in THIS repo's runs" is not
"dead".** Two of two headline deletions were unsafe, for opposite reasons — one
live in the tests, one live for users we do not fixture.

## 4. What the reduction review got RIGHT, and what shipped

All verified before acting, all merged:

- `VALID_STRATEGIES` omitted `bun-cjs` while the help text advertised it, and
  `SplitStrategyType` declared `webpack` with no adapter — a **silent** no-op
  override. Both lists now derive from the registry; unknown overrides throw.
- `activeKillSwitches()` had zero production callers while the run manifest got
  its answer from a `HUMANIFY_`-prefix match — over-reporting a token budget, a
  deliberately-excluded probe, and a switch read only by the emitted tree. One
  owner now; the env-read guard was widened to `experiments/lib/` and proven to
  catch the original defect by planting it back.
- `listJsFilesRecursive` not skipping `.humanify/` (ranked #3 in
  `docs/responsibility.md`) — `env-reads` on a split tree counted every read
  twice. All three tree walkers now agree.

## 5. What is NOT decided

No redesign has been built and none is committed to. Both design reviews
proposed the same first increment, which is the right instinct: **measure,
change nothing.** Build the index (or the decision record) alongside the current
system, assert it agrees, ship no behaviour change.

One of them makes the ceiling computable before any code changes — _on a real
pair, what fraction of release N's units are present in N-1's record?_ That
number bounds the entire lever and costs one measurement. It has not been run.

Known-unfixed, filed separately: `RenameReport.outcomes` keyed by old name
(26,270 renames invisible on one bundle); the cross-release record split across
two artifacts (top-level names to the ledger, inner names to the bundle, neither
holding both); and a null-control divergence in `neutrality.sh` on one pair that
is real, rare (1 in 28 as of this writing) and still unexplained.

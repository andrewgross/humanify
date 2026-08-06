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
"dead".** THREE of three deletion recommendations from that review were wrong as
stated, each partly right about the facts and wrong about the action:

- the "dead" scheduler was live in the TEST SUITE;
- the "dead" split fallback is live for every USER whose bundle is not a single
  wrapper IIFE — a population we do not fixture;
- `retryBatchWindowMs`, "dead twice over", is a deliberate TEST SEAM. It is set
  to 30ms so a retry-merging test does not ride on wall-clock luck, and
  `DEFAULT_WINDOW_MS` is 25 — deleting it would have tightened that window 17%
  to save four lines. Only its PLUGIN-level half was genuinely unreachable, and
  only that half was removed.

An audit can establish "nothing in this repo calls X". It cannot establish that
deleting X is safe, because the reason X exists is frequently not a caller.

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
pair, what fraction of release N's units are present in N-1's record?_

## Increment 0: the ceiling, MEASURED on all four pairs (2026-08-06)

Both design reviews proposed computing, before building anything, what fraction
of release N's units are present in N-1's record. A content-keyed record can
carry a name only where the new key MATCHES one already recorded — the
exact-hash tier. Everything else the cascade resolves is work a record cannot
do, because those tiers exist precisely for units whose content CHANGED.

Read from `resolutionStats` on a cold 4-pair eval (`ceiling-4pair`,
`REBASE_PRIOR=1`, all four `exit 0`, all four `cache +0`):

| pair    | exact     | cascade   | `enclosingStatement` | units       |
| ------- | --------- | --------- | -------------------- | ----------- |
| 2.1.86  | 57.4%     | 94.7%     | 21.5%                | 35,663      |
| 2.1.119 | 59.7%     | 96.5%     | 20.9%                | 40,496      |
| 2.1.198 | 58.5%     | 95.6%     | 20.8%                | 57,419      |
| 2.1.216 | 59.6%     | 97.3%     | 20.5%                | 63,723      |
| **all** | **58.9%** | **96.2%** | —                    | **197,301** |

**THE CEILING IS ~59%, NOT ~96%.** The cascade resolves 96.2% of units; a
record replaces only the exact tier, **58.9%**. The other ~37% is resolved by
tiers that read structure and context.

### A prediction of mine that FAILED, and why that strengthens the result

The single-pair figure (59.7%, 118→119) was published with the caveat that it
was "likely OPTIMISTIC", reasoning that a small release delta keeps more units
byte-identical and that heavier hops would push them out of the exact tier.

**That was wrong.** The spread across four pairs is 2.3 points, the weighted
figure (58.9%) is within one point of the one-pair number, and the pair with the
FEWEST units has the LOWEST exact share — the opposite of the predicted
direction. Exact-match share does not track delta size here.

A number that varied by pair would be a sample. A number sitting in a 2.3-point
band across four independent pairs and 197k units is a **property of the
pipeline and this codebase's shape**. Size against ~59%, not a range.

### Corroboration

`enclosingStatement` lands at 20.5–21.5% on all four pairs against exp053's
independently measured **21.1%** — five measurements, two different instruments,
one point of total spread. That is the strongest evidence this decomposition is
real rather than an artifact of how the counters were read.

### What it means for the redesign

- "The record replaces the MAJORITY tier" is CORRECT and must not be read as
  "replaces the cascade". The cascade stays, in full, for ~37% of units.
- The payoff is therefore **not** fewer matching tiers, and **not** better
  matching — a record cannot improve matching, only stop recomputing what was
  already certain. It is not RE-DERIVING the 59%, which is what costs the prior
  parse, the dual-AST memory window, and the `clearBabelCacheAfterPriorMatch`
  that made exp059's two scope eras possible.
- Price the work against that. If re-derivation is cheap, the redesign is
  bookkeeping; if the prior parse and its memory window are the constraint —
  peak RSS on these runs was 14.9–26.5 GB — it is not.

Known-unfixed, filed separately: `RenameReport.outcomes` keyed by old name
(26,270 renames invisible on one bundle); the cross-release record split across
two artifacts (top-level names to the ledger, inner names to the bundle, neither
holding both); and a null-control divergence in `neutrality.sh` on one pair that
is real, rare (1 in 28 as of this writing) and still unexplained.

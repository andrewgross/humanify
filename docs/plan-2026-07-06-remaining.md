# Plan — remaining work after the 2026-07-06 execution (handoff)

Written 2026-07-06 by the agent that executed docs/plan-2026-07-06.md
(commits cc3888b..0c46d0e on main — all three workstreams landed,
measured, pushed). This file is the handoff for whoever continues:
every item is self-contained (evidence, design decision + rationale,
code anchors, TDD test plan, measurement). **Before starting an item,
check `git log --oneline cc6faa0..` and the checkboxes below — the
original agent may have completed more after writing this.**

Ground rules (CLAUDE.md + memory): red/green TDD — watch the test fail
first; precision over recall (wrong match/name is worse than missed);
no backwards compatibility — delete old code, write fresh tests;
`npm run check` before every commit; commits on main are fine; never
use C1/D4-style review shorthand in user-facing text.

Status legend: [ ] open, [x] done (update as you go).

---

## Item 1 — [x] DONE (commit 200baed) Hash instability root cause: shorthand serialization

**Evidence (executed 2026-07-06):** `{ u, kind: 1 }` and
`{ u: userId, kind: 1 }` produce DIFFERENT structural hashes; same for
destructuring `({ u })` vs `({ u: userId })`. Renaming a shorthand
binding forces Babel to expand shorthand → longhand (the KEY must keep
its external name), so any function containing a renamed shorthand
property/pattern changes hash after humanify+regenerate. This is (most
of) the known 0.4% instability: 158 hash-absent functions in the
cross-leg residual, which in turn starves propagation (leg stats:
structuralHashUnique −172, propagationResolved −181 on the humanified
leg vs the beautified control; shingleSimilarityResolved is IDENTICAL —
the earlier "rename-variant shingles" attribution in
WS3-MATCHER-HARDENING-RESULTS.md was wrong, see item 2).

**Fix (one field):** add `"shorthand"` to `SERIALIZE_SKIP_KEYS` in
`src/analysis/structural-hash.ts` (~line 453). Sound because the key
and value are serialized independently — `{u}` ≡ `{u: u}` carries zero
extra information in the flag. Slot ordering (binding-keyed
placeholders) is unaffected: identifier occurrence ORDER doesn't change,
only the one boolean field is dropped. No persisted hashes exist —
matching recomputes both sides per run.

**Tests (red first), in structural-hash.test.ts:**

- `{ u, kind: 1 }` fn hashes equal to `{ u: userId, kind: 1 }` fn
  (rename-equivalence).
- `({ u }) => u` equals `({ u: userId }) => userId`.
- Distinguishability guard: `{u: a}` vs `{u: b}` where a,b are
  DIFFERENT bindings both referenced elsewhere (two slots) must still
  hash differently from a version where the value binding coincides
  with another slot — i.e. assert `{u, w}` fn ≠ `{u: w, w: u}` fn
  (crossed values must differ).
- Placeholder-mapping invariance: buildPlaceholderMapping over the
  shorthand and longhand variants yields the same slot→name pairs
  modulo names.

**Measurement:**

1. `npm run test:unit` + `npm run test:fingerprint` — expect SNAPSHOT
   SHIFTS in fingerprint e2e wherever fixtures contain shorthand;
   review each: matches should only increase/stay. Update snapshots
   only after eyeballing (`--update-snapshot`).
2. Perturbation lab: `npx tsx experiments/011-perturbation-lab/run.ts
--name shorthand-fix` — expect zero row-level regressions vs
   results/ws3-check.json.
3. Matching-only leg (~5 min): from repo root,
   `node --max-old-space-size=16384 --import tsx/esm
experiments/013-bun-cjs-classification/measure-close-match-anomaly.ts
run --prior humanified --out /tmp/exp013-anomaly/leg-humanified-shorthand.json`
   then `... compare /tmp/exp013-anomaly/leg-beautified-ws3.json
/tmp/exp013-anomaly/leg-humanified-shorthand.json`.
   NOTE the beautified control leg was built pre-fix; hashes change on
   BOTH sides of that compare, so regenerate the control too
   (`run --prior beautified --out .../leg-beautified-shorthand.json`)
   — one leg per process, they mutate their inputs. Expect: "hash
   ABSENT from B prior" to collapse from 158 toward ~0 and the total
   354 residual to shrink by roughly both halves (absent + the
   propagation knock-on). Record numbers in
   experiments/013-bun-cjs-classification/WS3-MATCHER-HARDENING-RESULTS.md
   (append a "shorthand fix" section and CORRECT the shingle
   misattribution paragraph).

**Pitfall:** `computed` must NOT be skipped (x[a] vs x.a is semantic).
Only `shorthand`.

## Item 2 — [x] MEASURED (see WS3-MATCHER-HARDENING-RESULTS.md): residual 354 → 84; remaining 19 root-caused to duplicate humanified names flipping binding resolution (inspect-hash-divergence.ts is the tool); shingle enrichment NOT needed

The 196 "hash present, cascade failed" residual was largely downstream
of item 1 (weak seed network → propagation starved). After item 1's
legs, re-read the compare output:

- If the residual is now small (<50), stop; note numbers and move on.
- If a meaningful chunk remains, the remaining lever is shingle
  ENRICHMENT, not invariance: `computeShingleSet`
  (src/analysis/function-fingerprint.ts:283) is already
  rename-invariant (blurred edge n-grams + externalCalls +
  propertyAccesses + EXACT stringLiterals). For same-hash buckets its
  discriminating power is only callee shapes + exact literals. Add
  exact-literal _sequence_ n-grams from the preserveLiterals
  serialization: `hashPathWithMapping` exists; add a
  `serializePathTokens(path, {preserveLiterals: true})` export next to
  it in structural-hash.ts returning the parts array, and shingle
  4-gram windows of it. Keep SHINGLE_THRESHOLD 0.5 + strictly-better-
  than-runner-up. Risk to watch: exact literals differ on TRUE matches
  whose literals changed (version strings) — that only lowers their
  sim; the runner-up gate prevents a flip unless a sibling coincides.
  Measure with the same two-leg A/B + perturbation lab.

## Item 3 — [x] DONE (commit 1c83a83) Close-match signature transfer corroboration (review C6's surviving half)

**Problem:** `computePartialTransfer` (src/cache/prior-version.ts)
renames function name + params for ANY cosine-0.8 close pair, then
excludes them from LLM re-rename (`priorVersionTransferred`). A
deleted-helper + added-helper pair with similar count-features gets a
permanently wrong name presented as continuity. Body locals (landed
2026-07-06, src/cache/statement-align.ts) are declaration-anchored and
safe; the SIGNATURE transfer has no content corroboration at all.

**Design:** gate the signature transfer on statement-alignment
evidence, which is already computed for the pair:

- In buildCloseMatchContext, compute body transfers FIRST (they carry
  an implicit aligned-statement count — expose it: make
  computeBodyLocalTransfers return `{ transfers, alignedStatements,
totalNewStatements }` or add a parallel exported
  `countAlignedStatements(priorFn, newFn)`; prefer returning a small
  struct, callers destructure).
- Apply computePartialTransfer's name+param pairs ONLY when
  alignedStatements ≥ 1. Zero aligned statements = the bodies share no
  identical normalized statement = the pair is a shape coincidence →
  transfer NOTHING; still set `priorVersionContext`/`priorVersionNames`
  (LLM hints — the review's "downgrade to suggestion" behavior).
- Tiny-function escape hatch: single-statement bodies whose one
  statement aligns pass trivially (alignedStatements=1). A
  single-statement body whose statement does NOT align (the change IS
  the whole body) transfers nothing — correct: nothing is corroborated.
- Add `closeMatchRejected` (or `signatureTransfersGated`) counter to
  the close-match TransferStats path for observability.

**Tests (red first), prior-version.test.ts:**

- Unrelated same-shape pair (e.g. `function a(x){ return x.foo(1); }`
  vs `function b(y){ return y.bar(2); }` padded so cosine ≥ 0.8 — they
  must first BE a close pair; check with findCloseMatches directly if
  needed): nameTransfers must be EMPTY (today it transfers name+params
  → red).
- True close pair (statement inserted — reuse the existing fixtures):
  signature still transfers (≥1 aligned statement) — guards recall.
- Plugin-level (plugin-cross-version.test.ts): zero-alignment pair's
  function keeps LLM naming but the request carries
  priorVersionCode/priorVersionNames.

**Measurement:** perturbation lab (addConsoleLog rows exercise close
pairs); the matching-only harness doesn't measure transfers — rely on
unit + the hermetic diff-noise test (must stay green), and the next
full-bundle run's `closeMatch` TransferStats.

## Item 4 — [x] DONE (commit d0db823) Small-bug batch (each S-sized, TDD each, one commit each or grouped)

**4a. eval/`with` soundness guard (review C8).**
No detection exists (verified 2026-07-06). Renaming any binding visible
at a `with(obj){...}` block or a DIRECT `eval(...)` call site is
unsound (runtime name resolution). Design: during
`buildUnifiedGraph` (src/analysis/function-graph.ts), collect taint
sites: `WithStatement` paths, and `CallExpression` whose callee is
Identifier `eval` that does NOT resolve to a local binding
(`!path.scope.getBinding("eval")` — a resolved local eval is indirect
and safe). For each site, walk UP the scope chain: every enclosing
FUNCTION node (and the module scope if reached) is "taint-enclosing" —
its OWN bindings are visible at the site and must not be renamed. Mark
those FunctionNodes `status: "done"` + `renameMapping {names:{}}`
pre-processing (same shape as library skip) and exclude their
module-level siblings ONLY if the site is at module scope — module-
scope taint means renaming nothing in the file: emit a loud
console/debug warning "with/eval at module scope — renaming disabled
for this file" and mark ALL nodes done. Also gate the TRANSFER paths:
applyPriorVersionIfPresent runs before the processor — simplest sound
hook is to compute the taint set during graph build, store on the graph
(`graph.taintedFunctionIds: Set<string>` or mark nodes), and have
applyMatchedRenames/attachCloseMatchContext skip tainted functions.
Tests: with-block fixture (Vue2-style `with(this){ return _c(...) }`),
direct-eval fixture, LOCAL eval binding fixture (must still rename),
module-scope eval fixture (nothing renamed, warning emitted).

**4b. scorePairs bounded top-K (crash fix).**
src/analysis/close-match.ts:137 pushes every pair ≥ threshold —
O(old×new) memory. Keep per-OLD top-K (K=3) via insertion into a
3-slot array, then assignGreedy as today (it sorts globally — feed it
the union of per-old top-3; greedy result is unchanged for any pair
that would have won, because a winning pair is by definition its old
side's best surviving candidate... note: not literally identical to
today's output when an old's 4th-best would have been assigned after
its top-3 were stolen — accept the difference, it only DROPS weaker
assignments; precision-first). Test: synthetic 500×500 identical-vector
sets must not materialize 250K candidates (assert candidates.length ≤
500\*K via an exposed seam or just a memory/behavior test on the
result); plus behavior parity test on a small real case.

**4c. `extractVarNameRename` block-scope miss (review C9, recall).**
src/cache/prior-version.ts (~line 505 area): uses
`declaratorPath.scope`, which for `var` inside a block is the BLOCK
scope that doesn't own the binding → attemptValidatedRename returns
no-binding → silent missed transfer. Fix: resolve the owning scope via
`declaratorPath.scope.getBinding(name)?.scope` and pass THAT. Test: a
matched function-expression pair assigned inside an if-block with
`var` (`if (c) { var h = function(){...}; }`) — var-name transfer must
apply.

**4d. Name-keyed dedupe in binding collectors (review C9, recall).**
Two same-named bindings in sibling blocks: only the first is collected
(plugin.ts `collectBlockScopeBindings` dedupes by name via `seen`;
processor's collector has the same shape). The name-keyed transfer
Record genuinely can't express both; but the LLM path can rename the
second via the existing shadowed-bindings second pass
(processor.processFunction → collectShadowedBlockBindings). Verify
whether sibling-block (not shadowing) duplicates reach that second
pass; if not, extend its predicate to "same name, different binding
identity, not yet renamed". Keep the transfer path first-wins (document
it). Test: `function f(){ { let e = 1; use(e); } { let e = 2; use2(e); } }`
— both `e`s renamed (to different names) by the LLM path.

**4e. Swallowed catch (review I7 residue).**
src/rename/processor.ts ~line 842: `catch { this._skipReasons.error++ }`
around per-batch task work swallows programming errors as a skip stat.
Change: keep containment for provider throws (they're already contained
at the dispatch layer), but at THIS site rethrow
non-provider errors OR (simpler, chosen design) count into a new
`processor.internalErrors`, include in coverage summary output, and
have the CLI exit non-zero when internalErrors > 0 (same pattern as
parse/semantic failures in src/commands/unified.ts). Test: a mock
whose applyRename throws TypeError → run completes, coverage reports
internal error, promise of non-zero exit at CLI layer unit-tested via
the exit-code plumbing (see how parseFailures set process.exitCode).

**4f. Hygiene (no test needed beyond check/knip).**

- Delete `hashCalleeShapes` (src/analysis/function-fingerprint.ts:243)
  and `validateOutputParses` (src/output-validation.ts:162) — knip:prod
  flags them; verify no harness usage first (knip entries cover
  experiments).
- Plumb `retryBatchWindowMs` through RenamePluginOptions →
  processUnified options (WS2 agent left default-only).
- Memory dir: MEMORY.md links `project_cross_version_diff_gaps.md`
  which does not exist — either delete the line or write the file from
  the one surviving fact ("operator normalization is biggest win").

## Item 5 — [ ] OPEN — Structural refactor spec (execute AFTER items 1–4; sized M–L)

Order matters; each step keeps `npm run check` green.

**5.1 Collector unification (review D7).** One module
`src/rename/function-bindings.ts` exporting the single traversal with
two entry points: `collectOwnedBindingInfos(fn)` (LLM path: EXCLUDES
nested FunctionDeclaration names — they self-name, see commit 97f836b)
and `buildOwnedBindingMap(fn)` (transfer path: INCLUDES nested fn decl
names — exact-match pairs legitimately carry them; note the child's own
transfer then no-binding-skips, which is fine). The two callers:
processor.ts `getOwnBindings` (+ its four collect\* helpers ~line 2319)
and plugin.ts `buildFunctionBindingMap` (~line 473 + helpers). Port the
named-function-expression self-name handling (processor-only today).
Then extract the ~700-line prior-version transfer block out of
plugin.ts into `src/rename/prior-transfer.ts` (applyPriorVersionIfPresent
through suggestFromCloseMatchExternals), and `git mv src/cache
src/prior-version` (update imports; knip/tsconfig unaffected). This
also breaks the plugin↔processor circular import
(`getProximateUsedNames` moves to its own module or into
function-bindings.ts).

**5.2 Babel shim deletion (review I3).** Delete `ScopeLike`/
`BindingLike` (src/rename/validated-rename.ts:40-59) and `BabelBinding`
(src/analysis/function-graph.ts:5); type against `Scope`/`Binding`
from `@babel/traverse`. Every `?.()`-guarded call becomes a plain call
— the guards SILENTLY PASS safety checks when handed a shim, which is
why this matters (the capture check `scope.parent?.hasBinding?.()` is
in the validator itself). Tests that hand-roll scope objects must build
real ones via parseSync (most already do; the validated-rename tests
all do). Expect ~27 `?.(` deletions; grep for `?.(`.

**5.3 Config resolved once (review I6).** One `RunConfig` object
(`isEligible`, `profiler`, `usedNames`, bundler/minifier types)
resolved at createRenamePlugin entry; everything below takes it as a
REQUIRED param. Delete the seven argument-less `createIsEligible()`
re-defaults (grep `createIsEligible()`), the nine `?? NULL_PROFILER`
sites, and the optional-usedNames plumbing.

**5.4 SessionId grammar cleanup (review I5).** Matcher returns should
carry node identity, not parseable strings: add
`position: {line, column} | null` to FunctionNode/ModuleBindingNode at
graph build; delete propagation.ts `extractPosition` (currently NaN
for `module:` ids — latent only because scope-ordinal runs on
functions); sort scope-children by the stored position. Make binding
fingerprint `FunctionFingerprint | null` instead of the `binding:`
name-derived fallback hash, and partition matchable/unmatchable at
graph build (two consumers currently re-filter by prefix).

**5.5 Lifecycle state machine (review I4 — LAST, biggest).** Replace
the four "done" encodings (FunctionNode.status / processUnified's
doneIds / renameMapping-presence semantics / preDone list) with one
explicit state on the node:
`{ kind: "pending" } | { kind: "transferred", names } |
{ kind: "llm-done", names } | { kind: "skipped", reason } |
{ kind: "failed", error }` — `renameMapping` today means four things by
context (pending transfer / `{names:{}}` nothing-to-rename sentinel /
LLM output / close-match claim guard in attachCloseMatchContext); each
gets its own state or field. processUnified derives doneIds from
`kind !== "pending"` (the status-derived done change from commit
dee5862 already points this way). Assert legal transitions in one
place (`transition(node, from, to)`). Then delete `preDone` (metrics
take a count, not a list). Migration order: introduce the state field
alongside status → port readers → port writers → delete status/
sentinels. The hermetic diff-noise test and plugin-cross-version suite
are the safety net; run the full check between each port step.

## Full-bundle validation run — [x] DONE (see PHASE4-RESULTS.md)

Ran 2026-07-06 pinned at 1c83a83: incremental leg 2h59m → **8m56s**,
fresh leg 1h55m → 23m37s, exact transfers up, zero correctness
failures. Rename-noise +6.2%, attributed to the DESIGNED binding-
transfer precision trade (two-vote floor + phantom gate; some removed
transfers were consistent lies the metric rewarded). New follow-ups,
in value order: (1) recover safe single-vote binding transfers
(exact-matched voter + prior-unique name, or downgrade to
suggestedName) — ~980 bindings is the biggest noise block; (2)
attribute noise hunks per population via the diag JSONs; (3) re-run at
a commit ≥ d0db823; (4) cosmetic queue-state done/total mismatch
(pre-done counted in done, not in total).

### Original instructions (for re-runs)

From a worktree pinned at the commit containing items 1 (at minimum):

```bash
git worktree add /tmp/humanify-run-ws4 <commit>
ln -s /Users/andrewgross/Development/humanify/node_modules /tmp/humanify-run-ws4/node_modules
cd /tmp/humanify-run-ws4
PHASE2_OUT=/tmp/exp013-phase4 bash experiments/013-bun-cjs-classification/run-phase2.sh
```

Needs the LLM box (http://192.168.1.234:8000/v1, gpt-oss-20b,
HUMANIFY*API_KEY=local — the script defaults these, plus
`--reasoning-effort low` as of commit 0c46d0e). Compare against
PHASE3-RESULTS.md baselines: diff 131,437 lines / 22,983 hunks;
rename-noise 20,181 hunks / 65,170 lines (classifier: equal-count
change hunks identical after `\b[A-Za-z*$][A-Za-z0-9_$]\*\b`→`#`);
exact 33,912 / close 7,576 / fresh 1,148; binding cache 89.7%; fresh
leg ~2h, incremental ~3h (retry tail ~2h of it). Expectations: wall
clock down hard (reasoning effort + retry batching + fewer
double-named identifiers), rename-noise hunks down (close-match body
transfer + prompt adherence + name-stability fix from 97f836b), exact
count UP if item 1 landed (shorthand). Zero validation failures
required; `singletonRejected`/`injectivityDemoted` appear in the
matcher stats — nonzero is expected and fine.

## Deliberately not now (unchanged from the 2026-07-06 plan)

Changeset renderer / `humanify diff` (user decision: skip); dry-run
mode + pipeline stages; operator normalization (from the lost
cross-version-diff-gaps note — biggest remaining naming-consistency
win, but belongs with a future diff-quality pass).

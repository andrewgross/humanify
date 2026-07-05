# Code review — correctness & organization (2026-07-04)

Branch `fix/transfer-validation`. Three independent review passes (rename
correctness, duplication/dead code, invariants/fail-fast), consolidated
and deduplicated. **Findings only — no fixes applied yet** (an LLM
validation run was in flight). Items marked ⚡ were confirmed by
executing the repo's own helpers against crafted inputs, not just by
reading.

Effort: S = hours, M = a day-ish, L = multi-day.

---

## P0 — Correctness: silent runtime breakage or wrong names at scale

### C1. Free-identifier capture: renames can shadow names the bundle uses as globals ⚡ (S)

`GLOBAL_BUILTINS` (src/llm/validation.ts:76) covers ES/node builtins but
NOT browser globals: `window`, `document`, `self`, `location`, `$`,
`jQuery`, `Bun`, `define` are absent. Worse, `getUsedIdentifiers`
(context-builder.ts:141) reads `fnPath.scope.globals`, which Babel only
populates on the **Program** scope — it is always empty for function
scopes, so the file's own free names never enter the used set. The
transfer path's `hasBinding` check has the same blindness.

**Executed:** `var d = 1; console.log(document.title, d)` + rename
`d → document` was APPLIED → `document.title` reads a number at runtime.
Output parses cleanly; the parse gate cannot catch it.

**Fix:** add `globals.browser`/`worker` to `GLOBAL_BUILTINS`; collect the
Program scope's observed free names once per run and reject any rename
target in that set (both `getRenameRejection` and the LLM-path
validators). Invariant: _a rename may never bind a previously-free name._

### C2. `fastRenameBinding` misses duplicate-declaration writes ⚡ (S)

`getConstantViolationLHS` (validated-rename.ts) handles assignment and
for-in/of LHS but not the constant violation Babel records for a
**duplicate declaration** (`var a = 1; ... var a = 2;`, duplicate
`function a(){}`). Declaration ids are not referencePaths either, so the
second declaration keeps the old name.

**Executed:** `var a = 1; console.log(a); var a = 2; console.log(a)` +
rename `a → counter` → prints `1,1` instead of `1,2`. Duplicate
`for (var i ...)` loops and duplicate function declarations corrupt the
same way. All outputs re-parse cleanly.

**Exposure note:** the hole existed before in `applyModuleRename` (LLM
module bindings); the 2026-07-04 fast-rename unification widened it to
ALL rename paths. Babel's own renamer covers this case via its
`Declaration|VariableDeclarator` visitor. Bundler output rarely contains
same-scope duplicate declarations, but concatenated/legacy inputs do.

**Fix (~10 lines):** extend `getConstantViolationLHS` to handle
`VariableDeclarator` (→ `node.id`, may be a pattern) and
`FunctionDeclaration`/`ClassDeclaration` (→ `node.id`).

### C3. Phantom "external refs" + name-string voting rename unrelated bindings ⚡ (M)

`translatePriorNames` emits a pair for **every** placeholder slot —
including nested-function locals, member-expression property names,
object keys, and labels. `applyFunctionNameTransfers` (plugin.ts:496)
classifies every pair not in the function-owned binding map as an
`ExternalRefPair` with no check that the name resolves to any binding.
`propagateExternalReferences` then routes votes **by name string**
(`unmatchedModuleBindings.has(ref.oldName)`), and `getTopVote` lets a
single vote win.

**Executed:** matched pair `loadUsers(list){return list.map(user=>...)}`
vs `q(n){return n.map(e=>...)}` produces the phantom pair `{e → user}`.
Minified bundles have module bindings named `e`/`t`/`n` — one phantom
vote renames an unrelated module binding, marks it done, and removes it
from the graph so the LLM never revisits.

**Fix:** emit an external ref only when `fn.path.scope.getBinding(old)`
resolves to a binding OUTSIDE the function (drops properties/labels/
nested locals); key votes by binding identity, not name; consider a
≥2-vote floor. The closure branch already resolves bindings — the module
branch short-circuits before it.

### C4. Singleton hash-bucket matches with zero corroboration, on property-erased hashes ⚡ (S)

Function structural hashes normalize member property names away.
**Executed:** `(e)=>cache.get(e)` and `(t)=>registry.delete(t)` hash
IDENTICALLY. When each is its bucket's only member (1 old, 1 new),
`matchFunctions` (fingerprint-index.ts:425) matches immediately —
memberKey/shapes/shingle run only for multi-candidate buckets. A deleted
helper + an unrelated added helper auto-match, transfer the whole name
set, skip the LLM, and feed phantom votes (C3).

**Fix:** gate ALL exact matches (esp. singleton buckets) on cheap
corroboration that already exists: memberKey equality when defined,
overlap of `features.propertyAccesses`/`externalCalls`. Precision over
recall: reject on contradiction.

### C5. Cascade fallbacks discard earlier narrowing (S)

In `resolveMatch` (fingerprint-index.ts), when a stage's filter empties
the candidate set, the next stage falls back to a WIDER set — discarding
memberKey narrowing (line ~303) or ignoring an explicit memberKey
contradiction (line ~93). A candidate rejected by stronger evidence can
win at a weaker stage. Same pattern in propagation.ts:160/181/204.

**Fix:** treat an emptied filter as a contradiction → stop and mark
ambiguous; never fall back to a superset of what a stronger stage
rejected.

### C6. Close-match transfers apply names on cosine-0.8 count vectors (M)

`findCloseMatches` pairs functions on 12 generic count features; the
transfer path then RENAMES the function name + params (not merely
suggests) and excludes them from LLM re-rename. Deleted-helper +
added-helper pairs with similar shapes get permanently wrong names, and
the wrong prior code biases the LLM for the rest of the function.

**Fix:** require corroboration (shingle Jaccard / propertyAccesses
overlap) before applying; otherwise downgrade to `suggestedName`-style
hints (mechanism already exists for module bindings).

### C7. `matchFunctions` is not injective (S) — [also invariants I2]

A 2-old/1-new bucket matches BOTH old functions to the one new function
(`candidates.length === 1` short-circuits per old id).
`applyExactMatches` overwrites `renameMapping` last-writer-wins
(iteration-order-dependent names); stats double-count; propagation's
`reverseMatches` is maintained but never read. The binding path guards
this locally (`claimCounts`) — the function path, where precision
matters most, does not.

**Fix:** enforce injectivity inside `matchFunctions` before returning
(demote multi-claimed new ids to ambiguous); delete the binding-path
`claimCounts` hedge; use `reverseMatches` in `narrowCandidates`.

### C8. No eval/with guard (M)

Direct `eval` and `with` blocks make name-based renaming unsound
(runtime scope resolution by original name). No detection anywhere.
Real input class: Vue 2 render fns (`with(this){...}`), webpack
eval-devtool bundles.

**Fix:** at graph build, detect `WithStatement` / direct-eval sites and
mark bindings visible there ineligible.

### C9. Smaller confirmed items (S each)

- `extractVarNameRename` uses `declaratorPath.scope`; for a `var` inside
  a block the scope doesn't own the binding → silent missed transfer
  (recall only).
- Name-keyed dedupe in binding collectors: two same-named bindings in
  sibling blocks — only the first is renamed (cross-version noise).
- JSXIdentifier references would be silently skipped by
  `fastRenameBinding` — theoretical for compiled bundles; one-line
  branch closes it.
- Sequential LLM path skips the child-shadow check — moot if deleted
  (see D2).
- **Verified safe** ⚡: object shorthand (`{a}` → `{a: newName}`),
  shorthand destructuring, `a++`/`delete a`/`typeof a`, exports
  (guard → `scope.rename` → `export { counter as a }`), import
  specifiers, member-pattern elements.

---

## P1 — Invariants: establish at boundaries instead of hedging mid-run

### I1. Phantom graph edges: `graph.nodes.delete()` breaks the scheduler's invariant (M)

plugin.ts deletes matched binding nodes (and pre-done functions) from
`graph.nodes`, but `graph.dependencies`/`dependents` keep edges to the
deleted ids, and deleted BINDINGS are registered done nowhere (`preDone`
carries only functions). Every dependent of a matched binding becomes
permanently unsatisfiable and is released only by the Tier-2 deadlock
force-break — which dumps ALL blocked nodes unordered. In high-cache-hit
runs (the product's headline scenario) leaf-first ordering silently
evaporates for the LLM residue; the force-breaker masks it.

**Invariant:** every id in `dependencies ∪ dependents` is in
`graph.nodes ∪ doneIds` at `processUnified` entry. Stop deleting; mark
done and pass through (extend `preDone` to carry ids). Assert once at
entry; assert-log when the force-breaker fires so it becomes a signal.

### I2. Prior-version input contract (S)

`--prior-version` that is empty or unparseable silently yields
`emptyResult` → full-cost, zero-transfer run with no warning
(prior-version.ts:112/:125), while the MAIN input fails fast. No
wrong-program sanity check exists (`matchRate` computed, never gated).

**Fix:** parse the prior file at CLI load (pass the AST down, removing
the internal parse-or-empty fork); hard-error on parse failure;
warn-or-abort below a match-rate floor (<5-10% ⇒ "prior version does not
appear to be the same program").

### I3. Delete the Babel type shims; use real `Binding`/`Scope` types (M)

~27 defensive `?.()` calls exist because two hand-rolled duck types
erase Babel's guarantees (`BabelBinding` in function-graph.ts:5,
`ScopeLike`/`BindingLike` in validated-rename.ts). The dangerous ones
are in the safety validator itself: `scope.parent?.hasBinding?.(...)`
silently PASSES the capture check and `binding.path?.find?.(...)`
silently skips export detection when handed a shim. Tests should build
tiny real scopes via `parseSync` instead of object literals (most
already do). Nine near-identical shim types exist across the repo —
consolidate to one shared module or delete outright.

### I4. One node lifecycle instead of four "done" encodings (L)

"Finished" is encoded as: `FunctionNode.status`, processor's private
`done` set, `processUnified`'s `doneIds`, and `renameMapping` presence —
and `renameMapping` itself means four different things by context
(pending transfer / `{names:{}}` nothing-to-rename sentinel / LLM output
/ close-match claim guard). Replace with one explicit state machine
(`pending | transferred | llm-done | skipped | failed` with payloads),
transitions asserted at phase boundaries. Do this LAST — after the
deletions shrink the blast radius.

### I5. Name-keyed sentinels and sessionId grammars (S each)

- `module:${name}` sessionIds are keyed by a name the run renames;
  staleness is currently absorbed by luck + `no-binding` rejections that
  can't distinguish "already renamed" from "never existed". Return node
  identity from the matcher; treat sessionIds as opaque.
- `binding:${name}` fallback hashes must be remembered and filtered by
  every consumer (two do, redundantly). Make fingerprint
  `FunctionFingerprint | null` and partition matchable/unmatchable at
  graph build.
- THREE sessionId grammars exist (`file:line:col`, `file:pos:N`,
  `module:name`); propagation's `extractPosition` parses positionally
  and yields NaN for two of them, silently corrupting scope-ordinal
  sibling sorting — a precision risk inside the precision stage. Store
  `position: {line,column} | null` on nodes; delete the parser.
- `"[code generation failed]"` sentinel flows into LLM prompts as if it
  were code — drop the entry instead. `targetScope = null as unknown as
Scope` (function-graph.ts:945) launders an impossible state — throw.

### I6. Config resolved once (S)

`isEligible` is built properly once (plugin.ts:1073) but re-defaulted
argument-less at seven other sites — the fallback LOSES bundler/minifier
skip rules, so a future unthreaded caller silently gets weaker
eligibility. Same disease for `profiler` (9 sites) and `usedNames?`
(optional only for the dead `processAll` path). Resolve in one RunConfig
at the plugin boundary; make params required below it.

### I7. Silent catch triage (S)

Keep the legitimate ones (LLM JSON fallbacks, context-snippet codegen —
narrowed to just the `generate` call). Fix the load-bearing ones:
processor.ts:452/1438/1489 swallow ANY error in a function/batch task as
`_skipReasons.error++` + debug line — programming errors should fail the
run or at minimum surface in coverage + non-zero exit. Adjacent bug:
per-span `outcome` is computed from the GLOBAL failure counter, so every
span after the first failure reports "error".

### I8. Runtime assertions to add (S, high leverage)

1. Graph closure at `processUnified` entry (I1).
2. Match injectivity in `matchFunctions` (C7).
3. Placeholder-map alignment when hashes are equal (`translatePriorNames`).
4. Post-`fastRenameBinding` spot check: `scope.bindings[newName] ===
binding && !scope.bindings[oldName]` — catches NH-class bugs at the
   rename site, minutes before the parse gate.
5. `isValidRenameTarget` asserted INSIDE `fastRenameBinding` (defense
   against callers that skip validation, as the pre-fix transfer did).
6. Sentinel non-escape until I5 lands; single done-transition until I4.
7. Prior-match sanity floor (I2).

---

## P2 — Dead code & duplication (knip clean; these are test-pinned)

Production reaches the processor ONLY via `createRenamePlugin` →
`processUnified`. Everything below is proven unreachable from that
chain.

### D1. `processAll` + its entire scheduler (~500 lines) (M)

37 test call sites, zero production callers. Includes
`runProcessAllLoop`, `dispatchAllReady`, `breakDeadlocksAll`,
`buildDependentsMap`, the private `done` set, `ProcessorOptions.
onProgress`, `ProgressCallback`. It is the pre-unification loop kept
alive by its own tests.

### D2. Sequential LLM path + entire single-name provider surface (~585 lines) (M)

`processFunctionSequential`/`suggestNameWithRetry`/`getRejectionReason`
run only when `llm.suggestAllNames` is missing — all three production
provider layers implement it. With it goes `suggestName`,
`suggestFunctionName`, `retrySuggestName`, `retryFunctionName`,
`suggestNames` across types/openai-compatible/debug-wrapper/rate-limiter,
prompt builders in prompts.ts:9-154, `validateSuggestion`, and
`LLMContext.functionCode`. Make `suggestAllNames` REQUIRED on
`LLMProvider`. Bonus: removes today's footgun where a suggestName-only
provider silently skips ALL module bindings.

### D3. Truly dead (not even test-pinned) (S)

`paramOnly` + `getParamBindings` + pattern-param collectors
(processor.ts:3010-3073) — nobody sets the option, anywhere.

### D4. knip:prod triage (S)

Delete + their test blocks: `applyCachedNames` (obsolete cache-era API —
copies renameMapping without placeholder translation, wrong by design),
`computePathNgrams`, `findLeafFunctions`, `getProcessingOrder`,
`invertPlaceholderMapping`, `buildBindingPlaceholderMapping`,
`formatMetrics`, `formatMetricsCompact`, `extractSourceRange`,
`collectPositionalReferences`, `buildSimilarityGraph`,
`computeGraphDensity`, `createSemaphore`, whole file
`src/split/directory-grouping.ts`. Keep (harness-used, add to knip
entries): `findNewFunctions`, `getMatchStats`.

### D5. `byCalleeShapeKey` — production waste (S)

Built on EVERY fingerprint index (per-function `makeCalleeShapeKey`)
and never read for matching; readers are one test assertion and a
harness debug dump. Remove field + population.

### D6. Small leftovers (S)

`_MODULE_BATCH_SIZE`, `RenameDecision.fromCache`, `RenameMapping.model`,
`IdentifierOutcome "not-collected"` (+ dead branches in coverage.ts /
harness), stale `placeholderMapping` doc comment, `applyModuleRename`
one-line delegate, unused `_key`/`_doneIds` params. Package.json:
`test:llm` matches zero files (platform-dependent no-op); `download-ci-
model` invokes a command that does not exist; `test` vs `check` run
different "full" suites — reconcile.

### D7. Unify duplicates (S-M)

- Function-owned binding collectors: plugin.ts:379-479 vs
  processor.ts:2898-3094 — same traversal, one latent drift (named
  function-expression self-names handled only by processor's). Extract
  one `collectFunctionOwnedBindings` module; also breaks half the
  plugin↔processor circular import (move `getProximateUsedNames` out of
  plugin.ts for the other half).
- "function/class-expr init" predicate duplicated between
  `shouldSkipBinding` and `isMatchableBinding` — share the core test.
- Two propagation systems: KEEP BOTH (different stages — identity
  during matching vs name votes during application); rename the plugin
  one (`voteTransferExternalRefs`) and extract the ~700-line transfer
  block out of plugin.ts into its own module. `src/cache/` is a misnomer
  → `src/prior-version/`.

---

## P3 — Tests to fix

- **processor.test.ts (3,125 lines)** pins both dead paths (37
  `processAll` sites, suggestName-only mocks, an explicit "falls back to
  sequential" pin). Port unique scheduling assertions to
  `processUnified`, then delete those suites. Prerequisite for D1/D2.
- **rename.e2etest.ts** — the only e2e — exercises the sequential
  pipeline production never runs AND silently skips all module bindings.
  Rewrite mocks around `suggestAllNames`.
- **plugin.test.ts** default mock and **plugin-cross-version.test.ts**
  `countingProvider` are suggestName-only — the cross-version test's
  fresh legs measure the wrong pipeline. Add `suggestAllNames`.
- **prior-version-functional.test.ts** validates a hand-rolled
  `applyRenames` with its own (weaker) collision policy; the comment
  claims parity with plugin.ts and is wrong. Rewrite on
  `createRenamePlugin({priorVersionCode})`, keep the import-and-execute
  behavioral assertions (they are the best runtime-validation we have).

---

## Semantic validation passes (the "runnable output" guarantee)

The output parse gate is blind to C1-C4. Add cheap AST-level invariants
(pre vs post):

1. **Free-name set preservation** — the Program-scope `globals` key set
   must be identical before and after renaming. Catches capture (C1)
   and left-behind references (C2). O(n), and the post-crawl can reuse
   the parse-validation AST.
2. **Per-scope binding census** — binding count and per-binding
   reference+violation counts unchanged (position-keyed scopes).
3. **Rename bijection per scope** — no two old bindings map to one new
   name (defense-in-depth over target-in-scope).
4. **Reserved/builtin scan** of all output binding identifiers.

Longer-term (C10/strategic): scope-aware placeholder assignment (key by
resolved binding, not name string) — fixes the exact-match recall hole
where minifier name-reuse across sibling scopes (`e` twice) can never
match its humanified prior (verified hash mismatch). Changes all hashes;
schedule deliberately.

---

## Suggested sequencing

**Phase 1 — small precision/correctness fixes (all S, TDD each):**
C2 duplicate-declaration violations → C1 globals/free-name guard →
C7 injectivity (delete claimCounts hedge) → C5 stop-on-empty cascade →
I2 prior-input contract → C4 singleton corroboration gate →
assertions I8.4/I8.5 → semantic validation pass #1.

**Phase 2 — transfer-path precision (M):** C3 binding-identity votes,
C6 close-match downgrade-to-suggestion, I1 graph closure (stop deleting
nodes), I7 catch triage.

**Phase 3 — deletions & test retargeting (order matters):** D4/D3/D6
zero-risk deletions → P3 test retargeting → D1+D2 big cut
(`suggestAllNames` required) → D5, D7 unifications, I3 shim removal,
I6 RunConfig.

**Phase 4 — structural (L):** I4 lifecycle state machine, C8 eval/with
guard, C10 scope-aware placeholders (re-hash), remaining I5 items.

Estimated net: ~1,500-2,000 lines deleted, one provider interface, one
scheduler, materially higher precision on the paths that decide names.
